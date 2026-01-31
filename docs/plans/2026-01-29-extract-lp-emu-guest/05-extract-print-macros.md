# Phase 5: Extract Print Macros

## Scope of Phase

Extract the print macros and writer implementation from `lp-builtins-app/src/print.rs` into
`lp-riscv-emu-guest/src/print.rs`.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Extract Print Code

Read `lp-builtins-app/src/print.rs` and extract the entire file content.

### 2. Create print.rs

Create `lp-riscv-emu-guest/src/print.rs`:

```rust
use core::fmt::{self, Write};

use crate::syscall::{syscall, SYSCALL_ARGS};

/// Syscall number for write
const SYSCALL_WRITE: i32 = 2;

/// Writer that sends output to the host via syscall
///
/// Syscall 2: Write string to host
/// - args[0] = pointer to string (as i32)
/// - args[1] = length of string
pub struct BuiltinsWriter;

impl Write for BuiltinsWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        // Skip empty strings (formatting artifacts)
        if s.is_empty() {
            return Ok(());
        }

        // Syscall 2: Write string to host
        // args[0] = pointer to string (as i32)
        // args[1] = length of string
        let ptr = s.as_ptr() as usize as i32;
        let len = s.len() as i32;

        let mut args = [0i32; SYSCALL_ARGS];
        args[0] = ptr;
        args[1] = len;
        let _ = syscall(SYSCALL_WRITE, &args);
        Ok(())
    }
}

/// Global writer instance
static mut WRITER: BuiltinsWriter = BuiltinsWriter;

/// Print function used by print!/println! macros
///
/// This function is called by the print! and println! macros
/// when used in a no_std environment.
#[unsafe(no_mangle)]
#[allow(static_mut_refs)] // Safe: WRITER is only accessed from this single-threaded function
pub fn _print(args: fmt::Arguments) {
    unsafe {
        // Use addr_of_mut! to safely get a pointer to the mutable static
        // This avoids creating a mutable reference directly, which is unsafe in Rust 2024
        match (*core::ptr::addr_of_mut!(WRITER)).write_fmt(args) {
            Ok(()) => {}
            Err(_) => {
                // If formatting fails, we can't do much in no_std
                // But at least we tried
            }
        }
    }
}

/// Print macro for no_std environments
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {
        $crate::_print(core::format_args!($($arg)*));
    };
}

/// Println macro for no_std environments
#[macro_export]
macro_rules! println {
    () => {
        $crate::print!("\n");
    };
    ($($arg:tt)*) => {
        $crate::print!($($arg)*);
        $crate::print!("\n");
    };
}
```

**Note**: The macros use `#[macro_export]` so they'll be available at the crate root. We'll
re-export them in `lib.rs`.

### 3. Update lib.rs

Update `lp-riscv-emu-guest/src/lib.rs`:

```rust
#![no_std]

pub mod entry;
pub mod host;
pub mod print;  // Public module for print macros
mod panic;
mod syscall;

// Re-export macros
pub use print::{print, println};
```

**Note**: We can't re-export `#[macro_export]` macros directly, but they'll be available as
`lp_riscv_emu_guest::print!` and `lp_riscv_emu_guest::println!`. Alternatively, we can create
wrapper macros.
Let's check how `lp-builtins-app` uses them.

Actually, looking at the original code, the macros are `#[macro_export]` which means they're
available at the crate root. When we re-export them, they'll be available as
`lp_riscv_emu_guest::print!`
etc. But for convenience, we might want to also provide `host_debug!` and `host_println!` macros
that use the host functions.

Let's check if `lp-builtins` provides these macros... Actually, `host_debug!` is used in
`lp-builtins-app/src/main.rs` but it's not defined there. It must come from `lp-builtins`. We don't
need to provide it here.

For now, just re-export the print macros. Applications can use `lp_riscv_emu_guest::print!` or
import
them.

## Validate

Run from workspace root:

```bash
cargo check --package lp-riscv-emu-guest --target riscv32imac-unknown-none-elf
```

This should compile successfully. The macros will be available at the crate root.
