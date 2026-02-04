//! Tauri command handlers

use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};
use tracing::{debug, error, info};

use shared_protocol::{InputEvent, PeerId, QualityPreset};

use crate::session::{Session, SessionConfig};
use crate::state::AppState;

/// Error type for commands
#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("Session not found")]
    SessionNotFound,
    #[error("Invalid peer ID format")]
    InvalidPeerId,
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
    #[error("Session already exists")]
    SessionExists,
    #[error("Internal error: {0}")]
    Internal(String),
}

impl serde::Serialize for CommandError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

type CommandResult<T> = Result<T, CommandError>;

/// Get our peer ID for display
#[tauri::command]
pub fn get_peer_id(state: State<'_, Arc<AppState>>) -> String {
    state.display_id()
}

/// Session status response
#[derive(serde::Serialize)]
pub struct SessionStatus {
    pub state: String,
    pub remote_peer_id: Option<String>,
    pub rtt_ms: Option<f64>,
    pub fps: Option<f64>,
    pub bitrate_kbps: Option<u32>,
}

/// Start a new remote session
#[tauri::command]
pub async fn start_session(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    peer_id: String,
) -> CommandResult<SessionStatus> {
    info!("Starting session with peer: {}", peer_id);

    // Parse the peer ID
    let remote_peer_id = PeerId::from_display_string(&peer_id)
        .ok_or(CommandError::InvalidPeerId)?;

    // Check if session already exists
    if state.sessions.read().contains_key(&remote_peer_id) {
        return Err(CommandError::SessionExists);
    }

    // Create session config
    let config = SessionConfig {
        remote_peer_id,
        signaling_url: state.signaling_url.read().clone(),
        quality: QualityPreset::LowLatency,
    };

    // Create the session
    let session = Session::new(state.peer_id, config);
    
    // Connect first (awaiting this ensures the frontend knows when it's done)
    let mut active_session = session.connect().await
        .map_err(|e| CommandError::ConnectionFailed(e.to_string()))?;

    // Clone for the async block
    let app_clone = app.clone();
    
    // Start the streaming loop in the background
    let _session_handle = state.runtime.spawn(async move {
        active_session.run(move |event| {
            // Emit events to the frontend
            match event {
                SessionEvent::VideoFrame(data) => {
                    let _ = app_clone.emit("video-frame", data);
                }
                SessionEvent::StateChanged(new_state) => {
                    let _ = app_clone.emit("session-state", new_state);
                }
                SessionEvent::Stats { rtt_ms, fps, bitrate_kbps } => {
                    let _ = app_clone.emit("session-stats", serde_json::json!({
                        "rtt_ms": rtt_ms,
                        "fps": fps,
                        "bitrate_kbps": bitrate_kbps,
                    }));
                }
                SessionEvent::Error(err) => {
                    error!("Session error: {}", err);
                    let _ = app_clone.emit("session-error", err);
                }
            }
        }).await;
    });

    Ok(SessionStatus {
        state: "active".to_string(),
        remote_peer_id: Some(peer_id),
        rtt_ms: Some(0.0),
        fps: Some(0.0),
        bitrate_kbps: Some(0),
    })
}

/// Session events emitted to frontend
#[derive(Clone)]
pub enum SessionEvent {
    VideoFrame(Vec<u8>),
    StateChanged(String),
    Stats { rtt_ms: f64, fps: f64, bitrate_kbps: u32 },
    Error(String),
}

/// Stop the current session
#[tauri::command]
pub async fn stop_session(
    state: State<'_, Arc<AppState>>,
    peer_id: String,
) -> CommandResult<()> {
    info!("Stopping session with peer: {}", peer_id);

    let remote_peer_id = PeerId::from_display_string(&peer_id)
        .ok_or(CommandError::InvalidPeerId)?;

    if let Some(session) = state.remove_session(&remote_peer_id) {
        session.disconnect();
        Ok(())
    } else {
        Err(CommandError::SessionNotFound)
    }
}

/// Get the status of a session
#[tauri::command]
pub fn get_session_status(
    state: State<'_, Arc<AppState>>,
    peer_id: String,
) -> CommandResult<SessionStatus> {
    let remote_peer_id = PeerId::from_display_string(&peer_id)
        .ok_or(CommandError::InvalidPeerId)?;

    let sessions = state.sessions.read();
    let session = sessions.get(&remote_peer_id)
        .ok_or(CommandError::SessionNotFound)?;

    let stats = session.stats();
    
    Ok(SessionStatus {
        state: format!("{:?}", session.state()),
        remote_peer_id: Some(peer_id),
        rtt_ms: Some(stats.rtt_ms),
        fps: Some(stats.fps),
        bitrate_kbps: Some(stats.bitrate_kbps),
    })
}

/// Send an input event to the remote peer
#[tauri::command]
pub fn send_input(
    state: State<'_, Arc<AppState>>,
    peer_id: String,
    event: serde_json::Value,
) -> CommandResult<()> {
    debug!("Sending input to peer {}: {:?}", peer_id, event);

    let remote_peer_id = PeerId::from_display_string(&peer_id)
        .ok_or(CommandError::InvalidPeerId)?;

    let sessions = state.sessions.read();
    let session = sessions.get(&remote_peer_id)
        .ok_or(CommandError::SessionNotFound)?;

    // Parse and send the input event
    let input_event: InputEvent = serde_json::from_value(event)
        .map_err(|e| CommandError::Internal(e.to_string()))?;

    session.send_input(input_event)
        .map_err(|e| CommandError::Internal(e.to_string()))?;

    Ok(())
}

/// Request a keyframe from the remote encoder
#[tauri::command]
pub fn request_keyframe(
    state: State<'_, Arc<AppState>>,
    peer_id: String,
) -> CommandResult<()> {
    debug!("Requesting keyframe from peer: {}", peer_id);

    let remote_peer_id = PeerId::from_display_string(&peer_id)
        .ok_or(CommandError::InvalidPeerId)?;

    let sessions = state.sessions.read();
    let session = sessions.get(&remote_peer_id)
        .ok_or(CommandError::SessionNotFound)?;

    session.request_keyframe()
        .map_err(|e| CommandError::Internal(e.to_string()))?;

    Ok(())
}

/// Set quality preset
#[tauri::command]
pub fn set_quality(
    state: State<'_, Arc<AppState>>,
    peer_id: String,
    preset: String,
) -> CommandResult<()> {
    info!("Setting quality to {} for peer: {}", preset, peer_id);

    let remote_peer_id = PeerId::from_display_string(&peer_id)
        .ok_or(CommandError::InvalidPeerId)?;

    let quality = match preset.to_lowercase().as_str() {
        "low_latency" | "lowlatency" => QualityPreset::LowLatency,
        "balanced" => QualityPreset::Balanced,
        "high_quality" | "highquality" => QualityPreset::HighQuality,
        _ => return Err(CommandError::Internal("Invalid quality preset".to_string())),
    };

    let sessions = state.sessions.read();
    let session = sessions.get(&remote_peer_id)
        .ok_or(CommandError::SessionNotFound)?;

    session.set_quality(quality)
        .map_err(|e| CommandError::Internal(e.to_string()))?;

    Ok(())
}
