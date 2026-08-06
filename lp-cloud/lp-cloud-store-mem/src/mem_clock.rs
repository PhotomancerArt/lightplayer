//! A clock a test can move.

use core::cell::Cell;
use lp_cloud_domain::Clock;

/// A clock that stands still until something moves it.
///
/// Time advances through [`advance`](MemClock::advance), which takes `&self`
/// so a test can move the clock while the service still owns it — expiring a
/// session is otherwise unreachable without waiting for real time to pass.
#[derive(Debug, Clone)]
pub struct MemClock {
    now: Cell<f64>,
}

impl MemClock {
    /// A clock reading `start` (f64 epoch seconds).
    pub fn new(start: f64) -> Self {
        Self {
            now: Cell::new(start),
        }
    }

    /// Move the clock by `seconds` (negative moves it back).
    pub fn advance(&self, seconds: f64) {
        self.now.set(self.now.get() + seconds);
    }

    /// Set the clock to an exact instant.
    pub fn set(&self, epoch_seconds: f64) {
        self.now.set(epoch_seconds);
    }
}

impl Clock for MemClock {
    fn now(&self) -> f64 {
        self.now.get()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advances_and_rewinds() {
        let clock = MemClock::new(100.0);
        assert_eq!(clock.now(), 100.0);
        clock.advance(5.0);
        assert_eq!(clock.now(), 105.0);
        clock.advance(-105.0);
        assert_eq!(clock.now(), 0.0);
        clock.set(42.0);
        assert_eq!(clock.now(), 42.0);
    }
}
