//! Sample statistics (avg, sdev, min, max) for scalar metrics like FPS.

pub mod windowed;

pub use windowed::WindowedStatsCollector;

use lp_model::server::SampleStats;

/// Compute SampleStats (avg, population sdev, min, max) from values.
///
/// When `n < min_samples_for_avg`, uses the most recent sample for `avg` instead of
/// the mean, preventing startup/cold samples from diluting the reported rate.
/// `sdev` is 0 when n < 2.
///
/// # Arguments
///
/// * `values` - Sample values (e.g. FPS readings)
/// * `min_samples_for_avg` - Minimum samples before using mean for avg (default 3)
pub fn compute_sample_stats(values: &[f32], min_samples_for_avg: usize) -> SampleStats {
    const DEFAULT_MIN_SAMPLES: usize = 3;

    let min_samples = if min_samples_for_avg == 0 {
        DEFAULT_MIN_SAMPLES
    } else {
        min_samples_for_avg
    };

    let n = values.len();
    if n == 0 {
        return SampleStats {
            avg: 0.0,
            sdev: 0.0,
            min: 0.0,
            max: 0.0,
        };
    }

    let min = values.iter().copied().fold(f32::INFINITY, f32::min);
    let max = values.iter().copied().fold(f32::NEG_INFINITY, f32::max);

    let (avg, sdev) = if n < min_samples {
        let last = values[n - 1];
        (last, 0.0)
    } else {
        let sum: f32 = values.iter().sum();
        let avg = sum / n as f32;
        let variance = if n == 1 {
            0.0
        } else {
            values
                .iter()
                .map(|x| {
                    let d = x - avg;
                    d * d
                })
                .sum::<f32>()
                / n as f32
        };
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_sample_stats_empty() {
        let stats = compute_sample_stats(&[], 3);
        assert_eq!(stats.avg, 0.0);
        assert_eq!(stats.sdev, 0.0);
        assert_eq!(stats.min, 0.0);
        assert_eq!(stats.max, 0.0);
    }

    #[test]
    fn test_compute_sample_stats_single() {
        let stats = compute_sample_stats(&[42.0], 3);
        assert_eq!(stats.avg, 42.0);
        assert_eq!(stats.sdev, 0.0);
        assert_eq!(stats.min, 42.0);
        assert_eq!(stats.max, 42.0);
    }

    #[test]
    fn test_compute_sample_stats_two() {
        let stats = compute_sample_stats(&[40.0, 60.0], 3);
        assert_eq!(stats.avg, 60.0);
        assert_eq!(stats.sdev, 0.0);
        assert_eq!(stats.min, 40.0);
        assert_eq!(stats.max, 60.0);
    }

    #[test]
    fn test_compute_sample_stats_few_samples_uses_last() {
        let stats = compute_sample_stats(&[10.0, 20.0], 3);
        assert_eq!(stats.avg, 20.0);
        assert_eq!(stats.sdev, 0.0);
        assert_eq!(stats.min, 10.0);
        assert_eq!(stats.max, 20.0);
    }

    #[test]
    fn test_compute_sample_stats_enough_samples_uses_mean() {
        let stats = compute_sample_stats(&[40.0, 50.0, 60.0], 3);
        assert!((stats.avg - 50.0).abs() < 0.01);
        assert!(stats.sdev > 0.0);
    }
}
