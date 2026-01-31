# Phase 4: Extract Host Communication

## Scope of Phase

Extract the host communication functions (`__host_debug` and `__host_println`) from
`lp-builtins-app/src/host.rs` into `lp-riscv-emu-guest/src/host.rs`.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Extract Host Communication Code

Read `lp-builtins-app/src/host.rs` and extract the entire file content.

### 2. Create host.rs

Create `lp-riscv-emu-guest/src/host.rs`:

```rust
use crate::syscall::{syscall, SYSCALL_ARGS};

/// Syscall number for write (always prints)
const SYSCALL_WRITE: i32 = 2;

/// Syscall number for debug (only prints if DEBUG=1)
const SYSCALL_DEBUG: i32 = 3;

/// Debug function implementation for emulator.
///
/// This function is called by the `host_debug!` macro.
/// Uses a separate syscall so the emulator can check DEBUG=1 env var.
#[unsafe(no_mangle)]
pub extern "C" fn __host_debug(ptr: *const u8, len: usize) {
    let ptr = ptr as usize as i32;
    let len = len as i32;

    let mut args = [0i32; SYSCALL_ARGS];
    args[0] = ptr;
    args[1] = len;
    let _ = syscall(SYSCALL_DEBUG, &args);

    // Add trailing newline
    let newline = "\n";
    let ptr = newline.as_ptr() as usize as i32;
    let len = newline.len() as i32;
    let mut args = [0i32; SYSCALL_ARGS];
    args[0] = ptr;
    args[1] = len;
    let _ = syscall(SYSCALL_DEBUG, &args);
}

/// Println function implementation for emulator.
///
/// This function is called by the `host_println!` macro.
#[unsafe(no_mangle)]
pub extern "C" fn __host_println(ptr: *const u8, len: usize) {
    // Print the message
    let ptr = ptr as usize as i32;
    let len = len as i32;

    let mut args = [0i32; SYSCALL_ARGS];
    args[0] = ptr;
    args[1] = len;
    let _ = syscall(SYSCALL_WRITE, &args);

    // Print newline
    let newline = "\n";
    let ptr = newline.as_ptr() as usize as i32;
    let len = newline.len() as i32;
    let mut args = [0i32; SYSCALL_ARGS];
    args[0] = ptr;
    args[1] = len;
    let _ = syscall(SYSCALL_WRITE, &args);
}
```

### 3. Update lib.rs

Update `lp-riscv-emu-guest/src/lib.rs`:

```rust
#![no_std]

pub mod entry;
pub mod host;  // Public module for host communication
mod panic;
mod syscall;
```

## Validate

Run from workspace root:

```bash
cargo check --package lp-riscv-emu-guest --target riscv32imac-unknown-none-elf
```

This should compile successfully. The `__host_debug` and `__host_println` functions are
`#[no_mangle]` so they'll be accessible from code that links against this crate.
