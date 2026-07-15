//! Rolling per-card and aggregate statistics for the preview lab (PoC A).
//!
//! Pure host-testable math: the wasm-side lab runner feeds one
//! [`PreviewFrameSample`] per presented frame and reads snapshots for the
//! stats overlay and the automation JSON. Times are milliseconds from
//! `performance.now()`-style clocks.

use std::collections::VecDeque;

/// Horizon for rate/mean computation. Old samples age out so the overlay
/// tracks the current configuration rather than the whole run.
const WINDOW_MS: f64 = 2_000.0;

/// Per-frame cost breakdown for one presented preview frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreviewFrameSample {
    /// Engine tick inside the worker (wasm call bracketing).
    pub tick_ms: f64,
    /// Bus resolve + texture materialization + RGBA8 conversion in the worker.
    pub render_ms: f64,
    /// Worker `postMessage` → main-thread processing latency (includes poll
    /// granularity of the page loop).
    pub transport_ms: f64,
    /// Canvas `putImageData` on the main thread.
    pub present_ms: f64,
}

/// Rolling stats for one preview card.
#[derive(Debug, Default)]
pub struct CardStats {
    samples: VecDeque<(f64, PreviewFrameSample)>,
    pub presented_frames: u64,
    /// Frames skipped by backpressure (previous frame still in flight when
    /// the schedule came due).
    pub dropped_frames: u64,
    /// Frames that returned a preview error.
    pub error_frames: u64,
}

/// Point-in-time view of one card's rolling window.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
pub struct CardStatsSnapshot {
    pub achieved_fps: f64,
    pub mean_tick_ms: f64,
    pub mean_render_ms: f64,
    pub mean_transport_ms: f64,
    pub mean_present_ms: f64,
    pub presented_frames: u64,
    pub dropped_frames: u64,
    pub error_frames: u64,
}

impl CardStats {
    pub fn record(&mut self, now_ms: f64, sample: PreviewFrameSample) {
        self.presented_frames += 1;
        self.samples.push_back((now_ms, sample));
        self.prune(now_ms);
    }

    pub fn record_dropped(&mut self) {
        self.dropped_frames += 1;
    }

    pub fn record_error(&mut self) {
        self.error_frames += 1;
    }

    pub fn snapshot(&mut self, now_ms: f64) -> CardStatsSnapshot {
        self.prune(now_ms);
        let count = self.samples.len();
        let mut snapshot = CardStatsSnapshot {
            presented_frames: self.presented_frames,
            dropped_frames: self.dropped_frames,
            error_frames: self.error_frames,
            ..CardStatsSnapshot::default()
        };
        if count == 0 {
            return snapshot;
        }
        let window_start = self.samples.front().map(|(t, _)| *t).unwrap_or(now_ms);
        // Rate over the observed span (capped at the window) so short runs
        // do not read as artificially slow.
        let span_ms = (now_ms - window_start).clamp(1.0, WINDOW_MS);
        snapshot.achieved_fps = count as f64 * 1_000.0 / span_ms;
        for (_, sample) in &self.samples {
            snapshot.mean_tick_ms += sample.tick_ms;
            snapshot.mean_render_ms += sample.render_ms;
            snapshot.mean_transport_ms += sample.transport_ms;
            snapshot.mean_present_ms += sample.present_ms;
        }
        let n = count as f64;
        snapshot.mean_tick_ms /= n;
        snapshot.mean_render_ms /= n;
        snapshot.mean_transport_ms /= n;
        snapshot.mean_present_ms /= n;
        snapshot
    }

    fn prune(&mut self, now_ms: f64) {
        while let Some((t, _)) = self.samples.front() {
            if now_ms - *t > WINDOW_MS {
                self.samples.pop_front();
            } else {
                break;
            }
        }
    }
}

/// Aggregate view across all cards.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
pub struct LabAggregate {
    /// Sum of per-card achieved fps.
    pub total_fps: f64,
    /// Mean per-frame phase costs across all cards' windows.
    pub mean_tick_ms: f64,
    pub mean_render_ms: f64,
    pub mean_transport_ms: f64,
    pub mean_present_ms: f64,
    /// Estimated worker-side CPU cores consumed:
    /// `Σ (tick + render) × frame rate / 1000`.
    pub est_worker_cores: f64,
    /// Estimated main-thread cores consumed by canvas presentation.
    pub est_present_cores: f64,
    pub total_dropped: u64,
    pub total_errors: u64,
}

