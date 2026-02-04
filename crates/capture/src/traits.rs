//! Screen capture trait abstraction

use crate::{CaptureResult, CapturedFrame, DisplayInfo};

/// Capture configuration
#[derive(Debug, Clone)]
pub struct CaptureConfig {
    /// Target display ID (None for primary)
    pub display_id: Option<u32>,
    /// Target FPS
    pub target_fps: u32,
    /// Enable dirty rect detection
    pub dirty_rects: bool,
    /// Capture cursor
    pub capture_cursor: bool,
    /// Capture audio (if supported)
    pub capture_audio: bool,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            display_id: None,
            target_fps: 30,
            dirty_rects: true,
            capture_cursor: true,
            capture_audio: false,
        }
    }
}

/// Screen capture trait
pub trait ScreenCapture: Send + Sync {
    /// Get available displays
    fn displays(&self) -> CaptureResult<Vec<DisplayInfo>>;

    /// Start capturing with the given configuration
    fn start(&mut self, config: CaptureConfig) -> CaptureResult<()>;

    /// Stop capturing
    fn stop(&mut self) -> CaptureResult<()>;

    /// Check if capture is running
    fn is_running(&self) -> bool;

    /// Capture a single frame (blocking)
    fn capture_frame(&mut self) -> CaptureResult<CapturedFrame>;

    /// Try to capture a frame without blocking
    fn try_capture_frame(&mut self) -> CaptureResult<Option<CapturedFrame>>;

    /// Get current capture statistics
    fn stats(&self) -> CaptureStats;
}

/// Capture statistics
#[derive(Debug, Clone, Default)]
pub struct CaptureStats {
    /// Total frames captured
    pub frames_captured: u64,
    /// Frames dropped due to slow consumer
    pub frames_dropped: u64,
    /// Average capture latency in microseconds
    pub avg_capture_latency_us: u64,
    /// Current FPS
    pub current_fps: f64,
}
