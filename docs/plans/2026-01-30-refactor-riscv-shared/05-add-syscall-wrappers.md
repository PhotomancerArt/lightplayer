# Phase 5: Add Syscall Wrappers in lp-riscv-emu-guest

## Scope of phase

Add simple wrapper functions for serial syscalls in `lp-riscv-emu-guest` to make them easier to use.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Update `lp-riscv/lp-riscv-emu-guest/src/syscall.rs`

Add wrapper functions:

```rust
use crate::syscall::{SYSCALL_ARGS, SYSCALL_SERIAL_HAS_DATA, SYSCALL_SERIAL_READ, SYSCALL_SERIAL_WRITE, syscall};

/// Write bytes to serial output buffer
///
/// # Arguments
/// * `data` - Bytes to write
///
/// # Returns
/// * Positive number: bytes written
/// * Negative number: error code
pub fn sys_serial_write(data: &[u8]) -> i32 {
    if data.is_empty() {
        return 0;
    }

    let mut args = [0i32; SYSCALL_ARGS];
    args[0] = data.as_ptr() as i32;
    args[1] = data.len() as i32;
    syscall(SYSCALL_SERIAL_WRITE, &args)
}

/// Read bytes from serial input buffer
///
/// # Arguments
/// * `buf` - Buffer to read into
///
/// # Returns
/// * Positive number: bytes read
/// * Negative number: error code
pub fn sys_serial_read(buf: &mut [u8]) -> i32 {
    if buf.is_empty() {
        return 0;
    }

    let mut args = [0i32; SYSCALL_ARGS];
    args[0] = buf.as_ptr() as i32;
    args[1] = buf.len() as i32;
    syscall(SYSCALL_SERIAL_READ, &args)
}

/// Check if serial input has data available
///
/// # Returns
/// * `true` if data is available
/// * `false` otherwise
pub fn sys_serial_has_data() -> bool {
    let args = [0i32; SYSCALL_ARGS];
    syscall(SYSCALL_SERIAL_HAS_DATA, &args) != 0
}
```

### 2. Update `lp-riscv/lp-riscv-emu-guest/src/lib.rs`

Add re-exports:

```rust
pub use syscall::{
    // ... existing re-exports ...
    sys_serial_write, sys_serial_read, sys_serial_has_data,
};
```

## Validate

Run from workspace root:

```bash
cargo check --package lp-riscv-emu-guest
cargo check --package lp-riscv-emu-guest-test-app
```

Ensure:

- Code compiles without errors
- Wrappers are accessible
- No warnings
- Functions have proper documentation
