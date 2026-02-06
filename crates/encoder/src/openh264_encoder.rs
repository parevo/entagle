//! OpenH264 encoder implementation

use bytes::Bytes;
use capture::{CapturedFrame, PixelFormat};
use openh264::Error as OpenH264Error;
use openh264::encoder::{Encoder, EncoderConfig as OpenH264Config};
use std::time::Instant;
use tracing::{debug, info};

use crate::{
    Codec, EncodedFrame, EncodedFrameType, EncoderConfig, EncoderError, EncoderResult,
    EncoderStats, VideoEncoder,
};

/// OpenH264-based software encoder
pub struct OpenH264Encoder {
    encoder: Option<Encoder>,
    config: EncoderConfig,
    stats: EncoderStats,
    force_keyframe: bool,
    frame_counter: u64,
    encode_times: Vec<u64>,
    yuv_buffer: Vec<u8>,
}

impl OpenH264Encoder {
    /// Create a new OpenH264 encoder
    pub fn new() -> Self {
        Self {
            encoder: None,
            config: EncoderConfig::default(),
            stats: EncoderStats::default(),
            force_keyframe: false,
            frame_counter: 0,
            encode_times: Vec::with_capacity(100),
            yuv_buffer: Vec::new(),
        }
    }

    /// Convert BGRA/RGBA to I420 (YUV planar)
    fn rgb_to_yuv(&mut self, frame: &CapturedFrame) -> EncoderResult<&[u8]> {
        let width = frame.width as usize;
        let height = frame.height as usize;

        // I420 size: Y plane + U plane (1/4) + V plane (1/4) = 1.5 * width * height
        let y_size = width * height;
        let uv_size = (width / 2) * (height / 2);
        let total_size = y_size + 2 * uv_size;

        self.yuv_buffer.resize(total_size, 0);

        let (y_plane, uv_planes) = self.yuv_buffer.split_at_mut(y_size);
        let (u_plane, v_plane) = uv_planes.split_at_mut(uv_size);

        let is_bgra = frame.format == PixelFormat::Bgra8;

        // Convert RGB(A) to YUV
        for y in 0..height {
            for x in 0..width {
                let pixel_offset = y * frame.stride as usize + x * 4;

                let (r, g, b) = if is_bgra {
                    (
                        frame.data[pixel_offset + 2] as i32,
                        frame.data[pixel_offset + 1] as i32,
                        frame.data[pixel_offset] as i32,
                    )
                } else {
                    (
                        frame.data[pixel_offset] as i32,
                        frame.data[pixel_offset + 1] as i32,
                        frame.data[pixel_offset + 2] as i32,
                    )
                };

                // BT.601 conversion
                let y_val = ((66 * r + 129 * g + 25 * b + 128) >> 8) + 16;
                y_plane[y * width + x] = y_val.clamp(0, 255) as u8;

                // Subsample for U and V (every 2x2 block)
                if (x % 2 == 0) && (y % 2 == 0) {
                    let u_val = ((-38 * r - 74 * g + 112 * b + 128) >> 8) + 128;
                    let v_val = ((112 * r - 94 * g - 18 * b + 128) >> 8) + 128;

                    let uv_idx = (y / 2) * (width / 2) + (x / 2);
                    u_plane[uv_idx] = u_val.clamp(0, 255) as u8;
                    v_plane[uv_idx] = v_val.clamp(0, 255) as u8;
                }
            }
        }

        Ok(&self.yuv_buffer)
    }
}

impl Default for OpenH264Encoder {
    fn default() -> Self {
        Self::new()
    }
}

impl VideoEncoder for OpenH264Encoder {
    fn init(&mut self, config: EncoderConfig) -> EncoderResult<()> {
        if config.codec != Codec::H264 {
            return Err(EncoderError::InvalidConfig(
                "OpenH264 only supports H.264".to_string(),
            ));
        }

        info!(
            "Initializing OpenH264 encoder: {}x{} @ {} kbps, {} fps",
            config.width, config.height, config.bitrate_kbps, config.fps
        );

        let mut openh264_config = OpenH264Config::new()
            .set_bitrate_bps(config.bitrate_kbps * 1000)
            .max_frame_rate(config.fps as f32)
            .usage_type(openh264::encoder::UsageType::ScreenContentRealTime)
            .enable_skip_frame(false);

        let encoder =
            Encoder::with_api_config(openh264::OpenH264API::from_source(), openh264_config)
                .map_err(|e: OpenH264Error| EncoderError::InitFailed(e.to_string()))?;

        self.encoder = Some(encoder);
        self.config = config;
        self.stats = EncoderStats::default();
        self.frame_counter = 0;
        self.encode_times.clear();

        Ok(())
    }

