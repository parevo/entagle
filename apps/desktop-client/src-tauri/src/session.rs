//! Session management - the hot path
//!
//! This module contains the main capture -> encode -> send loop.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender, bounded};
use parking_lot::{Mutex, RwLock};
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, error, info, warn};

use crate::signaling::SignalingClient;
use capture::{CaptureConfig, CapturedFrame};
use encoder::{EncodedFrame, EncoderConfig, OpenH264Encoder, VideoEncoder};
use input_injector::{InputProcessor, create_injector};
use net_transport::QuicTransport;
use shared_protocol::{
    FrameType, IceCandidate, IceCandidateType, InputEvent, InputPacket, PeerId, QualityPreset,
    SessionRole, SessionState, SignalingMessage, VideoCodec, VideoPacket, VideoPacketHeader,
    MAX_DATAGRAM_SIZE,
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
    pub role: SessionRole,
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

/// Reassembles fragmented video frames
struct FrameAssembler {
    frames: HashMap<u64, FrameAssembly>,
    max_frames: usize,
    max_age: Duration,
}

struct FrameAssembly {
    header: VideoPacketHeader,
    fragments: Vec<Option<Vec<u8>>>,
    received: usize,
    last_update: Instant,
}

impl FrameAssembler {
    fn new(max_frames: usize, max_age: Duration) -> Self {
        Self {
            frames: HashMap::new(),
            max_frames,
            max_age,
        }
    }

    fn push(&mut self, packet: VideoPacket) -> Option<VideoPacket> {
        if packet.header.total_fragments <= 1 {
            return Some(packet);
        }

        self.evict_old();

        let frame_id = packet.header.frame_id;
        let total = packet.header.total_fragments as usize;

        let entry = self.frames.entry(frame_id).or_insert_with(|| FrameAssembly {
            header: packet.header.clone(),
            fragments: vec![None; total],
            received: 0,
            last_update: Instant::now(),
        });

        if entry.fragments.len() != total {
            *entry = FrameAssembly {
                header: packet.header.clone(),
                fragments: vec![None; total],
                received: 0,
                last_update: Instant::now(),
            };
        }

        let idx = packet.header.fragment_index as usize;
        if idx < total && entry.fragments[idx].is_none() {
            entry.fragments[idx] = Some(packet.payload);
            entry.received += 1;
            entry.last_update = Instant::now();
        }

        if entry.received == total {
            let mut payload = Vec::new();
            for fragment in entry.fragments.iter_mut() {
                if let Some(data) = fragment.take() {
                    payload.extend_from_slice(&data);
                }
            }
            let header = entry.header.clone();
            self.frames.remove(&frame_id);
            return Some(VideoPacket { header, payload });
        }

        None
    }

    fn evict_old(&mut self) {
        let cutoff = Instant::now() - self.max_age;
        self.frames.retain(|_, frame| frame.last_update >= cutoff);

        if self.frames.len() > self.max_frames {
            let mut entries: Vec<(u64, Instant)> = self
                .frames
                .iter()
                .map(|(id, frame)| (*id, frame.last_update))
                .collect();
            entries.sort_by_key(|(_, t)| *t);
            let to_remove = entries.len().saturating_sub(self.max_frames);
            for (frame_id, _) in entries.into_iter().take(to_remove) {
                self.frames.remove(&frame_id);
            }
        }
    }
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
    pending_connection: Mutex<Option<PendingConnection>>,
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
            pending_connection: Mutex::new(None),
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
    pub async fn connect(
        self: Arc<Self>,
        notify: Option<Arc<dyn Fn(crate::commands::SessionEvent) + Send + Sync>>,
    ) -> SessionResult<ActiveSession> {
        info!("Connecting to peer: {}", self.remote_peer_id);
        *self.state.write() = SessionState::Connecting;

        // 1. Connect to Signaling Server
        let signaling = SignalingClient::new(self.our_peer_id, self.config.signaling_url.clone());
        let (signal_tx, mut signal_rx) = signaling
            .connect()
            .await
            .map_err(|e| SessionError::Connection(format!("Signaling failed: {}", e)))?;

        // 2. Wait for Registration
        info!("Waiting for signaling registration...");
        loop {
            match tokio::time::timeout(Duration::from_secs(5), signal_rx.recv()).await {
                Ok(Some(SignalingMessage::Registered { peer_id })) => {
                    if peer_id == self.our_peer_id {
                        info!("Registered with signaling server");
                        break;
                    }
                }
                Ok(Some(SignalingMessage::Error { message })) => {
                    return Err(SessionError::Connection(format!(
                        "Signaling error: {}",
                        message
                    )));
                }
                Ok(None) => return Err(SessionError::Connection("Signaling closed".into())),
                Err(_) => return Err(SessionError::Connection("Registration timeout".into())),
                _ => {}
            }
        }

        let transport: QuicTransport;

        match self.config.role {
            SessionRole::Host => {
                // === HOST ROLE (Server) ===
                info!("Initializing Host transport...");

                // Bind to random port
                let bind_addr = "0.0.0.0:0".parse().unwrap();
                transport = QuicTransport::new_server(bind_addr)
                    .await
                    .map_err(|e| SessionError::Transport(e.to_string()))?;

                // Determine our local IP to share (MVP: use local_ip_crate or simplified)
                // For MVP, we'll try to get the local IP matching the signaling URL or default
                let local_ip = local_ip_address::local_ip().unwrap_or("127.0.0.1".parse().unwrap());
                let local_port = transport.local_addr().map(|s| s.port()).unwrap_or(4433);

                info!("Host initialized. Waiting for incoming connection request...");

                // Wait for Connection Request from Viewer
                let viewer_peer_id: PeerId;
                loop {
                    match tokio::time::timeout(Duration::from_secs(60), signal_rx.recv()).await {
                        Ok(Some(SignalingMessage::IncomingConnection { from_peer_id })) => {
                            info!("Received connection request from: {}", from_peer_id);
                            if let Some(cb) = &notify {
                                cb(crate::commands::SessionEvent::IncomingConnection {
                                    from_peer_id: from_peer_id.to_display_string(),
                                });
                            }

                            let (tx, rx) = oneshot::channel::<bool>();
                            *self.pending_connection.lock() = Some(PendingConnection {
                                from_peer_id,
                                response: tx,
                            });

                            let approved = match tokio::time::timeout(
                                Duration::from_secs(120),
                                rx,
                            )
                            .await
                            {
                                Ok(Ok(value)) => value,
                                _ => false,
                            };

                            self.pending_connection.lock().take();

                            if approved {
                                viewer_peer_id = from_peer_id;
                                // Notify signaling server about acceptance
                                signal_tx
                                    .send(SignalingMessage::Accept {
                                        from_peer_id: viewer_peer_id,
                                    })
                                    .await
                                    .map_err(|_| SessionError::ChannelError)?;
                                break;
                            } else {
                                signal_tx
                                    .send(SignalingMessage::Reject {
                                        from_peer_id,
                                        reason: "User rejected request".to_string(),
                                    })
                                    .await
                                    .map_err(|_| SessionError::ChannelError)?;
                                // Continue waiting for another request
                                continue;
                            }
                        }
                        Ok(Some(SignalingMessage::Error { message })) => {
                            warn!("Signaling error: {}", message);
                        }
                        Ok(None) => {
                            return Err(SessionError::Connection("Signaling closed".into()));
                        }
                        Err(_) => {
                            // Keep waiting
                            debug!("Waiting for connection...");
                        }
                        _ => {}
                    }
                }

                // Create and Send Candidate
                let candidate_addr = IceCandidate {
                    address: local_ip.to_string(),
                    port: local_port,
                    candidate_type: IceCandidateType::Host,
                    priority: 1,
                };

                info!(
                    "Sending candidate to viewer: {:?} ({})",
                    candidate_addr, viewer_peer_id
                );

                signal_tx
                    .send(SignalingMessage::IceCandidate {
                        target_peer_id: viewer_peer_id,
                        candidate: candidate_addr,
                    })
                    .await
                    .map_err(|_| SessionError::ChannelError)?;

                // Accept QUIC connection
                info!("Accepting QUIC connection...");
                transport
                    .accept()
                    .await
                    .map_err(|e| SessionError::Connection(e.to_string()))?;
            }
            SessionRole::Viewer => {
                // === VIEWER ROLE (Client) ===
                info!("Initializing Viewer transport...");

                // Bind to random port
                let bind_addr = "0.0.0.0:0".parse().unwrap();
                transport = QuicTransport::new_client(bind_addr)
                    .await
                    .map_err(|e| SessionError::Transport(e.to_string()))?;

                info!("Requesting connection to Host: {}", self.remote_peer_id);

                // Send Connect Request
                signal_tx
                    .send(SignalingMessage::Connect {
                        target_peer_id: self.remote_peer_id,
                    })
                    .await
                    .map_err(|_| SessionError::ChannelError)?;

                // Wait for Candidate
                info!("Viewer waiting for candidate...");
                let remote_addr: std::net::SocketAddr;
                loop {
                    match tokio::time::timeout(Duration::from_secs(120), signal_rx.recv()).await {
                        Ok(Some(SignalingMessage::IceCandidate {
                            target_peer_id,
                            candidate,
                        })) => {
                            if target_peer_id == self.remote_peer_id {
                                info!(
                                    "Received candidate: {}:{}",
                                    candidate.address, candidate.port
                                );
                                remote_addr = format!("{}:{}", candidate.address, candidate.port)
                                    .parse()
                                    .map_err(|_| {
                                        SessionError::Connection("Invalid remote address".into())
                                    })?;
                                break;
                            }
                        }
                        Ok(Some(SignalingMessage::Error { message })) => {
                            if message.contains("queued") {
                                info!("Request queued (Host offline), waiting...");
                                // Do not exit, keep waiting for candidate
                            } else {
                                return Err(SessionError::Connection(format!(
                                    "Remote error: {}",
                                    message
                                )));
                            }
                        }
                        Ok(None) => {
                            return Err(SessionError::Connection("Signaling closed".into()));
                        }
                        Err(_) => return Err(SessionError::Connection("Candidate timeout".into())),
                        _ => {}
                    }
                }

                // Connect
                info!("Viewer connecting to {}", remote_addr);
                transport
                    .connect(remote_addr, "entangle.local")
                    .await
                    .map_err(|e| SessionError::Connection(e.to_string()))?;
            }
        }

        info!("Transport established!");
        *self.state.write() = SessionState::Active;
        self.running.store(true, Ordering::SeqCst);

        Ok(ActiveSession {
            session: self,
            transport: Arc::new(transport),
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

    pub fn has_pending_connection(&self) -> bool {
        let has = self.pending_connection.lock().is_some();
        debug!("has_pending_connection: {}", has);
        has
    }

    pub fn resolve_pending_connection(
        &self,
        from_peer_id: PeerId,
        approved: bool,
    ) -> SessionResult<()> {
        let mut pending = self.pending_connection.lock();
        if let Some(p) = pending.take() {
            if p.from_peer_id != from_peer_id {
                *pending = Some(p);
                return Err(SessionError::Connection("Mismatched peer id".into()));
            }
            let _ = p.response.send(approved);
            Ok(())
        } else {
            Err(SessionError::NotActive)
        }
    }
}

/// An active, connected session
pub struct ActiveSession {
    session: Arc<Session>,
    transport: Arc<QuicTransport>,
}

struct PendingConnection {
    from_peer_id: PeerId,
    response: oneshot::Sender<bool>,
}

impl ActiveSession {
    /// Get the persistent session
    pub fn session(&self) -> Arc<Session> {
        self.session.clone()
    }

    /// Run the session main loop
    ///
    /// This is the HOT PATH: Capture -> Diff -> Encode -> Send
    /// Run the session main loop
    ///
    /// This is the HOT PATH: Capture -> Diff -> Encode -> Send (Host)
    /// OR Receive -> Decode -> Render (Viewer)
    pub async fn run<F>(&mut self, event_callback: F)
    where
        F: Fn(crate::commands::SessionEvent) + Send + 'static,
    {
        info!(
            "Starting session main loop with role: {:?}",
            self.session.config.role
        );

        // Notify frontend that we are now active
        event_callback(crate::commands::SessionEvent::StateChanged(
            "Active".to_string(),
        ));

        match self.session.config.role {
            SessionRole::Host => self.run_host(event_callback).await,
            SessionRole::Viewer => self.run_viewer(event_callback).await,
        }

        self.session.disconnect();
        info!("Session main loop ended");
    }

    /// Host loop: Capture -> Send Video; Receive Input -> Inject
    async fn run_host<F>(&mut self, event_callback: F)
    where
        F: Fn(crate::commands::SessionEvent) + Send + 'static,
    {
        // 1. Start Capture (Producer)
        let (frame_tx, mut frame_rx) = mpsc::channel::<EncodedFrame>(10);
        let session_clone = self.session.clone();

        std::thread::spawn(move || {
            if let Err(e) = Self::capture_loop(session_clone, frame_tx) {
                error!("Capture loop error: {}", e);
            }
        });

        // Initialize Input Injector
        let mut input_processor = match create_injector() {
            Ok(injector) => match InputProcessor::new(injector) {
                Ok(processor) => Some(processor),
                Err(e) => {
                    error!("Failed to create InputProcessor: {}", e);
                    None
                }
            },
            Err(e) => {
                error!("Failed to create InputInjector: {}", e);
                None
            }
        };

        // 2. Main Loop: Send Video & Receive Input
        let mut last_stats_time = Instant::now();
        let start_time = Instant::now();
        let mut frame_count = 0u64;
        let mut bytes_sent = 0u64;

        loop {
            if !self.session.running.load(Ordering::SeqCst) {
                break;
            }

            tokio::select! {
                // Outgoing Video
                Some(frame) = frame_rx.recv() => {
                    let frame_len = frame.data.len();

                    // Create Video Packet
                    let header = VideoPacketHeader {
                        frame_id: frame.sequence,
                        fragment_index: 0,
                        total_fragments: 1,
                        timestamp_us: frame.pts_us,
                        frame_type: match frame.frame_type {
                            encoder::EncodedFrameType::Key => FrameType::Key,
                            _ => FrameType::Delta,
                        },
                        codec: VideoCodec::H264, // Assume H.264 for MVP
                        width: frame.width,
                        height: frame.height,
                        dirty_rect: None,
                    };

                    let header_size = match bincode::serialized_size(&VideoPacket {
                        header: header.clone(),
                        payload: Vec::new(),
                    }) {
                        Ok(size) => size as usize,
                        Err(e) => {
                            error!("Failed to compute header size: {}", e);
                            continue;
                        }
                    };

                    let max_payload = MAX_DATAGRAM_SIZE.saturating_sub(header_size);
                    if max_payload == 0 {
                        warn!("Header too large for datagram");
                        continue;
                    }

                    let total_fragments =
                        ((frame_len + max_payload - 1) / max_payload) as u16;

                    let mut sent_all = true;
                    for (index, chunk) in frame.data.chunks(max_payload).enumerate() {
                        let mut fragment_header = header.clone();
                        fragment_header.fragment_index = index as u16;
                        fragment_header.total_fragments = total_fragments;

                        let packet = VideoPacket {
                            header: fragment_header,
                            payload: chunk.to_vec(),
                        };

                        match packet.to_bytes() {
                            Ok(bytes) => {
                                if let Err(e) = self.transport.send_datagram(bytes) {
                                    warn!("Failed to send frame fragment: {}", e);
                                    sent_all = false;
                                    break;
                                }
                            }
                            Err(e) => {
                                error!("Failed to serialize video packet: {}", e);
                                sent_all = false;
                                break;
                            }
                        }
                    }

                    if sent_all {
                        frame_count += 1;
                        bytes_sent += frame_len as u64;
                    }

                    // Stats logic
                    if last_stats_time.elapsed() >= Duration::from_secs(1) {
                         let elapsed = start_time.elapsed().as_secs_f64();
                         let fps = frame_count as f64 / elapsed;
                         let bitrate = ((bytes_sent as f64 * 8.0) / 1000.0 / elapsed.max(1.0)) as u32;

                         // Emit stats locally
                         event_callback(crate::commands::SessionEvent::Stats {
                             rtt_ms: 0.0, // Host doesn't easily measure RTT unless we add PING
                             fps,
                             bitrate_kbps: bitrate,
                         });
                         last_stats_time = Instant::now();
                    }
                }

                // Incoming Input (via Datagrams for MVP, or Streams)
                Ok(data) = self.transport.recv_datagram() => {
                    // Try to deserialize as InputPacket
                    if let Ok(packet) = bincode::deserialize::<InputPacket>(&data) {
                        debug!("Host received input: {:?}", packet.event);

                        if let Some(processor) = &mut input_processor {
                            if let Err(e) = processor.process_packet(&packet) {
                                warn!("Input injection failed: {}", e);
                            }
                        }
                    }
                }

                else => break,
            }
        }
    }

    /// Viewer loop: Receive Video -> Emit; Send Input -> Network
    async fn run_viewer<F>(&mut self, event_callback: F)
    where
        F: Fn(crate::commands::SessionEvent) + Send + 'static,
    {
        // 1. Start Input Sender
        let (input_tx, input_rx) = bounded::<InputPacket>(100);
        *self.session.input_tx.lock() = Some(input_tx);

        let session_clone = self.session.clone();
        let transport_clone = self.transport.clone();
        tokio::spawn(async move {
            Self::input_loop(session_clone, transport_clone, input_rx).await;
        });

        // 2. Receive Video Loop
        let mut assembler = FrameAssembler::new(128, Duration::from_secs(2));
        loop {
            if !self.session.running.load(Ordering::SeqCst) {
                break;
            }

            match self.transport.recv_datagram().await {
                Ok(data) => {
                    // Deserialize Video Packet
                    match VideoPacket::from_bytes(&data) {
                        Ok(packet) => {
                            if let Some(packet) = assembler.push(packet) {
                                let is_keyframe =
                                    matches!(packet.header.frame_type, FrameType::Key);
                                let event = crate::commands::VideoFrameEvent {
                                    data: packet.payload,
                                    is_keyframe,
                                    timestamp_us: packet.header.timestamp_us,
                                    width: packet.header.width,
                                    height: packet.header.height,
                                    frame_id: packet.header.frame_id,
                                };

                                event_callback(crate::commands::SessionEvent::VideoFrame(event));
                            }
                        }
                        Err(e) => {
                            warn!("Failed to deserialize packet: {}", e);
                        }
                    }
                }
                Err(_) => {
                    // Connection closed or error
                    break;
                }
            }
        }
    }

    /// Capture and encode loop (runs on dedicated thread)
    fn capture_loop(
        session: Arc<Session>,
        frame_tx: mpsc::Sender<EncodedFrame>,
    ) -> SessionResult<()> {
        info!("Starting capture loop");

        // Initialize capture
        let mut capturer =
            capture::create_capture().map_err(|e| SessionError::Capture(e.to_string()))?;

        let capture_config = CaptureConfig {
            display_id: None,
            target_fps: 30,
            dirty_rects: true,
            capture_cursor: true,
            capture_audio: false,
        };

        capturer
            .start(capture_config)
            .map_err(|e| SessionError::Capture(e.to_string()))?;

        // Initialize encoder
        let mut encoder = OpenH264Encoder::new();

        let displays = capturer
            .displays()
            .map_err(|e| SessionError::Capture(e.to_string()))?;

        let primary = displays
            .iter()
            .find(|d| d.is_primary)
            .unwrap_or(&displays[0]);

        let encoder_config = EncoderConfig {
            width: primary.width,
            height: primary.height,
            bitrate_kbps: 3000,
            fps: 30,
            keyframe_interval: 60,
            low_latency: true,
            ..Default::default()
        };

        encoder
            .init(encoder_config)
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
                        !frame.dirty_rects.is_empty() || frame.dirty_percentage() > 0.01
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
                                if e.to_string().contains("Empty bitstream") {
                                    encoder.force_keyframe();
                                    debug!("Encoder returned empty bitstream, retrying keyframe");
                                } else {
                                    warn!("Encoding error: {}", e);
                                }
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
    async fn input_loop(
        session: Arc<Session>,
        transport: Arc<QuicTransport>,
        input_rx: Receiver<InputPacket>,
    ) {
        info!("Starting input loop");

        while session.running.load(Ordering::SeqCst) {
            match input_rx.recv_timeout(Duration::from_millis(100)) {
                Ok(packet) => {
                    if transport.is_connected() {
                        // Serialize packet and send over a stream or datagram
                        if let Ok(data) = bincode::serialize(&packet) {
                            if let Err(e) = transport.send_datagram(data.into()) {
                                warn!("Failed to send input: {}", e);
                            }
                        }
                    }
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
