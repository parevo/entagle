//! Input Injector - OS-level input injection for Entangle
//!
//! Provides keyboard and mouse input simulation for remote control.

mod error;
mod traits;

#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "windows")]
mod windows;

pub use error::*;
pub use traits::*;

#[cfg(target_os = "macos")]
pub use macos::MacOSInputInjector;

#[cfg(target_os = "windows")]
pub use windows::WindowsInputInjector;

/// Create a platform-appropriate input injector
pub fn create_injector() -> InjectorResult<Box<dyn InputInjector>> {
    #[cfg(target_os = "macos")]
    {
        Ok(Box::new(MacOSInputInjector::new()?))
    }

    #[cfg(target_os = "windows")]
    {
        Ok(Box::new(WindowsInputInjector::new()?))
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Err(InjectorError::UnsupportedPlatform)
    }
}
