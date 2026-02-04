//! Session management - the hot path
//!
//! This module contains the main capture -> encode -> send loop.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossbeam_channel::{bounded, Receiver, Sender};
use parking_lot::{Mutex, RwLock};
use thiserror::Error;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use capture::{CaptureConfig, CapturedFrame};
use encoder::{EncodedFrame, EncoderConfig, VideoEncoder, OpenH264Encoder};
use shared_protocol::{
    InputEvent, InputPacket, PeerId, QualityPreset, SessionState,
};

/// Session error
#[derive(Debug, Error)]
pub enum SessionError {
    #[error("Connection failed: {0}")]
    Connection(String),
    #[error("Capture error: {0}")]
    Capture(String),
    #[error("Encoding error: {0}")]
    Encoding(String),
    #[error("Transport error: {0}")]
    Transport(String),
    #[error("Session not active")]
    NotActive,
    #[error("Channel error")]
    ChannelError,
}

pub type SessionResult<T> = Result<T, SessionError>;

/// Session configuration
pub struct SessionConfig {
    pub remote_peer_id: PeerId,
    pub signaling_url: String,
    pub quality: QualityPreset,
}

/// Session statistics
#[derive(Debug, Clone, Default)]
pub struct SessionStats {
    pub rtt_ms: f64,
    pub fps: f64,
    pub bitrate_kbps: u32,
    pub frames_sent: u64,
    pub bytes_sent: u64,
    pub packets_lost: u64,
}

/// Session state machine
pub struct Session {
    our_peer_id: PeerId,
    remote_peer_id: PeerId,
    config: SessionConfig,
    state: RwLock<SessionState>,
    stats: RwLock<SessionStats>,
    running: AtomicBool,
    input_tx: Mutex<Option<Sender<InputPacket>>>,
    input_sequence: AtomicU64,
}

impl Session {
    /// Create a new session
    pub fn new(our_peer_id: PeerId, config: SessionConfig) -> Self {
        Self {
            our_peer_id,
            remote_peer_id: config.remote_peer_id,
            config,
            state: RwLock::new(SessionState::Disconnected),
            stats: RwLock::new(SessionStats::default()),
            running: AtomicBool::new(false),
            input_tx: Mutex::new(None),
            input_sequence: AtomicU64::new(0),
        }
    }

    /// Get current session state
    pub fn state(&self) -> SessionState {
        *self.state.read()
    }

    /// Get current session stats
    pub fn stats(&self) -> SessionStats {
        self.stats.read().clone()
    }

    /// Connect to the remote peer
    pub async fn connect(self) -> SessionResult<ActiveSession> {
        info!("Connecting to peer: {}", self.remote_peer_id);
        
        *self.state.write() = SessionState::Connecting;

        // TODO: Connect to signaling server
        // TODO: Perform ICE/STUN for NAT traversal
        // TODO: Establish QUIC connection
        // TODO: Perform cryptographic handshake

        // For now, simulate connection setup
        *self.state.write() = SessionState::Handshaking;
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        *self.state.write() = SessionState::Active;
        self.running.store(true, Ordering::SeqCst);

        Ok(ActiveSession {
            session: Arc::new(self),
        })
    }

    /// Disconnect the session
    pub fn disconnect(&self) {
        info!("Disconnecting session with: {}", self.remote_peer_id);
        self.running.store(false, Ordering::SeqCst);
        *self.state.write() = SessionState::Ended;
    }

    /// Send an input event
    pub fn send_input(&self, event: InputEvent) -> SessionResult<()> {
        let tx = self.input_tx.lock();
        let tx = tx.as_ref().ok_or(SessionError::NotActive)?;

        let sequence = self.input_sequence.fetch_add(1, Ordering::Relaxed);
        let packet = InputPacket::new(sequence, event);

        tx.send(packet).map_err(|_| SessionError::ChannelError)?;
        Ok(())
    }

    /// Request a keyframe
    pub fn request_keyframe(&self) -> SessionResult<()> {
        // TODO: Send keyframe request over control channel
        debug!("Keyframe requested");
        Ok(())
    }

    /// Set quality preset
    pub fn set_quality(&self, _quality: QualityPreset) -> SessionResult<()> {
        // TODO: Update encoder settings
        debug!("Quality preset updated");
        Ok(())
    }
}

/// An active, connected session
pub struct ActiveSession {
    session: Arc<Session>,
}

