//! macOS screen capture using ScreenCaptureKit
//!
//! Real implementation using `screencapturekit` crate.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bytes::Bytes;
use screencapturekit::{
    cm_sample_buffer::CMSampleBuffer,
    sc_content_filter::{InitParams, SCContentFilter},
    sc_error_handler::StreamErrorHandler,
    sc_output_handler::{SCStreamOutputType, StreamOutput},
    sc_shareable_content::SCShareableContent,
    sc_stream::SCStream,
    sc_stream_configuration::{PixelFormat as SCStreamPixelFormat, SCStreamConfiguration},
};
use screencapturekit_sys::{
    cv_pixel_buffer_ref::CVPixelBufferRef,
    sc_stream_frame_info::SCFrameStatus,
};

use crate::{
    CaptureConfig, CaptureError, CaptureResult, CaptureStats, CapturedFrame, DirtyRect,
    DisplayInfo, PixelFormat, ScreenCapture,
};

fn has_screen_recording_permission() -> bool {
    core_graphics::access::ScreenCaptureAccess::default().preflight()
}

struct OutputHandler {
    latest_frame: Arc<Mutex<Option<CapturedFrame>>>,
    stats: Arc<Mutex<CaptureStats>>,
    frame_count: Arc<std::sync::atomic::AtomicU64>,
    display_id: u32,
    _dirty_rects_enabled: bool,
}

impl StreamOutput for OutputHandler {
    fn did_output_sample_buffer(
        &self,
        sample_buffer: CMSampleBuffer,
        of_type: SCStreamOutputType,
    ) {
        // Only process screen video frames
        if let SCStreamOutputType::Screen = of_type { // FIXED MATCH
            // Continue
        } else {
            return;
        }

        if !matches!(
            sample_buffer.frame_status,
            SCFrameStatus::Complete | SCFrameStatus::Started
        ) {
            return;
        }

        let Some(image_buf_ref) = sample_buffer.image_buf_ref.as_ref() else {
            return;
        };
        let pixel_buf = image_buf_ref.as_pixel_buffer();
        let pixel_buf_ref: &CVPixelBufferRef = &*pixel_buf;

        if pixel_buf_ref.lock_base_address(0) != 0 {
            return;
        }

        let base = pixel_buf_ref.get_base_address() as *const u8;
        if base.is_null() {
            pixel_buf_ref.unlock_base_address(0);
            return;
        }

        let width = unsafe { CVPixelBufferGetWidth(pixel_buf_ref) } as usize;
        let height = unsafe { CVPixelBufferGetHeight(pixel_buf_ref) } as usize;
        let stride = unsafe { CVPixelBufferGetBytesPerRow(pixel_buf_ref) } as usize;
        let size = stride.saturating_mul(height);
        let data = unsafe { std::slice::from_raw_parts(base, size).to_vec() };

        pixel_buf_ref.unlock_base_address(0);

        let frame = CapturedFrame {
            data: Bytes::from(data),
            width: width as u32,
            height: height as u32,
            stride: stride as u32,
            format: PixelFormat::Bgra8,
            timestamp: Instant::now(),
            sequence: self.frame_count.fetch_add(1, Ordering::Relaxed),
            dirty_rects: vec![DirtyRect::full_screen(width as u32, height as u32)],
            display_id: self.display_id,
        };

        let mut guard = self.latest_frame.lock().unwrap();
        *guard = Some(frame);

        // Update stats
        let mut stats = self.stats.lock().unwrap();
        stats.frames_captured += 1;
        stats.current_fps = 60.0;
    }
}

struct ErrorHandler;
impl StreamErrorHandler for ErrorHandler {
    fn on_error(&self) {
        tracing::error!("ScreenCaptureKit stream error");
    }
}

#[link(name = "CoreVideo", kind = "framework")]
unsafe extern "C" {
    fn CVPixelBufferGetWidth(pixel_buf: *const CVPixelBufferRef) -> usize;
    fn CVPixelBufferGetHeight(pixel_buf: *const CVPixelBufferRef) -> usize;
    fn CVPixelBufferGetBytesPerRow(pixel_buf: *const CVPixelBufferRef) -> usize;
}

pub struct MacOSCapture {
    running: Arc<AtomicBool>,
    stream: Option<SCStream>,
    latest_frame: Arc<Mutex<Option<CapturedFrame>>>,
    stats: Arc<Mutex<CaptureStats>>,
    frame_count: Arc<std::sync::atomic::AtomicU64>,
    current_config: Option<CaptureConfig>,
}

