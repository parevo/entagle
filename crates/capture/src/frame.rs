//! Captured frame data structures

use bytes::Bytes;
use std::time::Instant;

/// Pixel format of the captured frame
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    /// BGRA 8-bit per channel
    Bgra8,
    /// RGBA 8-bit per channel
    Rgba8,
    /// NV12 (YUV 4:2:0, used by hardware encoders)
    Nv12,
    /// I420 (YUV 4:2:0 planar)
    I420,
}

impl PixelFormat {
    /// Bytes per pixel for RGB formats
    pub fn bytes_per_pixel(&self) -> Option<usize> {
        match self {
            PixelFormat::Bgra8 | PixelFormat::Rgba8 => Some(4),
            PixelFormat::Nv12 | PixelFormat::I420 => None, // Variable
        }
    }
}

/// Region of the screen that changed
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirtyRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl DirtyRect {
    pub fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn full_screen(width: u32, height: u32) -> Self {
        Self {
            x: 0,
            y: 0,
            width,
            height,
        }
    }

    /// Check if this rect contains another rect
    pub fn contains(&self, other: &DirtyRect) -> bool {
        other.x >= self.x
            && other.y >= self.y
            && other.x + other.width <= self.x + self.width
            && other.y + other.height <= self.y + self.height
    }

    /// Merge two rects into a bounding box
    pub fn merge(&self, other: &DirtyRect) -> DirtyRect {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        let right = (self.x + self.width).max(other.x + other.width);
        let bottom = (self.y + self.height).max(other.y + other.height);

        DirtyRect {
            x,
            y,
            width: right - x,
            height: bottom - y,
        }
    }

    /// Calculate area
    pub fn area(&self) -> u64 {
        self.width as u64 * self.height as u64
    }
}

/// Captured frame data
#[derive(Debug, Clone)]
pub struct CapturedFrame {
    /// Raw pixel data
    pub data: Bytes,
    /// Frame width
    pub width: u32,
    /// Frame height
    pub height: u32,
    /// Stride (bytes per row, may include padding)
    pub stride: u32,
    /// Pixel format
    pub format: PixelFormat,
    /// Capture timestamp
    pub timestamp: Instant,
    /// Frame sequence number
    pub sequence: u64,
    /// Changed regions (if dirty rect detection is enabled)
    pub dirty_rects: Vec<DirtyRect>,
    /// Display ID this frame was captured from
    pub display_id: u32,
}

impl CapturedFrame {
    /// Check if the entire frame changed
    pub fn is_full_update(&self) -> bool {
        self.dirty_rects.is_empty()
            || (self.dirty_rects.len() == 1
                && self.dirty_rects[0].width == self.width
                && self.dirty_rects[0].height == self.height)
    }

    /// Get the bounding box of all dirty rects
    pub fn dirty_bounds(&self) -> Option<DirtyRect> {
        if self.dirty_rects.is_empty() {
            return None;
        }

        self.dirty_rects
            .iter()
            .copied()
            .reduce(|acc, rect| acc.merge(&rect))
    }

    /// Calculate the percentage of screen that changed
    pub fn dirty_percentage(&self) -> f64 {
        if self.dirty_rects.is_empty() {
            return 100.0;
        }

        let total_area = self.width as u64 * self.height as u64;
        let dirty_area: u64 = self.dirty_rects.iter().map(|r| r.area()).sum();

        (dirty_area as f64 / total_area as f64) * 100.0
    }

    /// Convert BGRA to RGBA in place (if needed for encoding)
    pub fn bgra_to_rgba(&mut self) {
        if self.format != PixelFormat::Bgra8 {
            return;
        }

        // Safety: we're swapping R and B channels
        let data = Bytes::from(
            self.data
                .chunks_exact(4)
                .flat_map(|chunk| [chunk[2], chunk[1], chunk[0], chunk[3]])
                .collect::<Vec<u8>>(),
        );

        self.data = data;
        self.format = PixelFormat::Rgba8;
    }
}

/// Display information
#[derive(Debug, Clone)]
pub struct DisplayInfo {
    /// Display ID
    pub id: u32,
    /// Display name
    pub name: String,
    /// Width in pixels
    pub width: u32,
    /// Height in pixels
    pub height: u32,
    /// Refresh rate in Hz
    pub refresh_rate: f64,
    /// Scale factor (for HiDPI)
    pub scale: f64,
    /// Is this the primary display?
    pub is_primary: bool,
    /// X position in virtual screen
    pub x: i32,
    /// Y position in virtual screen
    pub y: i32,
}