impl ActiveSession {
    /// Run the session main loop
    ///
    /// This is the HOT PATH: Capture -> Diff -> Encode -> Send
    pub async fn run<F>(&mut self, event_callback: F)
    where
        F: Fn(crate::commands::SessionEvent) + Send + 'static,
    {
        info!("Starting session main loop");

        // Create channels
        let (input_tx, input_rx) = bounded::<InputPacket>(100);
        let (frame_tx, mut frame_rx) = mpsc::channel::<EncodedFrame>(10);

        // Notify frontend that we are now active
        event_callback(crate::commands::SessionEvent::StateChanged("Active".to_string()));

        // Store input sender
        *self.session.input_tx.lock() = Some(input_tx);

        // Spawn capture thread (CPU-intensive, runs on dedicated thread)
        let session_clone = self.session.clone();
        let frame_tx_clone = frame_tx.clone();
        
        std::thread::spawn(move || {
            if let Err(e) = Self::capture_loop(session_clone, frame_tx_clone) {
                error!("Capture loop error: {}", e);
            }
        });

        // Spawn input sender task
        let session_clone = self.session.clone();
        tokio::spawn(async move {
            Self::input_loop(session_clone, input_rx).await;
        });

        // Main event loop - send frames and report stats
        let mut frame_count = 0u64;
        let mut bytes_sent = 0u64;
        let start_time = Instant::now();
        let mut last_stats_time = Instant::now();

        while self.session.running.load(Ordering::SeqCst) {
            tokio::select! {
                Some(frame) = frame_rx.recv() => {
                    // Send frame to frontend and network
                    let frame_data = frame.data.to_vec();
                    event_callback(crate::commands::SessionEvent::VideoFrame(frame_data));
                    
                    frame_count += 1;
                    bytes_sent += frame.data.len() as u64;

                    // Update stats every second
                    if last_stats_time.elapsed() >= Duration::from_secs(1) {
                        let elapsed = start_time.elapsed().as_secs_f64();
                        let fps = frame_count as f64 / elapsed;
                        let bitrate = (bytes_sent * 8 / 1000) as u32 / elapsed.max(1.0) as u32;
                        
                        let mut stats = self.session.stats.write();
                        stats.fps = fps;
                        stats.bitrate_kbps = bitrate;
                        stats.frames_sent = frame_count;
                        stats.bytes_sent = bytes_sent;

                        event_callback(crate::commands::SessionEvent::Stats {
                            rtt_ms: stats.rtt_ms,
                            fps,
                            bitrate_kbps: bitrate,
                        });

                        last_stats_time = Instant::now();
                    }
                }
                _ = tokio::time::sleep(Duration::from_millis(100)) => {
                    // Periodic check
                }
            }
        }

        info!("Session main loop ended");
    }

    /// Capture and encode loop (runs on dedicated thread)
    fn capture_loop(
        session: Arc<Session>,
        frame_tx: mpsc::Sender<EncodedFrame>,
    ) -> SessionResult<()> {
        info!("Starting capture loop");

        // Initialize capture
        let mut capturer = capture::create_capture()
            .map_err(|e| SessionError::Capture(e.to_string()))?;

        let capture_config = CaptureConfig {
            display_id: None,
            target_fps: 30,
            dirty_rects: true,
            capture_cursor: true,
            capture_audio: false,
        };

        capturer.start(capture_config)
            .map_err(|e| SessionError::Capture(e.to_string()))?;

        // Initialize encoder
        let mut encoder = OpenH264Encoder::new();
        
        let displays = capturer.displays()
            .map_err(|e| SessionError::Capture(e.to_string()))?;
        
        let primary = displays.iter().find(|d| d.is_primary).unwrap_or(&displays[0]);
        
        let encoder_config = EncoderConfig {
            width: primary.width,
            height: primary.height,
            bitrate_kbps: 3000,
            fps: 30,
            keyframe_interval: 60,
            low_latency: true,
            ..Default::default()
        };

        encoder.init(encoder_config)
            .map_err(|e| SessionError::Encoding(e.to_string()))?;

        // Main capture loop
        let frame_duration = Duration::from_secs_f64(1.0 / 30.0);
        let mut last_frame = None::<CapturedFrame>;

        while session.running.load(Ordering::SeqCst) {
            let loop_start = Instant::now();

            // Capture frame
            match capturer.capture_frame() {
                Ok(frame) => {
                    // Optional: Check if frame changed (dirty rect optimization)
                    let should_encode = if let Some(ref _last) = last_frame {
                        // Only encode if something changed
                        !frame.dirty_rects.is_empty() || frame.dirty_percentage() > 0.5
                    } else {
                        true
                    };

                    if should_encode {
                        // Encode frame
                        match encoder.encode(&frame) {
                            Ok(encoded) => {
                                // Send to main loop
                                if frame_tx.blocking_send(encoded).is_err() {
                                    debug!("Frame channel closed, stopping capture");
                                    break;
                                }
                            }
                            Err(e) => {
                                warn!("Encoding error: {}", e);
                            }
                        }
                    }

                    last_frame = Some(frame);
                }
                Err(e) => {
                    warn!("Capture error: {}", e);
                }
            }

            // Rate limiting
            let elapsed = loop_start.elapsed();
            if elapsed < frame_duration {
                std::thread::sleep(frame_duration - elapsed);
            }
        }

        capturer.stop().ok();
        info!("Capture loop ended");
        
        Ok(())
    }

    /// Input sending loop
    async fn input_loop(session: Arc<Session>, input_rx: Receiver<InputPacket>) {
        info!("Starting input loop");

        while session.running.load(Ordering::SeqCst) {
            match input_rx.recv_timeout(Duration::from_millis(100)) {
                Ok(packet) => {
                    // TODO: Send over QUIC stream
                    debug!("Sending input packet: {:?}", packet.sequence);
                }
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                    // Continue
                }
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                    break;
                }
            }
        }

        info!("Input loop ended");
    }
}
