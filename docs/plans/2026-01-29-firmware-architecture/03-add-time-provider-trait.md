# Phase 3: Add TimeProvider trait to lp-shared

## Scope of phase

Add the `TimeProvider` trait to `lp-shared` crate. This provides a generic abstraction for getting time that can be used by both firmware and potentially other contexts.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Create time module structure

Create `lp-app/crates/lp-shared/src/time/` directory with:

- `mod.rs` - Module entry point
- `provider.rs` - TimeProvider trait

### 2. Update lp-shared/src/lib.rs

Add time module to exports:

```rust
// ... existing code ...

pub mod time;

// ... existing code ...
```

### 3. Create time/mod.rs

```rust
pub mod provider;

pub use provider::TimeProvider;
```

### 4. Create time/provider.rs

Define the `TimeProvider` trait:

```rust
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
```

## Tests

Add basic unit tests:

```rust
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
```

## Validate

Run from `lp-app/` directory:

```bash
cd lp-app
cargo check --package lp-shared
cargo test --package lp-shared
```

Ensure:

- Trait compiles with `no_std`
- All tests pass
- No warnings
- Trait is exported from `lp-shared`
