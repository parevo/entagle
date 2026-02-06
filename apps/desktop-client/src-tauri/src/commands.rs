//! Tauri command handlers

use std::process::Command;
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

#[derive(serde::Serialize)]
pub struct PermissionStatus {
    pub screen_recording: bool,
    pub accessibility: bool,
}

#[tauri::command]
pub fn get_permissions() -> PermissionStatus {
    let screen_recording = capture::has_screen_recording_permission();
    let accessibility = input_injector::create_injector()
        .map(|injector| injector.has_permission())
        .unwrap_or(false);

    PermissionStatus {
        screen_recording,
        accessibility,
    }
}

#[tauri::command]
pub fn request_permissions() -> PermissionStatus {
    let screen_recording = capture::request_screen_recording_permission();
    let accessibility = input_injector::create_injector()
        .and_then(|injector| injector.request_permission())
        .unwrap_or(false);

    PermissionStatus {
        screen_recording,
        accessibility,
    }
}

#[tauri::command]
pub fn open_permission_settings(kind: String) -> CommandResult<()> {
    #[cfg(target_os = "macos")]
    {
        let url = match kind.as_str() {
            "screen_recording" | "screen" => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture"
            }
            "accessibility" => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
            }
            _ => {
                return Err(CommandError::Internal(format!(
                    "Unknown permission kind: {}",
                    kind
                )));
            }
        };

        Command::new("open")
            .arg(url)
            .spawn()
            .map_err(|e| CommandError::Internal(e.to_string()))?;
    }

    Ok(())
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
    role: Option<String>,
) -> CommandResult<SessionStatus> {
    info!("Starting session with peer: {} (role: {:?})", peer_id, role);

    // Parse session role
    let session_role = match role.as_deref() {
        Some("viewer") | Some("Viewer") => shared_protocol::SessionRole::Viewer,
        _ => shared_protocol::SessionRole::Host,
    };
    // Parse the peer ID
    let remote_peer_id = if session_role == shared_protocol::SessionRole::Host {
        // For Host, the remote ID is dynamic/unknown initially.
        // If provided, use it (maybe for verification later?), but if empty/invalid, generate a placeholder.
        PeerId::from_display_string(&peer_id).unwrap_or_else(PeerId::new)
    } else {
        // // For Viewer, we MUST have a valid target ID.
        PeerId::from_display_string(&peer_id).ok_or(CommandError::InvalidPeerId)?
    };

    // Check if session already exists
    if state.sessions.read().contains_key(&remote_peer_id) {
        // If we generated a random ID, this is unlikely.
        // If user provided an ID, it might exist.
        return Err(CommandError::SessionExists);
    }

    // Create session config
    let config = SessionConfig {
        remote_peer_id,
        signaling_url: state.signaling_url.read().clone(),
        quality: QualityPreset::LowLatency,
        role: session_role,
    };

    // Create the session
    let session = Arc::new(Session::new(state.peer_id, config));

    // Store session immediately so it can be stopped while listening (host mode).
    state.add_session(remote_peer_id, session.clone());

    // Clone for the async block
    let app_clone = app.clone();

    match session_role {
        shared_protocol::SessionRole::Host => {
            let session_clone = session.clone();
            let _session_handle = state.runtime.spawn(async move {
                let app_for_notify = app_clone.clone();
                let notify = Arc::new(move |event: SessionEvent| {
                    emit_session_event(&app_for_notify, event);
                });
                match session_clone.connect(Some(notify)).await {
                    Ok(mut active_session) => {
                        let app_for_run = app_clone.clone();
                        active_session
                            .run(move |event| {
                                emit_session_event(&app_for_run, event);
                            })
                            .await;
                    }
                    Err(e) => {
                        error!("Host connect failed: {}", e);
                        let _ = app_clone.emit("session-error", e.to_string());
                        let _ = app_clone.emit("session-state", "failed");
                    }
                }
            });

            Ok(SessionStatus {
                state: "listening".to_string(),
                remote_peer_id: Some(remote_peer_id.to_display_string()),
                rtt_ms: Some(0.0),
                fps: Some(0.0),
                bitrate_kbps: Some(0),
            })
        }
        shared_protocol::SessionRole::Viewer => {
            // Viewer connects immediately and returns active status
            let mut active_session = session
                .connect(None)
                .await
                .map_err(|e| CommandError::ConnectionFailed(e.to_string()))?;

            let _session_handle = state.runtime.spawn(async move {
                active_session
                    .run(move |event| {
                        emit_session_event(&app_clone, event);
                    })
                    .await;
            });

            Ok(SessionStatus {
                state: "active".to_string(),
                remote_peer_id: Some(remote_peer_id.to_display_string()),
                rtt_ms: Some(0.0),
                fps: Some(0.0),
                bitrate_kbps: Some(0),
            })
        }
    }
}

/// Video frame event data
#[derive(Clone, serde::Serialize)]
pub struct VideoFrameEvent {
    pub data: Vec<u8>,
    pub is_keyframe: bool,
    pub timestamp_us: u64,
    pub width: u32,
    pub height: u32,
    pub frame_id: u64,
}

