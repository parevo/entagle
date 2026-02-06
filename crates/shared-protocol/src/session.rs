//! Session management messages

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique peer identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PeerId(pub Uuid);

impl PeerId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Format as user-friendly display string (UUID)
    pub fn to_display_string(&self) -> String {
        self.0.to_string().to_uppercase()
    }

    /// Parse from display string
    pub fn from_display_string(s: &str) -> Option<Self> {
        let trimmed = s.trim();
        if let Ok(uuid) = Uuid::parse_str(trimmed) {
            return Some(Self(uuid));
        }

        let cleaned: String = trimmed.chars().filter(|c| c.is_alphanumeric()).collect();
        if cleaned.len() != 32 {
            return None;
        }
        Uuid::parse_str(&cleaned.to_lowercase()).ok().map(Self)
    }
}

impl Default for PeerId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for PeerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_display_string())
    }
}

/// Session role
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionRole {
    /// Host sharing their screen
    Host,
    /// Viewer connecting to a host
    Viewer,
}

/// Session state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionState {
    /// Initial state, not connected
    Disconnected,
    /// Connecting to signaling server
    Connecting,
    /// Waiting for peer to accept
    WaitingForPeer,
    /// Performing NAT traversal
    NatTraversal,
    /// Performing cryptographic handshake
    Handshaking,
    /// Session is active
    Active,
    /// Session is paused
    Paused,
    /// Session ended gracefully
    Ended,
    /// Session failed with error
    Failed,
}

/// Session quality preset
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QualityPreset {
    /// Optimize for lowest latency
    LowLatency,
    /// Balanced latency and quality
    Balanced,
    /// Optimize for quality
    HighQuality,
}

impl Default for QualityPreset {
    fn default() -> Self {
        Self::LowLatency
    }
}

/// Session configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    /// Quality preset
    pub quality: QualityPreset,
    /// Target FPS (5-60)
    pub target_fps: u8,
    /// Maximum bitrate in kbps
    pub max_bitrate_kbps: u32,
    /// Enable audio
    pub audio_enabled: bool,
    /// Enable clipboard sync
    pub clipboard_sync: bool,
    /// Enable file transfer
    pub file_transfer: bool,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            quality: QualityPreset::LowLatency,
            target_fps: 30,
            max_bitrate_kbps: 5000,
            audio_enabled: false,
            clipboard_sync: true,
            file_transfer: true,
        }
    }
}

/// Session control messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SessionMessage {
    /// Initial handshake request
    Hello {
        peer_id: PeerId,
        protocol_version: u32,
        role: SessionRole,
        public_key: [u8; 32],
    },
    /// Handshake response
    HelloAck {
        peer_id: PeerId,
        public_key: [u8; 32],
        session_id: Uuid,
    },
    /// Session configuration
    Configure(SessionConfig),
    /// Request keyframe
    RequestKeyframe,
    /// Pause streaming
    Pause,
    /// Resume streaming
    Resume,
    /// End session gracefully
    Goodbye { reason: String },
    /// Ping for latency measurement
    Ping { timestamp_us: u64 },
    /// Pong response
    Pong { ping_timestamp_us: u64 },
    /// Quality adjustment request
    AdjustQuality {
        target_bitrate_kbps: u32,
        target_fps: u8,
    },
}

impl SessionMessage {
    /// Serialize to bytes
    pub fn to_bytes(&self) -> Result<Vec<u8>, bincode::Error> {
        bincode::serialize(self)
    }

    /// Deserialize from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self, bincode::Error> {
        bincode::deserialize(data)
    }
}

/// Signaling server messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SignalingMessage {
    /// Register with the signaling server
    Register {
        peer_id: PeerId,
    },
    /// Registration confirmation
    Registered {
        peer_id: PeerId,
    },
    /// Request connection to a peer
    Connect {
        target_peer_id: PeerId,
    },
    /// Incoming connection request
    IncomingConnection {
        from_peer_id: PeerId,
    },
    /// Accept incoming connection
    Accept {
        from_peer_id: PeerId,
    },
    /// Reject incoming connection
    Reject {
        from_peer_id: PeerId,
        reason: String,
    },
    /// ICE candidate exchange
    IceCandidate {
        target_peer_id: PeerId,
        candidate: IceCandidate,
    },
    /// Connection established notification
    Connected {
        peer_id: PeerId,
    },
    /// Peer disconnected
    Disconnected {
        peer_id: PeerId,
    },
    /// Error from signaling server
    Error {
        message: String,
    },
    /// Heartbeat
    Ping,
    Pong,
}

/// ICE candidate for NAT traversal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IceCandidate {
    /// Candidate type
    pub candidate_type: IceCandidateType,
    /// IP address
    pub address: String,
    /// Port
    pub port: u16,
    /// Priority
    pub priority: u32,
}

/// ICE candidate type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IceCandidateType {
    /// Host candidate (local address)
    Host,
    /// Server reflexive (STUN)
    ServerReflexive,
    /// Peer reflexive (discovered during connectivity checks)
    PeerReflexive,
    /// Relay (TURN)
    Relay,
}
