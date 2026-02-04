//! Screen Capture - Platform-native screen capture for Entangle
//!
//! Provides abstraction over platform-specific capture APIs:
//! - macOS: ScreenCaptureKit
//! - Windows: DXGI Desktop Duplication

mod error;
mod frame;
mod traits;

#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "windows")]
mod windows;

pub use error::*;
pub use frame::*;
pub use traits::*;

#[cfg(target_os = "macos")]
pub use macos::MacOSCapture;

#[cfg(target_os = "windows")]
pub use windows::WindowsCapture;

/// Create a platform-appropriate screen capture instance
pub fn create_capture() -> CaptureResult<Box<dyn ScreenCapture>> {
    #[cfg(target_os = "macos")]
    {
        Ok(Box::new(MacOSCapture::new()?))
    }

    #[cfg(target_os = "windows")]
    {
        Ok(Box::new(WindowsCapture::new()?))
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Err(CaptureError::UnsupportedPlatform)
    }
}
