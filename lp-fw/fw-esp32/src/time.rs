//! ESP32 TimeProvider implementation
//!
//! Uses embassy-time for millisecond-precision timing.

use embassy_time::Instant;
use lp_shared::time::TimeProvider;

/// ESP32 TimeProvider implementation using embassy-time
pub struct Esp32TimeProvider {
    /// Start time (when provider was created)
    start_time: Instant,
}

impl Esp32TimeProvider {
    /// Create a new ESP32 TimeProvider
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
        }
    }
}

impl TimeProvider for Esp32TimeProvider {
    fn now_ms(&self) -> u64 {
        // Get elapsed time since start
        let elapsed = Instant::now().saturating_duration_since(self.start_time);
        elapsed.as_millis()
    }

    fn elapsed_ms(&self, start_ms: u64) -> u64 {
        let current_ms = self.now_ms();
        current_ms.saturating_sub(start_ms)
    }
}

impl Default for Esp32TimeProvider {
    fn default() -> Self {
        Self::new()
    }
}
