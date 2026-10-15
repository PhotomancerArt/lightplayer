//! When the board's next picture is due.
//!
//! Fed the hub's [`PictureRate`]s and the time; answers when the next
//! picture is due, when the client must next wake for it, and whether the
//! board counts as watched. The rules (each pinned by a test in
//! `tests/relay_client_rules.rs`):
//!
//! - **Nothing until the hub asks**: there is no schedule until the first
//!   rate; the client clears it at every `Registered` and whenever the leg
//!   goes.
//! - **Each rate takes a picture at once** (clamped first,
//!   [`PictureRate::clamped`]), then follows its cadence. A picture already
//!   in flight counts as that one.
//! - **Watched** (`watched_for_s > 0`): a picture every `watched_ms` until
//!   `watched_for_s` after the rate arrived; then **idle**: the next one
//!   `idle_s` after the last (none when `idle_s` is 0). The board decays by
//!   itself; the hub needs no timer.
//! - **At most one in flight**: after a picture is asked for, the next ask
//!   waits for the next due time, answered or not. An edge that never
//!   answers is asked again then, never more often.

use crate::picture_rate::PictureRate;

/// See the module doc. Times are the client's `now_ms`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PictureSchedule {
    /// The hub's last rate, clamped; `None` until the hub asks.
    rate: Option<PictureRate>,
    /// Until when the board counts as watched (≤ the rate's arrival when
    /// not watched).
    watched_until: u64,
    /// Whether `due` follows the watched cadence.
    watching: bool,
    /// When the last picture was asked for.
    last_asked: u64,
    /// When the next picture is due; `None` when none is (idle with
    /// `idle_s` 0, or no rate yet).
    due: Option<u64>,
    /// A picture was asked for and the edge has not answered.
    asked: bool,
}

impl PictureSchedule {
    /// No schedule: nothing is due until the hub asks.
    pub(crate) const fn new() -> Self {
        Self {
            rate: None,
            watched_until: 0,
            watching: false,
            last_asked: 0,
            due: None,
            asked: false,
        }
    }

    /// Forget the schedule: a new registration, or the leg went away.
    pub(crate) fn clear(&mut self) {
        *self = Self::new();
    }

    /// The hub's `rate`, heard at `now`. `true` when the edge should take a
    /// picture now (`false` when one is already in flight: it counts as the
    /// picture taken at once).
    pub(crate) fn rate(&mut self, now: u64, rate: PictureRate) -> bool {
        let rate = rate.clamped();
        self.rate = Some(rate);
        self.watched_until = now + u64::from(rate.watched_for_s) * 1000;
        let take = !self.asked;
        self.asked_at(now);
        take
    }

    /// Time passed. `true` when a picture is due at `now` (asked again even
    /// if the last one was never answered).
    pub(crate) fn tick(&mut self, now: u64) -> bool {
        if self.rate.is_none() {
            return false;
        }
        if self.watching && now >= self.watched_until {
            // The watch ended: idle, counted from the last picture.
            self.watching = false;
            self.due = self.idle_ms().map(|idle| self.last_asked + idle);
        }
        match self.due {
            Some(due) if now >= due => {
                self.asked_at(now);
                true
            }
            _ => false,
        }
    }

    /// The edge has the picture it was asked for. `true` when it is wanted
    /// (a schedule exists and a picture was asked for); either way nothing
    /// is in flight after it.
    pub(crate) fn ready(&mut self) -> bool {
        let wanted = self.rate.is_some() && self.asked;
        self.asked = false;
        wanted
    }

    /// When the next picture is due, if one is.
    pub(crate) fn next_due(&self) -> Option<u64> {
        self.due
    }

    /// When the client must next tick for the pictures: the next due time,
    /// and the end of the watch.
    pub(crate) fn next_wake(&self) -> Option<u64> {
        let watch_end = self.watching.then_some(self.watched_until);
        match (self.due, watch_end) {
            (Some(due), Some(end)) => Some(due.min(end)),
            (due, end) => due.or(end),
        }
    }

    /// Whether the board counts as watched at `now`.
    pub(crate) fn is_watched(&self, now: u64) -> bool {
        self.rate.is_some() && now < self.watched_until
    }

    /// A picture is asked for at `now`: the next one is due a cadence
    /// later.
    fn asked_at(&mut self, now: u64) {
        self.asked = true;
        self.last_asked = now;
        self.watching = now < self.watched_until;
        self.due = if self.watching {
            self.rate.map(|rate| now + u64::from(rate.watched_ms))
        } else {
            self.idle_ms().map(|idle| now + idle)
        };
    }

    /// The idle cadence in milliseconds, or `None` for "no idle pictures".
    fn idle_ms(&self) -> Option<u64> {
        let idle_s = self.rate?.idle_s;
        (idle_s > 0).then(|| u64::from(idle_s) * 1000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_is_due_before_a_rate() {
        let mut schedule = PictureSchedule::new();
        assert!(!schedule.tick(1_000_000));
        assert_eq!(schedule.next_due(), None);
        assert_eq!(schedule.next_wake(), None);
        assert!(!schedule.is_watched(0));
        assert!(!schedule.ready(), "nothing was asked for");
    }

    #[test]
    fn idle_is_one_a_minute_from_the_last() {
        let mut schedule = PictureSchedule::new();
        assert!(schedule.rate(1_000, rate(60, 500, 0)));
        assert_eq!(schedule.next_due(), Some(61_000));
        assert!(!schedule.is_watched(1_000));
        assert!(!schedule.tick(60_999));
        assert!(schedule.tick(61_000));
        assert_eq!(schedule.next_due(), Some(121_000));
    }

    #[test]
    fn the_watch_ends_and_idle_counts_from_the_last_picture() {
        let mut schedule = PictureSchedule::new();
        assert!(schedule.rate(0, rate(60, 500, 15)));
        assert!(schedule.is_watched(14_999));
        assert!(!schedule.is_watched(15_000));
        let mut now = 0;
        while let Some(due) = schedule.next_due().filter(|due| *due < 15_000) {
            now = due;
            assert!(schedule.tick(now));
        }
        assert_eq!(now, 14_500);
        assert_eq!(schedule.next_wake(), Some(15_000), "the watch's end");
        assert!(!schedule.tick(15_000));
        assert_eq!(schedule.next_due(), Some(14_500 + 60_000));
        assert_eq!(schedule.next_wake(), Some(74_500));
    }

    #[test]
    fn idle_zero_takes_no_idle_pictures() {
        let mut schedule = PictureSchedule::new();
        assert!(schedule.rate(0, rate(0, 500, 0)), "still one at once");
        assert_eq!(schedule.next_due(), None);
        assert_eq!(schedule.next_wake(), None);
    }

    #[test]
    fn a_rate_while_a_picture_is_in_flight_does_not_ask_twice() {
        let mut schedule = PictureSchedule::new();
        assert!(schedule.rate(0, rate(60, 500, 15)));
        assert!(!schedule.rate(100, rate(60, 500, 15)));
        assert_eq!(schedule.next_due(), Some(600));
        assert!(schedule.ready());
        assert!(!schedule.ready(), "one answer per ask");
    }

    fn rate(idle_s: u16, watched_ms: u16, watched_for_s: u16) -> PictureRate {
        PictureRate {
            idle_s,
            watched_ms,
            watched_for_s,
        }
    }
}
