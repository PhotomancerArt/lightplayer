//! Time in this crate: caller-supplied epoch **milliseconds**, never read
//! from a clock.
//!
//! The repo's sans-IO convention is "timestamps are caller-supplied" (see
//! `docs/adr/2026-07-06-sans-io-core.md`); core crates that measure wall
//! clock generally take f64 epoch seconds. This crate uses integer epoch
//! millis instead, because every value it stores is compared, subtracted,
//! and asserted on in replay fixtures — integers make fixtures exact and
//! journals byte-reproducible, which f64 does not.
//!
//! Waiting is never a sleep: the model emits [`crate::Command::StartTimer`]
//! and learns that time passed from [`crate::Event::TimerFired`].

use serde::{Deserialize, Serialize};

use crate::journal::Scope;

/// An instant, as epoch milliseconds supplied by the caller.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct Millis(pub u64);

impl Millis {
    /// This instant advanced by a duration in milliseconds.
    pub fn plus_ms(self, ms: u64) -> Self {
        Self(self.0.saturating_add(ms))
    }

    /// Milliseconds elapsed from `earlier` to `self`, saturating at zero so a
    /// caller that replays out-of-order stamps gets 0 instead of a wrap.
    pub fn since(self, earlier: Millis) -> u64 {
        self.0.saturating_sub(earlier.0)
    }
}

/// A timer the model asked for and expects back as
/// [`Event::TimerFired`](crate::Event::TimerFired).
///
/// **One outstanding timer per scope.** Each device (and each pending link)
/// keeps at most one armed tick, scheduled for its nearest deadline; the
/// handler for a fired tick re-evaluates every deadline it owns (freshness,
/// activity deadline, cancel grace, hello re-ask) and re-arms once. The
/// `seq` is a generation: a fire whose `seq` is not the scope's currently
/// armed one is ignored, which is why the vocabulary needs no CancelTimer.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct TimerId {
    pub scope: Scope,
    pub seq: u64,
}

/// Hands out monotonically increasing timer generations.
///
/// Routing bookkeeping, not model state: it is never read to decide what a
/// device *is*, only to discard superseded timer fires.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct TimerAllocator {
    next_seq: u64,
}

impl TimerAllocator {
    pub fn next(&mut self, scope: Scope) -> TimerId {
        self.next_seq += 1;
        TimerId {
            scope,
            seq: self.next_seq,
        }
    }
}

/// A duration rendered with the unit that reads naturally at its magnitude
/// ("just now" / "12 s" / "3 min" / "2 hr"), per the LightPlayer
/// unit-awareness principle. Used by the projection so staleness renders
/// honestly instead of as a stuck spinner.
pub fn describe_age_ms(ms: u64) -> String {
    if ms < 1_500 {
        return "just now".to_string();
    }
    let seconds = ms / 1_000;
    if seconds < 90 {
        return format!("{seconds} s ago");
    }
    let minutes = seconds / 60;
    if minutes < 90 {
        return format!("{minutes} min ago");
    }
    let hours = minutes / 60;
    if hours < 48 {
        return format!("{hours} hr ago");
    }
    format!("{} days ago", hours / 24)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn since_saturates_instead_of_wrapping() {
        assert_eq!(Millis(5_000).since(Millis(1_500)), 3_500);
        assert_eq!(Millis(1_000).since(Millis(5_000)), 0);
    }

    #[test]
    fn timer_generations_are_monotonic_per_allocator() {
        let mut timers = TimerAllocator::default();
        let first = timers.next(Scope::Roster);
        let second = timers.next(Scope::Roster);

        assert_eq!(first.seq, 1);
        assert_eq!(second.seq, 2);
    }

    #[test]
    fn ages_pick_a_unit_that_reads_naturally() {
        assert_eq!(describe_age_ms(200), "just now");
        assert_eq!(describe_age_ms(12_000), "12 s ago");
        assert_eq!(describe_age_ms(3 * 60_000), "3 min ago");
        assert_eq!(describe_age_ms(2 * 3_600_000), "2 hr ago");
        assert_eq!(describe_age_ms(72 * 3_600_000), "3 days ago");
    }
}
