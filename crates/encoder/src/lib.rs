//! Video Encoder - H.264/H.265 encoding for Entangle
//!
//! Provides abstraction over encoding backends:
//! - OpenH264 (software, cross-platform)
//! - VideoToolbox (macOS hardware)
//! - NVENC (NVIDIA hardware)

mod error;
mod openh264_encoder;
mod traits;

pub use error::*;
pub use openh264_encoder::*;
pub use traits::*;
