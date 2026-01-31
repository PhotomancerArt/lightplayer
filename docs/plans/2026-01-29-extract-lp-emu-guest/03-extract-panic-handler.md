# Phase 3: Extract Panic Handler

## Scope of Phase

Extract the panic handler implementation from `lp-builtins-app/src/main.rs` into
`lp-riscv-emu-guest/src/panic.rs`.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Extract Panic Handler Code

Read `lp-builtins-app/src/main.rs` and extract:

1. The `panic_syscall` function (lines ~54-72)
2. The `ebreak` function (lines ~75-78)
3. The `#[panic_handler]` function (lines ~80-138)

### 2. Create panic.rs

Create `lp-riscv-emu-guest/src/panic.rs`:

```rust
use core::{
    arch::asm,
    fmt::Write,
    ptr::null,
};

use crate::syscall::{syscall, SYSCALL_ARGS, SYSCALL_PANIC};

/// Exit the interpreter
#[inline(always)]
pub(crate) fn ebreak() -> ! {
    unsafe { asm!("ebreak", options(nostack, noreturn)) }
}

/// Report a panic to the host VM
///
/// This should be called from the panic handler before ebreak.
/// args[0] = panic message pointer (as i32)
/// args[1] = panic message length
/// args[2] = file pointer (as i32, 0 if unavailable)
/// args[3] = file length
/// args[4] = line number (0 if unavailable)
fn panic_syscall(
    msg_ptr: *const u8,
    msg_len: usize,
    file_ptr: *const u8,
    file_len: usize,
    line: u32,
) -> ! {
    let args = [
        msg_ptr as i32,
        msg_len as i32,
        file_ptr as i32,
        file_len as i32,
        line as i32,
        0,
        0,
    ];
    let _ = syscall(SYSCALL_PANIC, &args);
    ebreak()
}

/// Panic handler
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    // Create a buffer for the panic message
    let mut panic_msg_buf = [0u8; 256];
    let mut cursor = 0;

    // Format the panic message into our buffer
    struct BufWriter<'a> {
        buf: &'a mut [u8],
        cursor: &'a mut usize,
    }

    impl<'a> Write for BufWriter<'a> {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            let bytes = s.as_bytes();
            let remaining = self.buf.len() - *self.cursor;
            let to_write = bytes.len().min(remaining);
            if to_write > 0 {
                self.buf[*self.cursor..*self.cursor + to_write].copy_from_slice(&bytes[..to_write]);
                *self.cursor += to_write;
            }
            Ok(())
        }
    }

    let mut writer = BufWriter {
        buf: &mut panic_msg_buf,
        cursor: &mut cursor,
    };

    // Try to format the full panic info
    let _ = write!(writer, "{}", info.message());

    // If message is empty, use default message
    if cursor == 0 {
        let default_msg = b"panic occurred (no message)";
        let to_copy = default_msg.len().min(panic_msg_buf.len());
        panic_msg_buf[..to_copy].copy_from_slice(&default_msg[..to_copy]);
        cursor = to_copy;
    }

    // Try to extract location info
    // Note: In no_std, location() may return None if location tracking is disabled
    // or if the panic was created without location info
    // The file name from Location is a string literal in the binary, so the pointer is valid
    let (file_ptr, file_len, line) = if let Some(loc) = info.location() {
        let file = loc.file();
        // file() returns &str which points to a string literal in the binary
        // This pointer is valid for the lifetime of the program
        let file_bytes = file.as_bytes();
        (file_bytes.as_ptr(), file_bytes.len(), loc.line())
    } else {
        (null(), 0, 0)
    };

    // Report panic to host with the message
    panic_syscall(panic_msg_buf.as_ptr(), cursor, file_ptr, file_len, line);
}
```

**Note**: This uses `crate::syscall` which we'll create in the next phase. For now, we'll need to
create a stub.

### 3. Create syscall.rs Stub

Create `lp-riscv-emu-guest/src/syscall.rs` with minimal content for now:

```rust
/// Syscall number for panic
pub(crate) const SYSCALL_PANIC: i32 = 1;

/// Number of syscall arguments
pub(crate) const SYSCALL_ARGS: usize = 7;

/// System call implementation
pub(crate) fn syscall(nr: i32, args: &[i32; SYSCALL_ARGS]) -> i32 {
    let error: i32;
    let value: i32;
    unsafe {
        core::arch::asm!(
            "ecall",
            in("x17") nr,
            inlateout("x10") args[0] => error,
            inlateout("x11") args[1] => value,
            in("x12") args[2],
            in("x13") args[3],
            in("x14") args[4],
            in("x15") args[5],
            in("x16") args[6],
        );
    }
    if error != 0 { error } else { value }
}
```

### 4. Update lib.rs

Update `lp-riscv-emu-guest/src/lib.rs`:

```rust
#![no_std]

pub mod entry;
mod panic;  // Panic handler is automatically registered via #[panic_handler]
mod syscall;
```

## Validate

Run from workspace root:

```bash
cargo check --package lp-riscv-emu-guest --target riscv32imac-unknown-none-elf
```

This should compile successfully. The panic handler will be automatically registered via the
`#[panic_handler]` attribute.
