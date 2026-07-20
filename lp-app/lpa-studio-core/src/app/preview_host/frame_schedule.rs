//! Pure per-slot present scheduling: deadlines, backpressure, delta clamp.
//!
//! Extracted from the preview-lab's proven loop (`lab_runner.rs`'s
//! `schedule_due_frames`) so the decisions are unit-testable without a
//! browser: time arrives as `f64` milliseconds from the caller's clock.

/// Ceiling on a single tick delta so a stalled slot (resume after suspend,
/// hidden-tab throttling, long deploy) does not fast-forward its sim.
pub const MAX_TICK_DELTA_MS: f64 = 250.0;

/// What to do for one slot at one scheduler poll.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FrameDecision {
    /// Not started or not due yet.
    Wait,
    /// Due, but the previous frame has not been answered — drop this
    /// period rather than queueing further behind (backpressure).
    Skip,
    /// Post a frame advancing the runtime clock by `delta_ms`.
    Send {
        /// Clamped clock advance for the tick riding this frame.
        delta_ms: u32,
    },
}

/// Deadline state for one slot.
#[derive(Clone, Debug)]
pub struct FrameSchedule {
    period_ms: f64,
    /// `None` while paused (suspended / not yet live).
    next_due_ms: Option<f64>,
    last_tick_at_ms: Option<f64>,
    in_flight: bool,
}

impl FrameSchedule {
    /// A paused schedule at `fps` (non-positive fps falls back to 1).
    pub fn new(fps: f32) -> Self {
        let fps = if fps > 0.0 { f64::from(fps) } else { 1.0 };
        Self {
            period_ms: 1_000.0 / fps,
            next_due_ms: None,
            last_tick_at_ms: None,
            in_flight: false,
        }
    }

    /// Start (or resume) presenting: the next frame is due immediately.
    pub fn start(&mut self, now_ms: f64) {
        self.next_due_ms = Some(now_ms);
    }

    /// Stop presenting (suspend). An in-flight frame may still complete.
    pub fn pause(&mut self) {
        self.next_due_ms = None;
    }

    /// Whether a posted frame is awaiting its completion.
    pub fn in_flight(&self) -> bool {
        self.in_flight
    }

    /// Decide for `now_ms`, updating deadlines. On [`FrameDecision::Send`]
    /// the frame counts as posted (in flight) until
    /// [`Self::frame_completed`] / [`Self::frame_failed`].
    pub fn poll(&mut self, now_ms: f64) -> FrameDecision {
        let Some(due) = self.next_due_ms else {
            return FrameDecision::Wait;
        };
        if now_ms < due {
            return FrameDecision::Wait;
        }
        if self.in_flight {
            self.next_due_ms = Some(due + self.period_ms);
            return FrameDecision::Skip;
        }
        let delta = self
            .last_tick_at_ms
            .map(|last| (now_ms - last).clamp(1.0, MAX_TICK_DELTA_MS))
            .unwrap_or(self.period_ms.min(MAX_TICK_DELTA_MS));
        self.in_flight = true;
        self.last_tick_at_ms = Some(now_ms);
        // Keep phase but avoid runaway catch-up bursts after stalls.
        let mut next = due + self.period_ms;
        if next < now_ms {
            next = now_ms + self.period_ms;
        }
        self.next_due_ms = Some(next);
        FrameDecision::Send {
            delta_ms: delta.round().max(1.0) as u32,
        }
    }

    /// The posted frame completed (present ack or pixel frame arrived).
    pub fn frame_completed(&mut self) {
        self.in_flight = false;
    }

