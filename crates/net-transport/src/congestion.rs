//! Congestion control and adaptive bitrate

use parking_lot::RwLock;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// RTT sample
#[derive(Debug, Clone, Copy)]
struct RttSample {
    rtt: Duration,
    timestamp: Instant,
}

/// Congestion controller configuration
#[derive(Debug, Clone)]
pub struct CongestionConfig {
    /// Minimum bitrate in kbps
    pub min_bitrate_kbps: u32,
    /// Maximum bitrate in kbps
    pub max_bitrate_kbps: u32,
    /// Initial bitrate in kbps
    pub initial_bitrate_kbps: u32,
    /// Target RTT in milliseconds
    pub target_rtt_ms: u32,
    /// RTT threshold for rate reduction
    pub rtt_threshold_ms: u32,
    /// How quickly to increase rate (0.0-1.0)
    pub increase_rate: f64,
    /// How quickly to decrease rate (0.0-1.0)
    pub decrease_rate: f64,
    /// Minimum FPS
    pub min_fps: u8,
    /// Maximum FPS
    pub max_fps: u8,
}

impl Default for CongestionConfig {
    fn default() -> Self {
        Self {
            min_bitrate_kbps: 500,
            max_bitrate_kbps: 10000,
            initial_bitrate_kbps: 3000,
            target_rtt_ms: 50,
            rtt_threshold_ms: 100,
            increase_rate: 0.05,
            decrease_rate: 0.2,
            min_fps: 10,
            max_fps: 60,
        }
    }
}

/// Current encoding parameters
#[derive(Debug, Clone, Copy)]
pub struct EncodingParams {
    /// Current bitrate in kbps
    pub bitrate_kbps: u32,
    /// Current FPS
    pub fps: u8,
    /// Current quality factor (0-100)
    pub quality: u8,
}

/// Congestion controller state
pub struct CongestionController {
    config: CongestionConfig,
    current_params: RwLock<EncodingParams>,
    rtt_samples: RwLock<VecDeque<RttSample>>,
    smoothed_rtt: RwLock<Duration>,
    rtt_variance: RwLock<Duration>,
    last_adjustment: RwLock<Instant>,
}

impl CongestionController {
    /// Create a new congestion controller
    pub fn new(config: CongestionConfig) -> Self {
        let initial_params = EncodingParams {
            bitrate_kbps: config.initial_bitrate_kbps,
            fps: 30,
            quality: 70,
        };

        Self {
            config,
            current_params: RwLock::new(initial_params),
            rtt_samples: RwLock::new(VecDeque::with_capacity(100)),
            smoothed_rtt: RwLock::new(Duration::from_millis(50)),
            rtt_variance: RwLock::new(Duration::from_millis(10)),
            last_adjustment: RwLock::new(Instant::now()),
        }
    }

    /// Record a new RTT sample
    pub fn record_rtt(&self, rtt: Duration) {
        let sample = RttSample {
            rtt,
            timestamp: Instant::now(),
        };

        let mut samples = self.rtt_samples.write();
        samples.push_back(sample);

        // Keep only samples from the last 2 seconds
        let cutoff = Instant::now() - Duration::from_secs(2);
        while samples.front().is_some_and(|s| s.timestamp < cutoff) {
            samples.pop_front();
        }

        // Update smoothed RTT using exponential moving average
        let alpha = 0.125;
        let beta = 0.25;

        let mut smoothed = self.smoothed_rtt.write();
        let mut variance = self.rtt_variance.write();

        let diff = if rtt > *smoothed {
            rtt - *smoothed
        } else {
            *smoothed - rtt
        };

        *variance = Duration::from_secs_f64(
            (1.0 - beta) * variance.as_secs_f64() + beta * diff.as_secs_f64(),
        );

        *smoothed = Duration::from_secs_f64(
            (1.0 - alpha) * smoothed.as_secs_f64() + alpha * rtt.as_secs_f64(),
        );

        drop(smoothed);
        drop(variance);
        drop(samples);

        // Adjust encoding parameters based on new RTT
        self.adjust_params();
    }

