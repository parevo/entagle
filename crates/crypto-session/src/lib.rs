//! Crypto Session - End-to-End Encryption for Entangle
//!
//! Provides X25519 key exchange with ChaCha20Poly1305 symmetric encryption.

mod error;
mod session;

pub use error::*;
pub use session::*;

/// Nonce size for ChaCha20Poly1305 (96 bits / 12 bytes)
pub const NONCE_SIZE: usize = 12;

/// Authentication tag size (128 bits / 16 bytes)
pub const TAG_SIZE: usize = 16;

/// Public key size (256 bits / 32 bytes)
pub const PUBLIC_KEY_SIZE: usize = 32;

/// Shared secret size (256 bits / 32 bytes)
pub const SHARED_SECRET_SIZE: usize = 32;