/// Session events emitted to frontend
#[derive(Clone, serde::Serialize)]
pub enum SessionEvent {
    VideoFrame(VideoFrameEvent),
    StateChanged(String),
    IncomingConnection { from_peer_id: String },
    Stats {
        rtt_ms: f64,
        fps: f64,
        bitrate_kbps: u32,
    },
    Error(String),
}

/// Stop the current session
#[tauri::command]
pub async fn stop_session(state: State<'_, Arc<AppState>>, peer_id: String) -> CommandResult<()> {
    info!("Stopping session with peer: {}", peer_id);

    let remote_peer_id =
        PeerId::from_display_string(&peer_id).ok_or(CommandError::InvalidPeerId)?;

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
    let remote_peer_id =
        PeerId::from_display_string(&peer_id).ok_or(CommandError::InvalidPeerId)?;

    let sessions = state.sessions.read();
    let session = sessions
        .get(&remote_peer_id)
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

    let remote_peer_id =
        PeerId::from_display_string(&peer_id).ok_or(CommandError::InvalidPeerId)?;

    let sessions = state.sessions.read();
    let session = sessions
        .get(&remote_peer_id)
        .ok_or(CommandError::SessionNotFound)?;

    // Parse and send the input event
    let input_event: InputEvent =
        serde_json::from_value(event).map_err(|e| CommandError::Internal(e.to_string()))?;

    session
        .send_input(input_event)
        .map_err(|e| CommandError::Internal(e.to_string()))?;

    Ok(())
}

/// Request a keyframe from the remote encoder
#[tauri::command]
pub fn request_keyframe(state: State<'_, Arc<AppState>>, peer_id: String) -> CommandResult<()> {
    debug!("Requesting keyframe from peer: {}", peer_id);

    let remote_peer_id =
        PeerId::from_display_string(&peer_id).ok_or(CommandError::InvalidPeerId)?;

    let sessions = state.sessions.read();
    let session = sessions
        .get(&remote_peer_id)
        .ok_or(CommandError::SessionNotFound)?;

    session
        .request_keyframe()
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

    let remote_peer_id =
        PeerId::from_display_string(&peer_id).ok_or(CommandError::InvalidPeerId)?;

    let quality = match preset.to_lowercase().as_str() {
        "low_latency" | "lowlatency" => QualityPreset::LowLatency,
        "balanced" => QualityPreset::Balanced,
        "high_quality" | "highquality" => QualityPreset::HighQuality,
        _ => return Err(CommandError::Internal("Invalid quality preset".to_string())),
    };

    let sessions = state.sessions.read();
    let session = sessions
        .get(&remote_peer_id)
        .ok_or(CommandError::SessionNotFound)?;

    session
        .set_quality(quality)
        .map_err(|e| CommandError::Internal(e.to_string()))?;

    Ok(())
}

fn emit_session_event(app: &AppHandle, event: SessionEvent) {
    match event {
        SessionEvent::VideoFrame(event) => {
            let _ = app.emit("video-frame", event);
        }
        SessionEvent::StateChanged(new_state) => {
            let _ = app.emit("session-state", new_state);
        }
        SessionEvent::IncomingConnection { from_peer_id } => {
            let _ = app.emit("incoming-connection", from_peer_id);
        }
        SessionEvent::Stats {
            rtt_ms,
            fps,
            bitrate_kbps,
        } => {
            let _ = app.emit(
                "session-stats",
                serde_json::json!({
                    "rtt_ms": rtt_ms,
                    "fps": fps,
                    "bitrate_kbps": bitrate_kbps,
                }),
            );
        }
        SessionEvent::Error(err) => {
            error!("Session error: {}", err);
            let _ = app.emit("session-error", err);
        }
    }
}

/// Accept an incoming connection request (host)
#[tauri::command]
pub fn accept_connection(
    state: State<'_, Arc<AppState>>,
    from_peer_id: String,
) -> CommandResult<()> {
    let from_peer_id =
        PeerId::from_display_string(&from_peer_id).ok_or(CommandError::InvalidPeerId)?;

    let sessions = state.sessions.read();
    let session = sessions.values().find(|s| s.has_pending_connection());
    if session.is_none() {
        error!("accept_connection: no pending session");
    }
    let session = session.ok_or(CommandError::SessionNotFound)?;

    if let Err(e) = session.resolve_pending_connection(from_peer_id, true) {
        error!("accept_connection resolve failed: {}", e);
        return Err(CommandError::Internal(e.to_string()));
    }

    Ok(())
}

/// Reject an incoming connection request (host)
#[tauri::command]
pub fn reject_connection(
    state: State<'_, Arc<AppState>>,
    from_peer_id: String,
    reason: String,
) -> CommandResult<()> {
    let from_peer_id =
        PeerId::from_display_string(&from_peer_id).ok_or(CommandError::InvalidPeerId)?;

    let sessions = state.sessions.read();
    let session = sessions.values().find(|s| s.has_pending_connection());
    if session.is_none() {
        error!("reject_connection: no pending session");
    }
    let session = session.ok_or(CommandError::SessionNotFound)?;

    let _ = reason;
    if let Err(e) = session.resolve_pending_connection(from_peer_id, false) {
        error!("reject_connection resolve failed: {}", e);
        return Err(CommandError::Internal(e.to_string()));
    }

    Ok(())
}