    /// Adjust encoding parameters based on network conditions
    fn adjust_params(&self) {
        let mut last_adj = self.last_adjustment.write();

        // Don't adjust too frequently
        if last_adj.elapsed() < Duration::from_millis(500) {
            return;
        }
        *last_adj = Instant::now();
        drop(last_adj);

        let smoothed_rtt = *self.smoothed_rtt.read();
        let rtt_ms = smoothed_rtt.as_millis() as u32;
        let target_rtt = self.config.target_rtt_ms;
        let threshold = self.config.rtt_threshold_ms;

        let mut params = self.current_params.write();

        if rtt_ms > threshold {
            // Network is congested - reduce bitrate and potentially FPS
            let reduction = self.config.decrease_rate;
            params.bitrate_kbps = (params.bitrate_kbps as f64 * (1.0 - reduction)) as u32;
            params.bitrate_kbps = params.bitrate_kbps.max(self.config.min_bitrate_kbps);

            // If bitrate is at minimum, reduce FPS
            if params.bitrate_kbps == self.config.min_bitrate_kbps
                && params.fps > self.config.min_fps
            {
                params.fps = (params.fps as f64 * 0.8) as u8;
                params.fps = params.fps.max(self.config.min_fps);
            }

            // Reduce quality
            params.quality = (params.quality as f64 * 0.9) as u8;
            params.quality = params.quality.max(30);

            tracing::debug!(
                rtt_ms,
                bitrate_kbps = params.bitrate_kbps,
                fps = params.fps,
                "Reduced encoding params due to high RTT"
            );
        } else if rtt_ms < target_rtt {
            // Network has capacity - increase bitrate
            let increase = self.config.increase_rate;
            params.bitrate_kbps = (params.bitrate_kbps as f64 * (1.0 + increase)) as u32;
            params.bitrate_kbps = params.bitrate_kbps.min(self.config.max_bitrate_kbps);

            // If we have headroom, increase FPS
            if params.bitrate_kbps < self.config.max_bitrate_kbps
                && params.fps < self.config.max_fps
            {
                params.fps = (params.fps + 1).min(self.config.max_fps);
            }

            // Increase quality
            params.quality = (params.quality + 2).min(100);

            tracing::trace!(
                rtt_ms,
                bitrate_kbps = params.bitrate_kbps,
                fps = params.fps,
                "Increased encoding params"
            );
        }
    }

    /// Get current encoding parameters
    pub fn current_params(&self) -> EncodingParams {
        *self.current_params.read()
    }

    /// Get smoothed RTT
    pub fn smoothed_rtt(&self) -> Duration {
        *self.smoothed_rtt.read()
    }

    /// Get RTT variance
    pub fn rtt_variance(&self) -> Duration {
        *self.rtt_variance.read()
    }

    /// Force a specific bitrate (for testing or manual override)
    pub fn set_bitrate(&self, bitrate_kbps: u32) {
        let mut params = self.current_params.write();
        params.bitrate_kbps =
            bitrate_kbps.clamp(self.config.min_bitrate_kbps, self.config.max_bitrate_kbps);
    }

    /// Force a specific FPS (for testing or manual override)
    pub fn set_fps(&self, fps: u8) {
        let mut params = self.current_params.write();
        params.fps = fps.clamp(self.config.min_fps, self.config.max_fps);
    }

    /// Request a keyframe (e.g., after packet loss)
    pub fn request_keyframe(&self) -> bool {
        // This is a hint to the encoder - actual implementation
        // would need to communicate with the encoder
        tracing::info!("Keyframe requested by congestion controller");
        true
    }

    /// Get recent RTT statistics
    pub fn rtt_stats(&self) -> RttStats {
        let samples = self.rtt_samples.read();

        if samples.is_empty() {
            return RttStats::default();
        }

        let rtts: Vec<Duration> = samples.iter().map(|s| s.rtt).collect();
        let sum: Duration = rtts.iter().sum();
        let avg = sum / rtts.len() as u32;

        let min = rtts.iter().min().copied().unwrap_or_default();
        let max = rtts.iter().max().copied().unwrap_or_default();

        // Calculate jitter (average deviation)
        let jitter: Duration = if rtts.len() > 1 {
            let total_diff: Duration = rtts
                .windows(2)
                .map(|w| {
                    if w[1] > w[0] {
                        w[1] - w[0]
                    } else {
                        w[0] - w[1]
                    }
                })
                .sum();
            total_diff / (rtts.len() - 1) as u32
        } else {
            Duration::ZERO
        };

        RttStats {
            average: avg,
            min,
            max,
            jitter,
            sample_count: rtts.len(),
        }
    }
}

/// RTT statistics summary
#[derive(Debug, Clone, Default)]
pub struct RttStats {
    pub average: Duration,
    pub min: Duration,
    pub max: Duration,
    pub jitter: Duration,
    pub sample_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_congestion_controller_high_rtt() {
        let controller = CongestionController::new(CongestionConfig::default());

        let initial = controller.current_params();

        // Simulate high RTT
        for _ in 0..20 {
            controller.record_rtt(Duration::from_millis(200));
            std::thread::sleep(Duration::from_millis(100));
        }

        let after_congestion = controller.current_params();

        // Bitrate should have decreased
        assert!(after_congestion.bitrate_kbps < initial.bitrate_kbps);
    }

    #[test]
    fn test_congestion_controller_low_rtt() {
        let config = CongestionConfig {
            initial_bitrate_kbps: 1000, // Start low
            ..Default::default()
        };
        let controller = CongestionController::new(config);

        let initial = controller.current_params();

        // Simulate low RTT
        for _ in 0..20 {
            controller.record_rtt(Duration::from_millis(10));
            std::thread::sleep(Duration::from_millis(100));
        }

        let after = controller.current_params();

        // Bitrate should have increased
        assert!(after.bitrate_kbps > initial.bitrate_kbps);
    }
}