unsafe impl Send for MacOSCapture {}
unsafe impl Sync for MacOSCapture {}

impl MacOSCapture {
    pub fn new() -> CaptureResult<Self> {
        Ok(Self {
            running: Arc::new(AtomicBool::new(false)),
            stream: None,
            latest_frame: Arc::new(Mutex::new(None)),
            stats: Arc::new(Mutex::new(CaptureStats::default())),
            frame_count: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            current_config: None,
        })
    }
}

impl ScreenCapture for MacOSCapture {
    fn displays(&self) -> CaptureResult<Vec<DisplayInfo>> {
        // Refetch or return cached

        let content = SCShareableContent::current();
        let mut displays = Vec::new();
        for (i, display) in content.displays.into_iter().enumerate() {
            displays.push(DisplayInfo {
                id: display.display_id,
                name: format!("Display {}", display.display_id),
                width: display.width as u32,
                height: display.height as u32,
                refresh_rate: 60.0,
                scale: 1.0,
                is_primary: i == 0, // Simple heuristic
                x: 0,
                y: 0,
            });
        }
        Ok(displays)
    }

    fn start(&mut self, config: CaptureConfig) -> CaptureResult<()> {
        if self.running.load(Ordering::SeqCst) {
            return Err(CaptureError::AlreadyRunning);
        }

        if !has_screen_recording_permission() {
            return Err(CaptureError::PermissionDenied);
        }

        let content = SCShareableContent::current();
        let display_id = config.display_id.unwrap_or_else(|| {
            // Default to first display
            content.displays.first().map(|d| d.display_id).unwrap_or(0)
        });

        let display = content
            .displays
            .iter()
            .find(|d| d.display_id == display_id)
            .ok_or(CaptureError::NoDisplays)?;

        // Filter
        let filter = SCContentFilter::new(InitParams::Display(display.clone()));

        // Config
        let stream_config = SCStreamConfiguration {
            width: display.width as u32,
            height: display.height as u32,
            shows_cursor: config.capture_cursor,
            pixel_format: SCStreamPixelFormat::ARGB8888,
            ..SCStreamConfiguration::default()
        };

        // Note: pixel_format method might require a typed enum.
        // If I leave it default, it usually picks BGRA.

        // Output Handler
        let output_handler = OutputHandler {
            latest_frame: self.latest_frame.clone(),
            stats: self.stats.clone(),
            frame_count: self.frame_count.clone(),
            display_id: display.display_id,
            _dirty_rects_enabled: config.dirty_rects,
        };

        let mut stream = SCStream::new(filter, stream_config, ErrorHandler);
        stream.add_output(output_handler, SCStreamOutputType::Screen);

        stream
            .start_capture()
            .map_err(|e| CaptureError::Platform(e.to_string()))?;

        self.stream = Some(stream);
        self.running.store(true, Ordering::SeqCst);
        self.current_config = Some(config);

        Ok(())
    }

    fn stop(&mut self) -> CaptureResult<()> {
        if let Some(stream) = self.stream.take() {
            stream.stop_capture().ok();
        }
        self.running.store(false, Ordering::SeqCst);
        Ok(())
    }

    fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    fn capture_frame(&mut self) -> CaptureResult<CapturedFrame> {
        // Blocking wait for a new frame
        // In this architecture, `capture_frame` is polled.
        // We should wait until `latest_frame` logic has a *new* frame.
        // But for low latency, we might just return the latest one available.
        // To implement blocking, we need a Condvar or logic to wait for sequence diff.

        let start = Instant::now();
        loop {
            // Check if we have a frame
            {
                let mut guard = self.latest_frame.lock().unwrap();
                if let Some(frame) = guard.take() {
                    return Ok(frame);
                }
            }

            if !self.running.load(Ordering::SeqCst) {
                return Err(CaptureError::NotRunning);
            }

            if start.elapsed() > Duration::from_secs(1) {
                // Timeout
                return Err(CaptureError::Processing("Timeout waiting for frame".into()));
            }

            std::thread::yield_now();
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn try_capture_frame(&mut self) -> CaptureResult<Option<CapturedFrame>> {
        let mut guard = self.latest_frame.lock().unwrap();
        Ok(guard.take())
    }

    fn stats(&self) -> CaptureStats {
        self.stats.lock().unwrap().clone()
    }
}
