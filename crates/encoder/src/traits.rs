//! Video encoder trait abstraction

use bytes::Bytes;
use capture::CapturedFrame;

use crate::EncoderResult;

/// Video codec type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Codec {
    H264,
    H265,
}

impl Default for Codec {
    fn default() -> Self {
        Self::H264
    }
}

/// Encoder rate control mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateControl {
    /// Constant Bitrate
    Cbr,
    /// Variable Bitrate
    Vbr,
    /// Constant Quality
    Cqp,
}

impl Default for RateControl {
    fn default() -> Self {
        Self::Vbr
    }
}

/// Encoder configuration
#[derive(Debug, Clone)]
pub struct EncoderConfig {
    /// Target codec
    pub codec: Codec,
    /// Frame width
    pub width: u32,
    /// Frame height
    pub height: u32,
    /// Target bitrate in kbps
    pub bitrate_kbps: u32,
    /// Target FPS
    pub fps: u32,
    /// Keyframe interval (GOP size)
    pub keyframe_interval: u32,
    /// Rate control mode
    pub rate_control: RateControl,
    /// Encoder preset (0-9, 0 = fastest, 9 = best quality)
    pub preset: u8,
    /// Enable low-latency mode
    pub low_latency: bool,
}

impl Default for EncoderConfig {
    fn default() -> Self {
        Self {
            codec: Codec::H264,
            width: 1920,
            height: 1080,
            bitrate_kbps: 3000,
            fps: 30,
            keyframe_interval: 60, // Keyframe every 2 seconds at 30fps
            rate_control: RateControl::Vbr,
            preset: 3, // Fast preset for low latency
            low_latency: true,
        }
    }
}

/// Encoded frame type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodedFrameType {
    /// Keyframe (I-frame)
    Key,
    /// Predicted frame (P-frame)
    Predicted,
    /// Bidirectional frame (B-frame)
    Bidirectional,
}

/// Encoded frame output
#[derive(Debug, Clone)]
pub struct EncodedFrame {
    /// NAL units data
    pub data: Bytes,
    /// Frame width
    pub width: u32,
    /// Frame height
    pub height: u32,
    /// Frame type
    pub frame_type: EncodedFrameType,
    /// Presentation timestamp in microseconds
    pub pts_us: u64,
    /// Decode timestamp in microseconds
    pub dts_us: u64,
    /// Frame sequence number
    pub sequence: u64,
    /// Encoding took this many microseconds
    pub encode_time_us: u64,
}

/// Video encoder trait
pub trait VideoEncoder: Send {
    /// Initialize the encoder with configuration
    fn init(&mut self, config: EncoderConfig) -> EncoderResult<()>;

    /// Encode a captured frame
    fn encode(&mut self, frame: &CapturedFrame) -> EncoderResult<EncodedFrame>;

    /// Force next frame to be a keyframe
    fn force_keyframe(&mut self);

    /// Update bitrate dynamically
    fn set_bitrate(&mut self, bitrate_kbps: u32) -> EncoderResult<()>;

    /// Update FPS dynamically
    fn set_fps(&mut self, fps: u32) -> EncoderResult<()>;

    /// Get current configuration
    fn config(&self) -> &EncoderConfig;

    /// Get encoder statistics
    fn stats(&self) -> EncoderStats;

    /// Flush the encoder and get any remaining frames
    fn flush(&mut self) -> EncoderResult<Vec<EncodedFrame>>;
}

/// Encoder statistics
#[derive(Debug, Clone, Default)]
pub struct EncoderStats {
    /// Total frames encoded
    pub frames_encoded: u64,
    /// Total bytes output
    pub bytes_output: u64,
    /// Average encoding time in microseconds
    pub avg_encode_time_us: u64,
    /// Average frame size in bytes
    pub avg_frame_size: u64,
    /// Current encoding FPS
    pub current_fps: f64,
    /// Keyframes generated
    pub keyframes: u64,
}
