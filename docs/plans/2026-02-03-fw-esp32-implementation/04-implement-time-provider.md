# Phase 4: Implement TimeProvider

## Scope of phase

Create the TimeProvider implementation using ESP32 timers via embassy-time.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Create time.rs

Implement `Esp32TimeProvider` using `embassy_time::Instant`:

```rust
//! ESP32 TimeProvider implementation
//!
//! Uses embassy-time for millisecond-precision timing.

use embassy_time::{Duration, Instant};
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
        elapsed.as_millis() as u64
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
```

**Note**: embassy-time's `Instant::now()` returns time since boot. We track `start_time` to provide a consistent reference point. Alternatively, we could use `Instant::now()` directly if embassy-time provides absolute time.

### 2. Update main.rs (stub for now)

Add time module (will be used in later phase):

```rust
mod time;
```

## Notes

- embassy-time provides `Instant` and `Duration` types
- `Instant::now()` returns time since boot
- We track `start_time` for consistency, but this may not be necessary
- If embassy-time provides absolute time, we can simplify

## Validate

Run:
```bash
cd lp-fw/fw-esp32
cargo check --features esp32c6
```

Expected: Code compiles without errors.
