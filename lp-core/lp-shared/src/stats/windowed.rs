//! Windowed sample collector for time-series stats (e.g. FPS over 5s).
//!
//! Maintains running sum and sum of squares to avoid iterating over samples on
//! every stats computation. Float math is expensive on embedded (e.g. ESP32).

use alloc::vec::Vec;

use lp_model::server::SampleStats;

/// Default minimum samples before using mean for avg (avoids cold-start dilution).
const DEFAULT_MIN_SAMPLES_FOR_AVG: usize = 3;

/// Collects timestamped samples and computes stats over a sliding time window.
///
/// Uses incremental sums (sum, sum_sq) so compute_stats does minimal float work.
pub struct WindowedStatsCollector {
    /// (timestamp_ms, value) - needed for prune and min/max
    samples: Vec<(u64, f32)>,
    /// Running sum of values (updated on push/prune)
    sum: f32,
    /// Running sum of value^2 (updated on push/prune)
    sum_sq: f32,
    min_samples_for_avg: usize,
}

impl WindowedStatsCollector {
    /// Create a new collector.
    pub fn new() -> Self {
        Self {
            samples: Vec::new(),
            sum: 0.0,
            sum_sq: 0.0,
            min_samples_for_avg: DEFAULT_MIN_SAMPLES_FOR_AVG,
        }
    }

    /// Create with custom min_samples_for_avg.
    pub fn with_min_samples_for_avg(min_samples: usize) -> Self {
        Self {
            samples: Vec::new(),
            sum: 0.0,
            sum_sq: 0.0,
            min_samples_for_avg: min_samples,
        }
    }

    /// Add a sample. Call after prune_older_than to keep the window bounded.
    pub fn push(&mut self, timestamp_ms: u64, value: f32) {
        self.sum += value;
        self.sum_sq += value * value;
        self.samples.push((timestamp_ms, value));
    }

    /// Remove samples older than cutoff_ms (retain timestamp >= cutoff_ms).
    pub fn prune_older_than(&mut self, cutoff_ms: u64) {
        let mut retained = Vec::new();
        for (ts, v) in core::mem::take(&mut self.samples) {
            if ts >= cutoff_ms {
                retained.push((ts, v));
            } else {
                self.sum -= v;
                self.sum_sq -= v * v;
            }
        }
        self.samples = retained;
    }

    /// Compute stats. Uses cached sum/sum_sq; only iterates for min/max (n <= 5).
    pub fn compute_stats(&self) -> SampleStats {
        let n = self.samples.len();
        if n == 0 {
            return SampleStats {
                avg: 0.0,
                sdev: 0.0,
                min: 0.0,
                max: 0.0,
            };
        }

        let min = self
            .samples
            .iter()
            .map(|(_, v)| *v)
            .fold(f32::INFINITY, f32::min);
        let max = self
            .samples
            .iter()
            .map(|(_, v)| *v)
            .fold(f32::NEG_INFINITY, f32::max);

        let (avg, sdev) = if n < self.min_samples_for_avg {
            let last = self.samples[n - 1].1;
            (last, 0.0)
        } else {
            let avg = self.sum / n as f32;
            let variance = (self.sum_sq / n as f32) - (avg * avg);
            let variance = if variance > 0.0 { variance } else { 0.0 };
            let sdev = libm::sqrtf(variance);
            (avg, sdev)
        };

        SampleStats {
            avg,
            sdev,
            min,
            max,
        }
    }
}

impl Default for WindowedStatsCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_windowed_prune() {
        let mut c = WindowedStatsCollector::new();
        c.push(1000, 50.0);
        c.push(2000, 60.0);
        c.push(3000, 55.0);
        c.push(6000, 65.0);

        c.prune_older_than(2500);
        let values: Vec<f32> = c.samples.iter().map(|(_, v)| *v).collect();
        assert_eq!(values, [55.0, 65.0]);
    }

    #[test]
    fn test_windowed_stats_after_prune() {
        let mut c = WindowedStatsCollector::new();
        c.push(1000, 40.0);
        c.push(2000, 60.0);
        c.push(3000, 50.0);
        c.push(4000, 54.0);

        c.prune_older_than(1500);
        let stats = c.compute_stats();
        assert!((stats.avg - 54.67).abs() < 0.1);
        assert_eq!(stats.min, 50.0);
        assert_eq!(stats.max, 60.0);
    }

    #[test]
    fn test_windowed_few_samples_uses_last() {
        let mut c = WindowedStatsCollector::new();
        c.push(1000, 10.0);
        c.push(2000, 30.0);

        let stats = c.compute_stats();
        assert_eq!(stats.avg, 30.0);
        assert_eq!(stats.sdev, 0.0);
    }
}
