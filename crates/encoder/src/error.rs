//! Encoder error types

use thiserror::Error;

#[derive(Debug, Error)]
pub enum EncoderError {
    #[error("Encoder initialization failed: {0}")]
    InitFailed(String),

    #[error("Encoding failed: {0}")]
    EncodingFailed(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("Unsupported resolution: {width}x{height}")]
    UnsupportedResolution { width: u32, height: u32 },

    #[error("Unsupported pixel format")]
    UnsupportedPixelFormat,

    #[error("Buffer too small")]
    BufferTooSmall,

    #[error("Encoder not initialized")]
    NotInitialized,

    #[error("Internal error: {0}")]
    Internal(String),
}

pub type EncoderResult<T> = Result<T, EncoderError>;
