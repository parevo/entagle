//! Application state management

use parking_lot::RwLock;
use shared_protocol::{PeerId, SessionState};
use std::collections::HashMap;
use tokio::runtime::Runtime;

use crate::session::Session;

/// Application-wide state
pub struct AppState {
    /// Our peer ID
    pub peer_id: PeerId,
    /// Active sessions (we could have multiple in the future)
    pub sessions: RwLock<HashMap<PeerId, Session>>,
    /// Tokio runtime for async operations
    pub runtime: Runtime,
    /// Signaling server URL
    pub signaling_url: RwLock<String>,
}

impl AppState {
    pub fn new() -> Self {
        let runtime = Runtime::new().expect("Failed to create Tokio runtime");

        Self {
            peer_id: PeerId::new(),
            sessions: RwLock::new(HashMap::new()),
            runtime,
            signaling_url: RwLock::new("ws://localhost:8080/ws".to_string()),
        }
    }

    /// Get our display ID (formatted for UI)
    pub fn display_id(&self) -> String {
        self.peer_id.to_display_string()
    }

    /// Add a session
    pub fn add_session(&self, remote_peer_id: PeerId, session: Session) {
        self.sessions.write().insert(remote_peer_id, session);
    }

    /// Remove a session
    pub fn remove_session(&self, remote_peer_id: &PeerId) -> Option<Session> {
        self.sessions.write().remove(remote_peer_id)
    }

    /// Get session state
    pub fn session_state(&self, remote_peer_id: &PeerId) -> Option<SessionState> {
        self.sessions.read().get(remote_peer_id).map(|s| s.state())
    }

    /// Check if we have an active session with a peer
    pub fn has_active_session(&self, remote_peer_id: &PeerId) -> bool {
        self.sessions
            .read()
            .get(remote_peer_id)
            .map(|s| matches!(s.state(), SessionState::Active))
            .unwrap_or(false)
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