    /// The posted frame failed, or the worker holding it is gone.
    pub fn frame_failed(&mut self) {
        self.in_flight = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paused_schedule_never_sends() {
        let mut schedule = FrameSchedule::new(12.0);
        assert_eq!(schedule.poll(0.0), FrameDecision::Wait);
        assert_eq!(schedule.poll(10_000.0), FrameDecision::Wait);
    }

    #[test]
    fn first_frame_after_start_is_due_immediately_with_period_delta() {
        let mut schedule = FrameSchedule::new(10.0); // 100 ms period
        schedule.start(1_000.0);
        assert_eq!(
            schedule.poll(1_000.0),
            FrameDecision::Send { delta_ms: 100 },
            "no prior tick: the delta is one period"
        );
        assert!(schedule.in_flight());
    }

    #[test]
    fn next_frame_waits_out_the_period_and_carries_the_real_delta() {
        let mut schedule = FrameSchedule::new(10.0);
        schedule.start(0.0);
        assert!(matches!(schedule.poll(0.0), FrameDecision::Send { .. }));
        schedule.frame_completed();
        assert_eq!(schedule.poll(50.0), FrameDecision::Wait, "mid-period");
        assert_eq!(
            schedule.poll(120.0),
            FrameDecision::Send { delta_ms: 120 },
            "real measured delta since the last tick"
        );
    }

    #[test]
    fn in_flight_frame_backpressures_to_a_skip_and_keeps_phase() {
        let mut schedule = FrameSchedule::new(10.0);
        schedule.start(0.0);
        assert!(matches!(schedule.poll(0.0), FrameDecision::Send { .. }));
        // Never completed: the next due poll drops the period.
        assert_eq!(schedule.poll(100.0), FrameDecision::Skip);
        assert!(schedule.in_flight(), "skip does not clear the flight");
        assert_eq!(schedule.poll(150.0), FrameDecision::Wait, "next period");
        schedule.frame_completed();
        assert!(matches!(schedule.poll(200.0), FrameDecision::Send { .. }));
    }

    #[test]
    fn delta_is_clamped_after_a_stall() {
        let mut schedule = FrameSchedule::new(10.0);
        schedule.start(0.0);
        assert!(matches!(schedule.poll(0.0), FrameDecision::Send { .. }));
        schedule.frame_completed();
        // 10 s stall (hidden tab, long deploy elsewhere): no fast-forward.
        assert_eq!(
            schedule.poll(10_000.0),
            FrameDecision::Send {
                delta_ms: MAX_TICK_DELTA_MS as u32
            }
        );
    }

    #[test]
    fn deadlines_do_not_burst_after_a_stall() {
        let mut schedule = FrameSchedule::new(10.0);
        schedule.start(0.0);
        assert!(matches!(schedule.poll(0.0), FrameDecision::Send { .. }));
        schedule.frame_completed();
        assert!(matches!(schedule.poll(5_000.0), FrameDecision::Send { .. }));
        schedule.frame_completed();
        // The next due is re-anchored past the stall, not 4.9 s in the past.
        assert_eq!(schedule.poll(5_001.0), FrameDecision::Wait);
        assert!(matches!(schedule.poll(5_100.0), FrameDecision::Send { .. }));
    }

    #[test]
    fn pause_stops_sending_and_resume_reanchors() {
        let mut schedule = FrameSchedule::new(10.0);
        schedule.start(0.0);
        assert!(matches!(schedule.poll(0.0), FrameDecision::Send { .. }));
        schedule.frame_completed();
        schedule.pause();
        assert_eq!(schedule.poll(1_000.0), FrameDecision::Wait);
        schedule.start(2_000.0);
        // Resume delta is clamped: the sim does not replay the 2 s gap.
        assert_eq!(
            schedule.poll(2_000.0),
            FrameDecision::Send {
                delta_ms: MAX_TICK_DELTA_MS as u32
            }
        );
    }

    #[test]
    fn non_positive_fps_falls_back_to_one_fps() {
        let mut schedule = FrameSchedule::new(0.0);
        schedule.start(0.0);
        assert_eq!(schedule.poll(0.0), FrameDecision::Send { delta_ms: 250 });
        schedule.frame_completed();
        assert_eq!(schedule.poll(500.0), FrameDecision::Wait);
        assert!(matches!(schedule.poll(1_000.0), FrameDecision::Send { .. }));
    }
}
