//! Capture error types

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CaptureError {
    #[error("Screen capture not available")]
    NotAvailable,

    #[error("Permission denied - screen recording permission required")]
    PermissionDenied,

    #[error("No displays found")]
    NoDisplays,

    #[error("Display not found: {0}")]
    DisplayNotFound(u32),

    #[error("Capture initialization failed: {0}")]
    InitFailed(String),

    #[error("Frame capture failed: {0}")]
    CaptureFailed(String),

    #[error("Unsupported pixel format: {0}")]
    UnsupportedFormat(String),

    #[error("Platform not supported")]
    UnsupportedPlatform,

    #[error("Capture already running")]
    AlreadyRunning,

    #[error("Capture not running")]
    NotRunning,

    #[error("Timeout waiting for frame")]
    Timeout,

    #[error("Platform error: {0}")]
    Platform(String),

    #[error("Processing error: {0}")]
    Processing(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

pub type CaptureResult<T> = Result<T, CaptureError>;
