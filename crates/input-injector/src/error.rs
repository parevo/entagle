//! Input injection error types

use thiserror::Error;

#[derive(Debug, Error)]
pub enum InjectorError {
    #[error("Input injection not available")]
    NotAvailable,

    #[error("Permission denied - accessibility permission required")]
    PermissionDenied,

    #[error("Failed to inject input: {0}")]
    InjectionFailed(String),

    #[error("Invalid key code: {0}")]
    InvalidKeyCode(u16),

    #[error("Invalid coordinates: ({x}, {y})")]
    InvalidCoordinates { x: f64, y: f64 },

    #[error("Platform not supported")]
    UnsupportedPlatform,

    #[error("Platform error: {0}")]
    Platform(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

pub type InjectorResult<T> = Result<T, InjectorError>;
