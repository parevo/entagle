//! Crypto session error types

use thiserror::Error;

/// Cryptographic operation error
#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("Key generation failed: {0}")]
    KeyGeneration(String),

    #[error("Key exchange failed: {0}")]
    KeyExchange(String),

    #[error("Encryption failed: {0}")]
    Encryption(String),

    #[error("Decryption failed: authentication tag mismatch")]
    DecryptionFailed,

    #[error("Invalid key length: expected {expected}, got {actual}")]
    InvalidKeyLength { expected: usize, actual: usize },

    #[error("Invalid nonce length: expected {expected}, got {actual}")]
    InvalidNonceLength { expected: usize, actual: usize },

    #[error("Nonce overflow: maximum message count exceeded")]
    NonceOverflow,

    #[error("Session not established")]
    SessionNotEstablished,

    #[error("Session already established")]
    SessionAlreadyEstablished,

    #[error("Invalid public key")]
    InvalidPublicKey,
}

pub type CryptoResult<T> = Result<T, CryptoError>;
