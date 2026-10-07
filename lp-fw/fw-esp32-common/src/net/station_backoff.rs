//! When the station searches again after it found nothing to join.

/// The search schedule (plan A6): try at once, then again after each of
/// [`Self::QUICK_RETRIES_MS`], then every [`Self::EARLY_EVERY_MS`] until the
/// search is [`Self::EARLY_FOR_MS`] old, then every [`Self::LATE_EVERY_MS`].
/// A settings change or a lost link starts it over ([`Self::restart`]), so a
/// board that just lost its router looks hard for a minute and then settles,
/// and a board that was just told about a network looks at once.
///
/// The quick retries are for the first scan after boot, which does not
/// always hear an access point that is there: on silicon 4 boots of 19 missed
/// it, and the next look 10 s later made the join take 11–23 s (the FC6
/// re-check, 2026-10-06). A second and third look a second or two later
/// find it.
///
/// Time is the caller's (`now_ms`, a monotonic millisecond count): nothing
/// here reads a clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StationBackoff {
    /// When the current search began; `None` before the first.
    since_ms: Option<u64>,
    /// Looks that found nothing since the search began.
    misses: u8,
}

impl StationBackoff {
    /// The waits after the first looks that find nothing, before the
    /// schedule settles to [`Self::EARLY_EVERY_MS`].
    pub const QUICK_RETRIES_MS: [u64; 2] = [1_000, 2_000];
    /// How often to look while the search is young.
    pub const EARLY_EVERY_MS: u64 = 10_000;
    /// How long the search counts as young.
    pub const EARLY_FOR_MS: u64 = 60_000;
    /// How often to look after that.
    pub const LATE_EVERY_MS: u64 = 60_000;

    /// A schedule that has not started.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            since_ms: None,
            misses: 0,
        }
    }

    /// Start the search over at `now_ms`: the next look is now.
    pub fn restart(&mut self, now_ms: u64) {
        self.since_ms = Some(now_ms);
        self.misses = 0;
    }

    /// When to look next, after a look at `now_ms` found nothing to join.
    pub fn next_after(&mut self, now_ms: u64) -> u64 {
        let since = *self.since_ms.get_or_insert(now_ms);
        let quick = Self::QUICK_RETRIES_MS
            .get(usize::from(self.misses))
            .copied();
        self.misses = self.misses.saturating_add(1);
        let every = match quick {
            Some(wait) => wait,
            None if now_ms.saturating_sub(since) < Self::EARLY_FOR_MS => Self::EARLY_EVERY_MS,
            None => Self::LATE_EVERY_MS,
        };
        now_ms.saturating_add(every)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_second_two_then_every_ten_for_a_minute_then_every_minute() {
        let mut backoff = StationBackoff::new();
        backoff.restart(1_000);
        let mut now = 1_000;
        let mut gaps = alloc::vec::Vec::new();
        for _ in 0..11 {
            let next = backoff.next_after(now);
            gaps.push(next - now);
            now = next;
        }
        assert_eq!(
            gaps,
            [
                1_000, 2_000, 10_000, 10_000, 10_000, 10_000, 10_000, 10_000, 60_000, 60_000,
                60_000
            ]
        );
    }

    #[test]
    fn a_restart_makes_it_young_again() {
        let mut backoff = StationBackoff::new();
        backoff.restart(0);
        backoff.next_after(0);
        backoff.next_after(1_000);
        assert_eq!(backoff.next_after(120_000), 180_000);
        backoff.restart(200_000);
        assert_eq!(backoff.next_after(200_000), 201_000, "quick again");
    }

    #[test]
    fn an_unstarted_schedule_starts_at_its_first_look() {
        let mut backoff = StationBackoff::new();
        assert_eq!(backoff.next_after(5_000), 6_000);
    }
}
