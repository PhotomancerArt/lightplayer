# Phase 2: Move Error Code Constants to lp-riscv-emu-shared

## Scope of phase

Define serial error code constants in `lp-riscv-emu-shared` so both host and guest can use
consistent
error codes.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Update `lp-riscv/lp-riscv-emu-shared/src/guest_serial.rs`

Replace the sketched struct with error constants:

```rust
//! Serial error code constants shared between host and guest

/// Serial error: Invalid pointer (guest provided invalid memory address)
pub const SERIAL_ERROR_INVALID_POINTER: i32 = -1;

/// Serial error: Buffer full (128KB limit exceeded)
pub const SERIAL_ERROR_BUFFER_FULL: i32 = -2;

/// Serial error: Buffer not allocated (lazy allocation not yet done)
pub const SERIAL_ERROR_BUFFER_NOT_ALLOCATED: i32 = -3;
```

### 2. Update `lp-riscv/lp-riscv-emu-shared/src/lib.rs`

Add re-export:

```rust
mod guest_serial;

pub use guest_serial::{
    SERIAL_ERROR_BUFFER_FULL, SERIAL_ERROR_BUFFER_NOT_ALLOCATED,
    SERIAL_ERROR_INVALID_POINTER,
};
```

### 3. Document error code usage

Add documentation explaining:

- Error codes are negative numbers (common syscall convention)
- Host returns these codes from `guest_write()` / `guest_read()`
- Guest can check return values against these constants

## Validate

Run from workspace root:

```bash
cargo check --package lp-riscv-emu-shared
```

Ensure:

- Code compiles without errors
- Constants are accessible
- Constants are properly documented
- No warnings
