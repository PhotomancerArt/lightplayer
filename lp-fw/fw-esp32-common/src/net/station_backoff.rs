//! When the station searches again after it found nothing to join.

/// The search schedule (plan A6): try at once, then every
/// [`Self::EARLY_EVERY_MS`] for [`Self::EARLY_FOR_MS`], then every
/// [`Self::LATE_EVERY_MS`]. A settings change or a lost link starts it over
/// ([`Self::restart`]), so a board that just lost its router looks hard for a
/// minute and then settles, and a board that was just told about a network
/// looks at once.
///
/// Time is the caller's (`now_ms`, a monotonic millisecond count): nothing
/// here reads a clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StationBackoff {
    /// When the current search began; `None` before the first.
    since_ms: Option<u64>,
}

impl StationBackoff {
    /// How often to look while the search is young.
    pub const EARLY_EVERY_MS: u64 = 10_000;
    /// How long the search counts as young.
    pub const EARLY_FOR_MS: u64 = 60_000;
    /// How often to look after that.
    pub const LATE_EVERY_MS: u64 = 60_000;

    /// A schedule that has not started.
    #[must_use]
    pub const fn new() -> Self {
        Self { since_ms: None }
    }

    /// Start the search over at `now_ms`: the next look is now.
    pub fn restart(&mut self, now_ms: u64) {
        self.since_ms = Some(now_ms);
    }

    /// When to look next, after a look at `now_ms` found nothing to join.
    pub fn next_after(&mut self, now_ms: u64) -> u64 {
        let since = *self.since_ms.get_or_insert(now_ms);
        let every = if now_ms.saturating_sub(since) < Self::EARLY_FOR_MS {
            Self::EARLY_EVERY_MS
        } else {
            Self::LATE_EVERY_MS
        };
        now_ms.saturating_add(every)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_ten_seconds_for_a_minute_then_every_minute() {
        let mut backoff = StationBackoff::new();
        backoff.restart(1_000);
        let mut now = 1_000;
        let mut gaps = alloc::vec::Vec::new();
        for _ in 0..9 {
            let next = backoff.next_after(now);
            gaps.push(next - now);
            now = next;
        }
        assert_eq!(
            gaps,
            [
                10_000, 10_000, 10_000, 10_000, 10_000, 10_000, 60_000, 60_000, 60_000
            ]
        );
    }

    #[test]
    fn a_restart_makes_it_young_again() {
        let mut backoff = StationBackoff::new();
        backoff.restart(0);
        assert_eq!(backoff.next_after(120_000), 180_000);
        backoff.restart(200_000);
        assert_eq!(backoff.next_after(200_000), 210_000);
    }

    #[test]
    fn an_unstarted_schedule_starts_at_its_first_look() {
        let mut backoff = StationBackoff::new();
        assert_eq!(backoff.next_after(5_000), 15_000);
    }
}
