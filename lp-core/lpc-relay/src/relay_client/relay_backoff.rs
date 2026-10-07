//! How long the board waits before dialling again.
//!
//! - After a failure: 1 s, doubling to 60 s, each wait jittered ±50 %, so
//!   a thousand boards that lost the same server do not come back in step.
//! - After the hub closed with "going away" (a deploy): 2–12 s, jittered,
//!   and the doubling starts over. The new server is up within seconds, the
//!   reconnect storm spreads over the window, and with the registration's
//!   own round trips every board is back within 15 s (the plan's AC5).
//! - A registration starts the doubling over.
//!
//! The randomness is the caller's: a `u32` per wait, from the board's RNG.

/// The first wait after a failure.
pub const FIRST_BACKOFF_MS: u64 = 1_000;
/// The longest wait after failures.
pub const MAX_BACKOFF_MS: u64 = 60_000;
/// The shortest wait after "going away".
pub const GOING_AWAY_MIN_MS: u64 = 2_000;
/// The longest wait after "going away".
pub const GOING_AWAY_MAX_MS: u64 = 12_000;

/// See the module doc.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelayBackoff {
    base_ms: u64,
}

impl RelayBackoff {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            base_ms: FIRST_BACKOFF_MS,
        }
    }

    /// The wait after a failure, in `[base/2, 3·base/2]`; the next base
    /// doubles, up to [`MAX_BACKOFF_MS`].
    pub fn after_failure(&mut self, random: u32) -> u64 {
        let base = self.base_ms;
        self.base_ms = (base * 2).min(MAX_BACKOFF_MS);
        base / 2 + u64::from(random) % (base + 1)
    }

    /// The wait after "going away", in
    /// `[GOING_AWAY_MIN_MS, GOING_AWAY_MAX_MS]`; the doubling starts over.
    pub fn after_going_away(&mut self, random: u32) -> u64 {
        self.reset();
        GOING_AWAY_MIN_MS + u64::from(random) % (GOING_AWAY_MAX_MS - GOING_AWAY_MIN_MS + 1)
    }

    /// A registration: the next failure waits [`FIRST_BACKOFF_MS`] again.
    pub fn reset(&mut self) {
        self.base_ms = FIRST_BACKOFF_MS;
    }
}

impl Default for RelayBackoff {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failures_double_to_a_minute_within_half_either_side() {
        let mut backoff = RelayBackoff::new();
        let mut base = FIRST_BACKOFF_MS;
        for _ in 0..10 {
            let low = RelayBackoff { base_ms: base }.after_failure(0);
            let high = RelayBackoff { base_ms: base }.after_failure(u32::MAX);
            assert!(low >= base / 2 && high <= base + base / 2, "base {base}");
            let wait = backoff.after_failure(12_345);
            assert!(wait >= base / 2 && wait <= base + base / 2);
            base = (base * 2).min(MAX_BACKOFF_MS);
        }
        assert_eq!(backoff.base_ms, MAX_BACKOFF_MS);
        assert_eq!(
            RelayBackoff { base_ms: 1000 }.after_failure(1000),
            1500,
            "the exact upper jitter is 1.5×"
        );
        assert_eq!(RelayBackoff { base_ms: 1000 }.after_failure(1001), 500);
    }

    #[test]
    fn going_away_waits_two_to_twelve_seconds_and_starts_over() {
        let mut backoff = RelayBackoff::new();
        for _ in 0..5 {
            backoff.after_failure(0);
        }
        for random in [0, 1, 10_000, 10_001, u32::MAX] {
            let wait = backoff.after_going_away(random);
            assert!(
                (GOING_AWAY_MIN_MS..=GOING_AWAY_MAX_MS).contains(&wait),
                "{wait}"
            );
        }
        assert_eq!(backoff.after_going_away(10_000), GOING_AWAY_MAX_MS);
        assert_eq!(backoff, RelayBackoff::new());
    }
}
