//! FPS calculation utilities for frame-rate tracking.

/// Compute instantaneous FPS from frame count and elapsed time.
///
/// Returns `Some(fps)` when elapsed_ms > 0, else `None`.
pub fn compute_instantaneous_fps(frames_done: u32, elapsed_ms: u64) -> Option<f32> {
    if elapsed_ms == 0 {
        return None;
    }
    Some((frames_done as f32 * 1000.0) / elapsed_ms as f32)
}

/// Tracks frame count and timing for FPS calculation.
///
/// Handles the "frames since last log" logic: before the first log, uses
/// total frames / uptime; after a log, uses frames_since_log / elapsed.
pub struct FpsTracker {
    last_log_time_ms: u64,
    last_log_frame: u32,
}

impl FpsTracker {
    /// Create a new tracker. Call `record_log` when you hit a log interval.
    pub fn new(now_ms: u64) -> Self {
        Self {
            last_log_time_ms: now_ms,
            last_log_frame: 0,
        }
    }

    /// Last log timestamp (for FPS log display).
    pub fn last_log_time_ms(&self) -> u64 {
        self.last_log_time_ms
    }

    /// Last log frame count (for FPS log display).
    pub fn last_log_frame(&self) -> u32 {
        self.last_log_frame
    }

    /// Record that we logged at this frame/time. Updates internal state.
    pub fn record_log(&mut self, frame_count: u32, now_ms: u64) {
        self.last_log_time_ms = now_ms;
        self.last_log_frame = frame_count;
    }

    /// Get instantaneous FPS.
    ///
    /// - When `frame_count > last_log_frame`: uses (frame_count - last_log_frame) / elapsed
    /// - When `frame_count > 0` but no log yet: uses frame_count / uptime
    /// - Otherwise: returns None
    pub fn instantaneous_fps(&self, frame_count: u32, now_ms: u64, startup_ms: u64) -> Option<f32> {
        if frame_count > self.last_log_frame {
            let elapsed_ms = now_ms.saturating_sub(self.last_log_time_ms);
            let frames_done = frame_count - self.last_log_frame;
            compute_instantaneous_fps(frames_done, elapsed_ms)
        } else if frame_count > 0 {
            let uptime_ms = now_ms.saturating_sub(startup_ms);
            compute_instantaneous_fps(frame_count, uptime_ms)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_instantaneous_fps() {
        assert_eq!(compute_instantaneous_fps(60, 1000), Some(60.0));
        assert_eq!(compute_instantaneous_fps(30, 500), Some(60.0));
        assert_eq!(compute_instantaneous_fps(0, 1000), Some(0.0));
        assert_eq!(compute_instantaneous_fps(60, 0), None);
    }

    #[test]
    fn test_fps_tracker_before_first_log() {
        let tracker = FpsTracker::new(0);
        assert_eq!(tracker.instantaneous_fps(30, 1000, 0), Some(30.0));
        assert_eq!(tracker.instantaneous_fps(60, 2000, 0), Some(30.0));
    }

    #[test]
    fn test_fps_tracker_after_log() {
        let mut tracker = FpsTracker::new(0);
        tracker.record_log(60, 1000);
        assert_eq!(tracker.instantaneous_fps(90, 1500, 0), Some(60.0));
    }
}
