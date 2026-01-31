# Phase 1: Add Time Mode to Emulator

## Scope of phase

Add time mode support to the emulator to allow deterministic time control for testing. This includes:
- Creating a `TimeMode` enum (RealTime vs Simulated)
- Adding time mode field to `Riscv32Emulator`
- Updating time syscall handler to use time mode
- Adding methods to advance simulated time

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Create time module (`lp-riscv/lp-riscv-emu/src/time.rs`)

```rust
//! Time mode for emulator time control

/// Time mode for controlling how time advances in the emulator
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeMode {
    /// Use real wall-clock time (default behavior)
    RealTime,
    /// Use simulated time that can be advanced manually
    Simulated(u32), // Current simulated time in milliseconds
}

impl Default for TimeMode {
    fn default() -> Self {
        TimeMode::RealTime
    }
}
```

### 2. Update `Riscv32Emulator` state (`lp-riscv/lp-riscv-emu/src/emu/emulator/state.rs`)

Add time mode field:

```rust
use crate::time::TimeMode;

pub struct Riscv32Emulator {
    // ... existing fields ...
    /// Time mode for controlling time advancement
    pub(super) time_mode: TimeMode,
}

impl Riscv32Emulator {
    pub fn with_traps(code: Vec<u8>, ram: Vec<u8>, traps: &[(u32, TrapCode)]) -> Self {
        Self {
            // ... existing fields ...
            time_mode: TimeMode::RealTime,
        }
    }

    /// Set the time mode
    pub fn with_time_mode(mut self, mode: TimeMode) -> Self {
        self.time_mode = mode;
        self
    }

    /// Set the time mode (mutating)
    pub fn set_time_mode(&mut self, mode: TimeMode) {
        self.time_mode = mode;
    }

    /// Advance simulated time (only works in Simulated mode)
    ///
    /// # Arguments
    /// * `ms` - Milliseconds to advance
    pub fn advance_time(&mut self, ms: u32) {
        if let TimeMode::Simulated(ref mut current) = self.time_mode {
            *current = current.saturating_add(ms);
        }
        // Ignore if in RealTime mode
    }

    /// Get elapsed milliseconds based on current time mode
    #[cfg(feature = "std")]
    pub(super) fn elapsed_ms(&self) -> u32 {
        match self.time_mode {
            TimeMode::RealTime => {
                if let Some(start) = self.start_time {
                    start.elapsed().as_millis() as u32
                } else {
                    0
                }
            }
            TimeMode::Simulated(current) => current,
        }
    }

    #[cfg(not(feature = "std"))]
    pub(super) fn elapsed_ms(&self) -> u32 {
        match self.time_mode {
            TimeMode::RealTime => 0,
            TimeMode::Simulated(current) => current,
        }
    }
}
```

### 3. Update time syscall handler (`lp-riscv/lp-riscv-emu/src/emu/emulator/execution.rs`)

The handler already calls `self.elapsed_ms()`, so it should automatically use the time mode. Verify that it works correctly:

```rust
} else if syscall_info.number == lp_riscv_emu_shared::SYSCALL_TIME_MS {
    // SYSCALL_TIME_MS: Get elapsed milliseconds since emulator start
    // Returns: a0 = elapsed milliseconds (u32)
    // Uses time_mode to determine if real-time or simulated
    let elapsed = self.elapsed_ms();
    self.regs[Gpr::A0.num() as usize] = elapsed as i32;
    Ok(StepResult::Continue)
}
```

### 4. Export time module (`lp-riscv/lp-riscv-emu/src/lib.rs`)

```rust
pub mod time;

pub use time::TimeMode;
```

## Tests

Add tests to verify time mode works:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simulated_time_mode() {
        let mut emu = Riscv32Emulator::new(vec![], vec![])
            .with_time_mode(TimeMode::Simulated(0));
        
        assert_eq!(emu.elapsed_ms(), 0);
        
        emu.advance_time(100);
        assert_eq!(emu.elapsed_ms(), 100);
        
        emu.advance_time(50);
        assert_eq!(emu.elapsed_ms(), 150);
    }

    #[test]
    fn test_realtime_mode_ignores_advance() {
        let mut emu = Riscv32Emulator::new(vec![], vec![])
            .with_time_mode(TimeMode::RealTime);
        
        // advance_time should be ignored in RealTime mode
        emu.advance_time(100);
        // Time should still be based on real time (or 0 if not started)
        // We can't assert exact value, but we can verify it doesn't jump to 100
        let initial = emu.elapsed_ms();
        emu.advance_time(100);
        // In RealTime mode, elapsed_ms should not jump by 100 immediately
        // (it might increase slightly due to real time passing, but not by 100)
        let after = emu.elapsed_ms();
        assert!(after < initial + 100, "RealTime mode should ignore advance_time");
    }
}
```

## Validate

Run from `lp-riscv/lp-riscv-emu/` directory:

```bash
cd lp-riscv/lp-riscv-emu
cargo test --lib
cargo check
```

Ensure:
- Time mode enum compiles
- `Riscv32Emulator` has time mode field and methods
- Time syscall handler uses time mode correctly
- Tests pass
- No warnings (except for TODO comments if any)
