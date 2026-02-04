//! Shared Protocol Definitions for Entangle
//!
//! This crate contains all packet definitions, enums, and types shared
//! across the Entangle remote desktop application.

mod packets;
mod session;
mod input;
mod error;

pub use packets::*;
pub use session::*;
pub use input::*;
pub use error::*;

/// Protocol version for compatibility checking
pub const PROTOCOL_VERSION: u32 = 1;

/// Maximum packet size for datagrams (MTU-safe)
pub const MAX_DATAGRAM_SIZE: usize = 1200;

/// Maximum video packet payload size
pub const MAX_VIDEO_PAYLOAD: usize = MAX_DATAGRAM_SIZE - 64; // Leave room for headers
