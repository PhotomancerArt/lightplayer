//! A LAN with a host on it runs at the host's pace.
//!
//! A host connected through a port forward ([`super::LanPortForward`]) is a
//! wall-clock peer: its timers (lp-link's resend timer and tail probe, a TCP
//! stack's) count host time, and so does every delay on its side of the
//! wall. The board's timers count guest time, and an idle board's guest time
//! runs far ahead of the host's: a `wfi` skips straight to the next timer,
//! so a few milliseconds the host takes to answer are tens or hundreds of
//! the board's. The board's own resend timer then fires on frames the host
//! has not had time to acknowledge, and an upload over the LAN is resent
//! frame after frame though nothing was lost (PR C's finding: 40–95 resent
//! per upload).
//!
//! So while a host is connected, each board's guest clock is held to the
//! host's: [`HostPace::wait`] says how long a board that has run ahead must
//! wait before it goes on. That is the unset pace; a run whose pace is
//! `realtime` is held the same way for its whole length, host or no host,
//! and one whose pace is `max` never is ([`super::lan_pace`]). Never the other way: a board slower than the host
//! (a shader compiling) is not hurried, and the moment it is level or behind
//! the pace starts again from there, so it never sprints to catch up.
//!
//! What that costs: a host's sleep overshoots what it was asked for (a 1 ms
//! sleep is about 1.5 ms on macOS) and the overshoot is never made up, so an
//! idle board with a host connected runs at about two thirds of the host's
//! speed (measured 0.65×). A pace that made the overshoot up (up to 10 ms of
//! catch-up) did run at 1.00×, but under it the shipped image's io task went
//! silent for over 2 s of its own time and one request took 3 s, in 3 runs of
//! 3, and never without it (2 of 2): until that is understood, a board that
//! has fallen behind is never hurried at all.
//!
//! The wall clock is passed in, never read here, so the rule is a pure
//! function of its readings.

use std::time::{Duration, Instant};

use lp_emu_core::sched::Cycles;

/// The longest one wait lasts. A board can be further ahead than this only
/// after a skip nothing bounded; it waits again at its next pump.
pub const MAX_WAIT: Duration = Duration::from_millis(50);

/// One board's pace against the host's clock (a connected host's, or wall
/// time's for a `realtime` run).
#[derive(Clone, Copy, Debug, Default)]
pub struct HostPace {
    /// A guest cycle and the host time it was level with.
    anchor: Option<(Cycles, Instant)>,
}

impl HostPace {
    /// How long the board, its guest clock at `guest` when the host's reads
    /// `wall`, must wait to be no further ahead of the host than it was at
    /// the anchor. Zero when it is level or behind, and then the anchor
    /// moves here: a board that fell behind is not owed a sprint.
    pub fn wait(&mut self, guest: Cycles, cycles_per_us: u64, wall: Instant) -> Duration {
        let Some((g0, w0)) = self.anchor else {
            self.anchor = Some((guest, wall));
            return Duration::ZERO;
        };
        // A restarted guest's clock reads from zero again.
        let Some(ran) = guest.checked_sub(g0) else {
            self.anchor = Some((guest, wall));
            return Duration::ZERO;
        };
        let guest_us = ran / cycles_per_us.max(1);
        let host_us =
            u64::try_from(wall.saturating_duration_since(w0).as_micros()).unwrap_or(u64::MAX);
        if guest_us <= host_us {
            self.anchor = Some((guest, wall));
            return Duration::ZERO;
        }
        Duration::from_micros(guest_us - host_us).min(MAX_WAIT)
    }

    /// The host left (or the pace no longer holds the board): the next one
    /// starts a new pace.
    pub fn release(&mut self) {
        self.anchor = None;
    }

    /// Whether the board is being held to the host's clock.
    pub fn engaged(&self) -> bool {
        self.anchor.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_board_ahead_of_the_host_waits_the_difference() {
        let t0 = Instant::now();
        let mut pace = HostPace::default();
        assert_eq!(
            pace.wait(0, CPU, t0),
            Duration::ZERO,
            "the first reading anchors"
        );
        // 3 ms of guest time in 1 ms of host time: 2 ms ahead.
        assert_eq!(pace.wait(3 * MS, CPU, t0 + ms(1)), ms(2));
        // Still ahead after waiting part of it: the anchor did not move.
        assert_eq!(pace.wait(3 * MS, CPU, t0 + ms(2)), ms(1));
        assert_eq!(pace.wait(3 * MS, CPU, t0 + ms(3)), Duration::ZERO);
    }

    #[test]
    fn a_board_behind_the_host_is_not_hurried_and_owes_no_sprint() {
        let t0 = Instant::now();
        let mut pace = HostPace::default();
        pace.wait(0, CPU, t0);
        // A compile: 1 ms of guest time took 100 ms of host time.
        assert_eq!(pace.wait(MS, CPU, t0 + ms(100)), Duration::ZERO);
        // Idle again: 5 ms of guest time at once is 5 ms ahead of where the
        // board was level, not 94 ms of credit.
        assert_eq!(pace.wait(6 * MS, CPU, t0 + ms(100)), ms(5));
    }

    #[test]
    fn an_overshot_wait_is_not_made_up() {
        let t0 = Instant::now();
        let mut pace = HostPace::default();
        pace.wait(0, CPU, t0);
        // A 1 ms wait that the host's sleep made 1.5 ms: half a ms behind,
        // and the pace starts again from there…
        assert_eq!(pace.wait(MS, CPU, t0 + us(1_500)), Duration::ZERO);
        // …so the next millisecond of guest time is waited for in full.
        assert_eq!(pace.wait(2 * MS, CPU, t0 + us(1_500)), ms(1));
    }

    #[test]
    fn a_long_skip_waits_at_most_the_cap_per_reading() {
        let t0 = Instant::now();
        let mut pace = HostPace::default();
        pace.wait(0, CPU, t0);
        assert_eq!(pace.wait(1_000 * MS, CPU, t0), MAX_WAIT);
    }

    #[test]
    fn a_restart_or_a_release_starts_a_new_pace() {
        let t0 = Instant::now();
        let mut pace = HostPace::default();
        pace.wait(50 * MS, CPU, t0);
        // The guest restarted: its clock reads from zero.
        assert_eq!(pace.wait(MS, CPU, t0), Duration::ZERO);
        assert_eq!(pace.wait(2 * MS, CPU, t0), ms(1));
        pace.release();
        assert!(!pace.engaged());
        assert_eq!(pace.wait(9 * MS, CPU, t0), Duration::ZERO);
        assert!(pace.engaged());
    }

    const CPU: u64 = 160;
    const MS: Cycles = 160_000;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    fn us(n: u64) -> Duration {
        Duration::from_micros(n)
    }
}
