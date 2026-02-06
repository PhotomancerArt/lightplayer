# Phase 1: Create LoggingMode Trait and Simplify LogLevel Enum

## Scope of Phase

Create the `LoggingMode` trait system and simplify the `LogLevel` enum by removing `LogLevel::Verbose`. This establishes the foundation for the monomorphic approach.

## Code Organization Reminders

- Place trait definitions and implementations at the top of files
- Keep related functionality grouped together
- Use clear, descriptive names

## Implementation Details

### 1. Create LoggingMode Trait

Add to `lp-riscv/lp-riscv-emu/src/emu/executor/mod.rs` (create new file):

```rust
//! Instruction executor for RISC-V 32-bit instructions.

/// Trait for compile-time logging mode control.
pub trait LoggingMode {
    /// Whether logging is enabled for this mode.
    const ENABLED: bool;
}

/// Logging enabled mode - creates InstLog entries.
pub struct LoggingEnabled;

impl LoggingMode for LoggingEnabled {
    const ENABLED: bool = true;
}

/// Logging disabled mode - zero logging overhead.
pub struct LoggingDisabled;

impl LoggingMode for LoggingDisabled {
    const ENABLED: bool = false;
}
```

### 2. Simplify LogLevel Enum

Update `lp-riscv/lp-riscv-emu/src/emu/logging.rs`:

```rust
/// Logging verbosity level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    /// No logging.
    None,
    /// Only log errors.
    Errors,
    /// Log each instruction execution.
    Instructions,
    // Removed: Verbose (was same as Instructions)
}
```

### 3. Update debug.rs

Update `lp-riscv/lp-riscv-emu/src/emu/emulator/debug.rs`:

```rust
pub fn log_instruction(&mut self, log: InstLog) {
    match self.log_level {
        LogLevel::None => {
            // Should not happen, but handle gracefully
        }
        LogLevel::Errors => {
            // Only log on errors (handled elsewhere)
        }
        LogLevel::Instructions => {
            // Implement rolling buffer: if buffer reaches 100, remove oldest
            if self.log_buffer.len() >= 100 {
                self.log_buffer.remove(0);
            }
            self.log_buffer.push(log);
        }
    }
}
```

### 4. Create executor/mod.rs Structure

Create `lp-riscv/lp-riscv-emu/src/emu/executor/mod.rs`:

```rust
//! Instruction executor for RISC-V 32-bit instructions.

/// Trait for compile-time logging mode control.
pub trait LoggingMode {
    /// Whether logging is enabled for this mode.
    const ENABLED: bool;
}

/// Logging enabled mode - creates InstLog entries.
pub struct LoggingEnabled;

impl LoggingMode for LoggingEnabled {
    const ENABLED: bool = true;
}

/// Logging disabled mode - zero logging overhead.
pub struct LoggingDisabled;

impl LoggingMode for LoggingDisabled {
    const ENABLED: bool = false;
}

// Category modules will be added in later phases
// pub mod arithmetic;
// pub mod immediate;
// pub mod load_store;
// pub mod branch;
// pub mod jump;
// pub mod system;
// pub mod compressed;
// pub mod bitmanip;
```

### 5. Update emu/mod.rs

Update `lp-riscv/lp-riscv-emu/src/emu/mod.rs` to include new executor module:

```rust
// ... existing code ...
pub mod executor;
// ... existing code ...
```

## Tests

No new tests needed for this phase - we're just setting up the infrastructure.

## Validate

Run:
```bash
cd lp-riscv/lp-riscv-emu
cargo check
```

Fix any compilation errors. Ensure:
- `LogLevel::Verbose` is removed everywhere
- `LoggingMode` trait compiles correctly
- All existing code still compiles
