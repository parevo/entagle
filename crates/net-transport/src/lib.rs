//! Network Transport - QUIC-based networking for Entangle
//!
//! Provides low-latency transport using QUIC datagrams for video
//! and reliable streams for input/control messages.

mod congestion;
mod error;
mod transport;

pub use congestion::*;
pub use error::*;
pub use transport::*;

/// Default QUIC port
pub const DEFAULT_QUIC_PORT: u16 = 19823;

/// Maximum datagram size
pub const MAX_DATAGRAM_SIZE: usize = 1200;
