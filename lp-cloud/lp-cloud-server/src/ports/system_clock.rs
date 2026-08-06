//! Wall time, from the operating system.

use std::time::{SystemTime, UNIX_EPOCH};

use lp_cloud_domain::Clock;

/// The real clock: f64 epoch seconds, matching `lpc-history`'s convention.
///
/// A machine whose clock is set before 1970 reads as 0 rather than
/// panicking — a nonsense timestamp is a cosmetic problem, and a service
/// that refuses to boot because NTP has not landed yet is not.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> f64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|since| since.as_secs_f64())
            .unwrap_or(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sanity, not precision: the clock reads a plausible present, so a
    /// session TTL means what it says.
    #[test]
    fn reads_a_time_after_2020() {
        assert!(SystemClock.now() > 1_577_836_800.0);
    }
}
