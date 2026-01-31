//! Time provider trait for firmware and other contexts
//!
//! Provides a generic abstraction for getting time that works in both
//! `no_std` firmware environments and standard library contexts.

/// Trait for providing time information
///
/// This trait abstracts over different time sources (hardware timers,
/// system time, simulated time, etc.) to provide a consistent interface
/// for getting the current time.
pub trait TimeProvider {
    /// Get the current time in milliseconds since boot/start
    ///
    /// The exact epoch is implementation-defined (e.g., system boot,
    /// emulator start, etc.). The important thing is that time advances
    /// monotonically.
    ///
    /// # Returns
    /// Current time in milliseconds since the epoch
    fn now_ms(&self) -> u64;

    /// Calculate elapsed time in milliseconds
    ///
    /// # Arguments
    /// * `start` - Start time (from a previous `now_ms()` call)
    ///
    /// # Returns
    /// Elapsed time in milliseconds
    fn elapsed_ms(&self, start: u64) -> u64 {
        let now = self.now_ms();
        if now >= start {
            now - start
        } else {
            // Handle wraparound (unlikely with u64, but be safe)
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Mock TimeProvider for testing
    struct MockTimeProvider {
        current_time: u64,
    }

    impl MockTimeProvider {
        fn new() -> Self {
            Self { current_time: 0 }
        }

        fn advance(&mut self, ms: u64) {
            self.current_time += ms;
        }
    }

    impl TimeProvider for MockTimeProvider {
        fn now_ms(&self) -> u64 {
            self.current_time
        }
    }

    #[test]
    fn test_now_ms() {
        let provider = MockTimeProvider::new();
        assert_eq!(provider.now_ms(), 0);
    }

    #[test]
    fn test_elapsed_ms() {
        let mut provider = MockTimeProvider::new();
        let start = provider.now_ms();
        provider.advance(100);
        assert_eq!(provider.elapsed_ms(start), 100);
    }

    #[test]
    fn test_elapsed_ms_wraparound() {
        let provider = MockTimeProvider::new();
        // Test wraparound handling
        assert_eq!(provider.elapsed_ms(u64::MAX), 0);
    }
}
