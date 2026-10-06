//! How often the UART link task may run a pass: the least time between two
//! passes' starts, so a link task that preempts the render does it rarely.
//!
//! On its own thread (the classic's `io-thread`) every pass preempts the
//! render on core 0, and on the classic's silicon a preemption costs the
//! render far more than the pass itself: a pass is ~0.5–0.9 ms, but a frame
//! that took one renders ~4–5 ms slower (the render's code and data are
//! fetched again through the flash cache the pass displaced). An
//! event-driven link task wakes on every piece of I/O-task news — during a
//! transfer that is 300–450 passes a second — and the render stretched to
//! ~190 ms frames under PR #943's first arrangement.
//!
//! Holding the next pass until [`PassPacing::min_interval_us`] after the last
//! one began bounds that. 25 ms on the classic (PR #943's desk sitting,
//! 2026-10-03, DOM-Z-102, a ~16 fps project, `lp-cli link rtt`; frame
//! interval p90 idle / transfers / requests):
//!
//! | link task                         | frames p90, ms     | request RTT p50 | transfers  |
//! |-----------------------------------|--------------------|-----------------|------------|
//! | main executor (main)              | 62.9 / 67.1 / 66.9 | 108 ms          | 13 KiB/s   |
//! | own thread, every event           | 62.8 / 194 / 71.0  | 46 ms           | 56–68 KiB/s|
//! | own thread, ≥ 10 ms between passes| 66.8 / 114 / 79.5  | 54 ms           | 50–60 KiB/s|
//! | own thread, ≥ 25 ms between passes| 66.9 / 87 / 74.5   | 55 ms           | 32 KiB/s   |
//!
//! The hold is cut short by nothing: a log burst, a queued reply and the
//! I/O task's news all wait for it. The pipes are what make that safe — the
//! TX pipe holds a pass's frames while the I/O task drains them, and the RX
//! pipe ([`super::uart_link_pipes::RX_PIPE_BYTES`]) catches what arrives in
//! between; the link's own ARQ resends anything a full pipe drops. Without
//! a thread of its own the task runs only between frames, and a hold would
//! only slow the link: that arrangement uses [`PassPacing::EVERY_EVENT`].

use lp_link::Micros;

/// The least time between the starts of two link passes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PassPacing {
    /// Microseconds; 0 runs a pass on every wake.
    pub min_interval_us: Micros,
}

impl PassPacing {
    /// A pass on every wake: a link task that only runs between frames.
    pub const EVERY_EVENT: Self = Self { min_interval_us: 0 };

    /// The classic's link thread: at most one pass every 25 ms (see the
    /// module docs for the measurements that chose it).
    pub const CLASSIC_LINK_THREAD: Self = Self {
        min_interval_us: 25_000,
    };

    /// When the pass after one that began at `pass_started` may begin,
    /// given that its wake came at `now`: `None` to run it at once, or the
    /// time to hold it until.
    pub fn hold_until(&self, pass_started: Micros, now: Micros) -> Option<Micros> {
        let earliest = pass_started.saturating_add(self.min_interval_us);
        (now < earliest).then_some(earliest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_event_never_holds_a_pass() {
        assert_eq!(PassPacing::EVERY_EVENT.hold_until(1_000, 1_000), None);
        assert_eq!(PassPacing::EVERY_EVENT.hold_until(1_000, 1_001), None);
    }

    #[test]
    fn a_wake_inside_the_interval_waits_for_its_end() {
        let pacing = PassPacing::CLASSIC_LINK_THREAD;
        assert_eq!(pacing.hold_until(1_000, 1_000), Some(26_000));
        assert_eq!(pacing.hold_until(1_000, 25_999), Some(26_000));
    }

    #[test]
    fn a_wake_after_the_interval_runs_at_once() {
        let pacing = PassPacing::CLASSIC_LINK_THREAD;
        assert_eq!(pacing.hold_until(1_000, 26_000), None);
        assert_eq!(pacing.hold_until(1_000, 90_000), None);
    }

    /// Back-to-back events — a transfer's I/O-task news every millisecond —
    /// run at most one pass per interval, however many wakes arrive.
    #[test]
    fn a_wake_every_millisecond_runs_forty_passes_a_second() {
        let pacing = PassPacing::CLASSIC_LINK_THREAD;
        let mut passes = 0;
        let mut last_start: Option<Micros> = None;
        let mut now: Micros = 0;
        while now < 1_000_000 {
            let start = match last_start.and_then(|s| pacing.hold_until(s, now)) {
                Some(held) => held,
                None => now,
            };
            if start >= 1_000_000 {
                break;
            }
            passes += 1;
            last_start = Some(start);
            // The next wake: the I/O task's news a millisecond after the
            // pass began.
            now = start + 1_000;
        }
        assert_eq!(passes, 40);
    }

    #[test]
    fn the_classic_thread_holds_for_25_ms() {
        assert_eq!(PassPacing::CLASSIC_LINK_THREAD.min_interval_us, 25_000);
    }
}
