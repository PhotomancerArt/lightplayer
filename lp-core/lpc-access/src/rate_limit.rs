//! Per-device login backoff, on caller-supplied time.
//!
//! Keyed on the DEVICE, not the connection: a failed guess costs the same
//! wait whether the guesser stays connected or reconnects. Held in RAM, so
//! a reboot resets it — acceptable, since the threat model is someone
//! cheeky nearby, not a cracker (and a reboot takes longer than the cap).

/// Failures allowed before any wait is imposed.
pub const FREE_ATTEMPTS: u32 = 3;

/// The first imposed wait, in milliseconds; it doubles per further failure.
pub const FIRST_BACKOFF_MS: u64 = 2_000;

/// The longest wait ever imposed, in milliseconds.
pub const MAX_BACKOFF_MS: u64 = 60_000;

/// Consecutive-failure counter with a wall of caller-supplied time behind
/// it. Time is a monotonic millisecond count the caller owns; this type
/// never reads a clock.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RateLimit {
    failures: u32,
    blocked_until_ms: u64,
}

impl RateLimit {
    /// A fresh limiter: no failures, nothing blocked.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Milliseconds until another attempt is allowed; `0` means now.
    #[must_use]
    pub fn retry_after_ms(&self, now_ms: u64) -> u64 {
        self.blocked_until_ms.saturating_sub(now_ms)
    }

    /// Record a failed attempt at `now_ms`, returning the wait it imposes.
    pub fn record_failure(&mut self, now_ms: u64) -> u64 {
        self.failures = self.failures.saturating_add(1);
        let backoff = backoff_after(self.failures);
        self.blocked_until_ms = now_ms.saturating_add(backoff);
        backoff
    }

    /// A successful login clears the slate.
    pub fn record_success(&mut self) {
        *self = Self::default();
    }

    /// Consecutive failures so far.
    #[must_use]
    pub fn failures(&self) -> u32 {
        self.failures
    }
}

/// The wait imposed after the `failures`-th consecutive failure:
/// 0, 0, 0, then 2 s, 4 s, 8 s … capped at [`MAX_BACKOFF_MS`].
fn backoff_after(failures: u32) -> u64 {
    if failures <= FREE_ATTEMPTS {
        return 0;
    }
    let doublings = failures - FREE_ATTEMPTS - 1;
    // 2 s << 5 = 64 s already exceeds the cap; clamp the shift so a long
    // run of failures cannot overflow it.
    let shift = doublings.min(16);
    (FIRST_BACKOFF_MS << shift).min(MAX_BACKOFF_MS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_free_attempts_then_doubling_to_the_cap() {
        let mut limit = RateLimit::new();
        let waits: [u64; 10] = core::array::from_fn(|_| limit.record_failure(0));
        assert_eq!(
            waits,
            [0, 0, 0, 2_000, 4_000, 8_000, 16_000, 32_000, 60_000, 60_000]
        );
    }

    #[test]
    fn the_wait_runs_on_the_callers_clock() {
        let mut limit = RateLimit::new();
        for _ in 0..FREE_ATTEMPTS {
            limit.record_failure(1_000);
        }
        assert_eq!(limit.retry_after_ms(1_000), 0);

        limit.record_failure(1_000);
        assert_eq!(limit.retry_after_ms(1_000), 2_000);
        assert_eq!(limit.retry_after_ms(2_500), 500);
        assert_eq!(limit.retry_after_ms(3_000), 0);
        assert_eq!(limit.retry_after_ms(99_000), 0);
    }

    #[test]
    fn success_clears_the_slate() {
        let mut limit = RateLimit::new();
        for _ in 0..6 {
            limit.record_failure(0);
        }
        limit.record_success();
        assert_eq!(limit.failures(), 0);
        assert_eq!(limit.retry_after_ms(0), 0);
        assert_eq!(limit.record_failure(0), 0);
    }

    #[test]
    fn a_long_run_never_overflows() {
        let mut limit = RateLimit::new();
        for _ in 0..10_000 {
            limit.record_failure(u64::MAX - 1);
        }
        assert_eq!(limit.retry_after_ms(0), u64::MAX);
    }
}
