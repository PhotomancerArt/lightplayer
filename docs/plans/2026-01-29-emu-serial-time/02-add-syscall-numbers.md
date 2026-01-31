# Phase 2: Add syscall numbers to lp-riscv-emu-guest

## Scope of phase

Add syscall number constants to `lp-riscv-emu-guest` for the new syscalls: YIELD, SERIAL_WRITE,
SERIAL_READ, SERIAL_HAS_DATA, and TIME_MS.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Update `lp-riscv/lp-riscv-emu-guest/src/syscall.rs`

Add new syscall number constants after the existing ones:

```rust
/// Syscall number for panic
pub(crate) const SYSCALL_PANIC: i32 = 1;

/// Syscall number for write (always prints)
pub(crate) const SYSCALL_WRITE: i32 = 2;

/// Syscall number for debug (only prints if DEBUG=1)
pub(crate) const SYSCALL_DEBUG: i32 = 3;

/// Syscall number for yield (yield control back to host)
pub(crate) const SYSCALL_YIELD: i32 = 4;

/// Syscall number for serial write (write bytes to serial output buffer)
pub(crate) const SYSCALL_SERIAL_WRITE: i32 = 5;

/// Syscall number for serial read (read bytes from serial input buffer)
pub(crate) const SYSCALL_SERIAL_READ: i32 = 6;

/// Syscall number for serial has_data (check if serial input has data)
pub(crate) const SYSCALL_SERIAL_HAS_DATA: i32 = 7;

/// Syscall number for time_ms (get elapsed milliseconds since emulator start)
pub(crate) const SYSCALL_TIME_MS: i32 = 8;
```

Note: Keep existing `SYSCALL_WRITE` and `SYSCALL_DEBUG` constants if they exist, or add them if
missing.

## Validate

Run from workspace root:

```bash
cargo check --package lp-riscv-emu-guest
```

Ensure:

- Code compiles without errors
- Constants are properly documented
- No warnings