    fn encode(&mut self, frame: &CapturedFrame) -> EncoderResult<EncodedFrame> {
        if self.encoder.is_none() {
            return Err(EncoderError::NotInitialized);
        }

        if frame.width != self.config.width || frame.height != self.config.height {
            return Err(EncoderError::UnsupportedResolution {
                width: frame.width,
                height: frame.height,
            });
        }

        let start = Instant::now();

        // Convert to YUV and clone to release the borrow on self
        let yuv_data = self.rgb_to_yuv(frame)?.to_vec();

        // Now we can borrow the encoder
        let encoder = self.encoder.as_mut().unwrap();

        // Create the YUV source
        let yuv_source = openh264::formats::YUVBuffer::from_vec(
            yuv_data,
            self.config.width as usize,
            self.config.height as usize,
        );

        // Encode
        let is_keyframe =
            self.force_keyframe || self.frame_counter % self.config.keyframe_interval as u64 == 0;

        if self.force_keyframe {
            self.force_keyframe = false;
            debug!("Forcing keyframe");
        }

        if is_keyframe {
            encoder.force_intra_frame();
        }

        // For actual encoding, we'd use encoder.encode() here
        let bitstream_result = encoder
            .encode(&yuv_source)
            .map_err(|e| EncoderError::EncodingFailed(e.to_string()))?;

        let encode_time = start.elapsed().as_micros() as u64;

        // Collect NAL units with Annex-B start codes
        let mut nal_data = Vec::new();
        for l in 0..bitstream_result.num_layers() {
            if let Some(layer) = bitstream_result.layer(l) {
                for n in 0..layer.nal_count() {
                    if let Some(nal) = layer.nal_unit(n) {
                        let has_start_code = nal.starts_with(&[0, 0, 0, 1])
                            || nal.starts_with(&[0, 0, 1]);
                        if !has_start_code {
                            nal_data.extend_from_slice(&[0, 0, 0, 1]);
                        }
                        nal_data.extend_from_slice(nal);
                    }
                }
            }
        }

        if nal_data.is_empty() {
            return Err(EncoderError::EncodingFailed(
                "Empty bitstream from encoder".to_string(),
            ));
        }

        let frame_type = if is_keyframe {
            EncodedFrameType::Key
        } else {
            EncodedFrameType::Predicted
        };

        let pts_us = (self.frame_counter as f64 / self.config.fps as f64 * 1_000_000.0) as u64;

        let encoded = EncodedFrame {
            data: Bytes::from(nal_data),
            width: frame.width,
            height: frame.height,
            frame_type,
            pts_us,
            dts_us: pts_us,
            sequence: self.frame_counter,
            encode_time_us: encode_time,
        };

        // Update stats
        self.frame_counter += 1;
        self.stats.frames_encoded += 1;
        self.stats.bytes_output += encoded.data.len() as u64;

        if is_keyframe {
            self.stats.keyframes += 1;
        }

        self.encode_times.push(encode_time);
        if self.encode_times.len() > 100 {
            self.encode_times.remove(0);
        }

        if !self.encode_times.is_empty() {
            self.stats.avg_encode_time_us =
                self.encode_times.iter().sum::<u64>() / self.encode_times.len() as u64;
        }

        self.stats.avg_frame_size = self.stats.bytes_output / self.stats.frames_encoded;

        Ok(encoded)
    }

    fn force_keyframe(&mut self) {
        self.force_keyframe = true;
    }

    fn set_bitrate(&mut self, bitrate_kbps: u32) -> EncoderResult<()> {
        self.config.bitrate_kbps = bitrate_kbps;
        // OpenH264 requires re-initialization for bitrate change
        // In a production implementation, we'd use the encoder's dynamic APIs
        debug!("Bitrate updated to {} kbps", bitrate_kbps);
        Ok(())
    }

    fn set_fps(&mut self, fps: u32) -> EncoderResult<()> {
        self.config.fps = fps;
        debug!("FPS updated to {}", fps);
        Ok(())
    }

    fn config(&self) -> &EncoderConfig {
        &self.config
    }

    fn stats(&self) -> EncoderStats {
        let mut stats = self.stats.clone();
        if self.stats.frames_encoded > 0 {
            let elapsed_secs = self.stats.frames_encoded as f64 / self.config.fps as f64;
            stats.current_fps = self.stats.frames_encoded as f64 / elapsed_secs;
        }
        stats
    }

    fn flush(&mut self) -> EncoderResult<Vec<EncodedFrame>> {
        // OpenH264 doesn't buffer frames in low-latency mode
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encoder_creation() {
        let encoder = OpenH264Encoder::new();
        assert!(encoder.encoder.is_none());
    }
}