pub fn aggregate(snapshots: &[CardStatsSnapshot]) -> LabAggregate {
    let mut agg = LabAggregate::default();
    let mut active = 0usize;
    for snap in snapshots {
        agg.total_fps += snap.achieved_fps;
        agg.total_dropped += snap.dropped_frames;
        agg.total_errors += snap.error_frames;
        if snap.achieved_fps > 0.0 {
            active += 1;
            agg.mean_tick_ms += snap.mean_tick_ms;
            agg.mean_render_ms += snap.mean_render_ms;
            agg.mean_transport_ms += snap.mean_transport_ms;
            agg.mean_present_ms += snap.mean_present_ms;
            agg.est_worker_cores +=
                (snap.mean_tick_ms + snap.mean_render_ms) * snap.achieved_fps / 1_000.0;
            agg.est_present_cores += snap.mean_present_ms * snap.achieved_fps / 1_000.0;
        }
    }
    if active > 0 {
        let n = active as f64;
        agg.mean_tick_ms /= n;
        agg.mean_render_ms /= n;
        agg.mean_transport_ms /= n;
        agg.mean_present_ms /= n;
    }
    agg
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(tick: f64, render: f64, transport: f64, present: f64) -> PreviewFrameSample {
        PreviewFrameSample {
            tick_ms: tick,
            render_ms: render,
            transport_ms: transport,
            present_ms: present,
        }
    }

    #[test]
    fn snapshot_reports_means_and_fps_over_span() {
        let mut stats = CardStats::default();
        // 10 frames over 900ms → ~11 fps over the observed span.
        for i in 0..10 {
            stats.record(i as f64 * 100.0, sample(2.0, 4.0, 1.0, 0.5));
        }

        let snap = stats.snapshot(900.0);

        assert_eq!(snap.presented_frames, 10);
        assert!((snap.mean_tick_ms - 2.0).abs() < 1e-9);
        assert!((snap.mean_render_ms - 4.0).abs() < 1e-9);
        assert!((snap.mean_transport_ms - 1.0).abs() < 1e-9);
        assert!((snap.mean_present_ms - 0.5).abs() < 1e-9);
        assert!((snap.achieved_fps - 10_000.0 / 900.0).abs() < 1e-6);
    }

    #[test]
    fn old_samples_age_out_of_the_window() {
        let mut stats = CardStats::default();
        stats.record(0.0, sample(1.0, 1.0, 1.0, 1.0));
        stats.record(5_000.0, sample(3.0, 3.0, 3.0, 3.0));

        let snap = stats.snapshot(5_000.0);

        // Only the recent sample remains in the window.
        assert!((snap.mean_tick_ms - 3.0).abs() < 1e-9);
        assert_eq!(snap.presented_frames, 2);
    }

    #[test]
    fn aggregate_sums_fps_and_estimates_cores() {
        let cards = vec![
            CardStatsSnapshot {
                achieved_fps: 10.0,
                mean_tick_ms: 2.0,
                mean_render_ms: 3.0,
                mean_transport_ms: 1.0,
                mean_present_ms: 0.5,
                presented_frames: 100,
                dropped_frames: 2,
                error_frames: 0,
            },
            CardStatsSnapshot {
                achieved_fps: 20.0,
                mean_tick_ms: 1.0,
                mean_render_ms: 1.0,
                mean_transport_ms: 1.0,
                mean_present_ms: 0.5,
                presented_frames: 200,
                dropped_frames: 0,
                error_frames: 1,
            },
        ];

        let agg = aggregate(&cards);

        assert!((agg.total_fps - 30.0).abs() < 1e-9);
        assert_eq!(agg.total_dropped, 2);
        assert_eq!(agg.total_errors, 1);
        // (2+3)*10/1000 + (1+1)*20/1000 = 0.05 + 0.04
        assert!((agg.est_worker_cores - 0.09).abs() < 1e-9);
        // 0.5*10/1000 + 0.5*20/1000 = 0.015
        assert!((agg.est_present_cores - 0.015).abs() < 1e-9);
    }

    #[test]
    fn empty_window_snapshot_is_zeroed() {
        let mut stats = CardStats::default();
        stats.record_dropped();

        let snap = stats.snapshot(100.0);

        assert_eq!(snap.achieved_fps, 0.0);
        assert_eq!(snap.dropped_frames, 1);
    }
}
