//! Error types for the protocol

use thiserror::Error;

/// Protocol error
#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("Serialization error: {0}")]
    Serialization(#[from] bincode::Error),

    #[error("Invalid packet type: {0}")]
    InvalidPacketType(u8),

    #[error("Protocol version mismatch: expected {expected}, got {actual}")]
    VersionMismatch { expected: u32, actual: u32 },

    #[error("Invalid peer ID format")]
    InvalidPeerId,

    #[error("Packet too large: {size} bytes (max: {max})")]
    PacketTooLarge { size: usize, max: usize },

    #[error("Missing required field: {0}")]
    MissingField(&'static str),

    #[error("Invalid state transition: {from:?} -> {to:?}")]
    InvalidStateTransition {
        from: crate::SessionState,
        to: crate::SessionState,
    },

    #[error("Session not found")]
    SessionNotFound,

    #[error("Peer not found")]
    PeerNotFound,

    #[error("Connection rejected: {0}")]
    ConnectionRejected(String),

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("Encryption error: {0}")]
    Encryption(String),

    #[error("Decryption error: {0}")]
    Decryption(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

/// Result type alias for protocol operations
pub type ProtocolResult<T> = Result<T, ProtocolError>;
