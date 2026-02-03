# Phase 3: Implement fw-emu Syscall Wrappers

## Scope of phase

Implement the syscall wrappers for serial, time, and output providers in fw-emu using the actual syscall functions from `lp-riscv-emu-guest`.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Update Cargo.toml (`lp-app/apps/fw-emu/Cargo.toml`)

Add `lp-riscv-emu-guest` dependency:

```toml
lp-riscv-emu-guest = { path = "../../../lp-riscv/lp-riscv-emu-guest" }
```

Update `lp-emu-guest` to use package alias if needed, or remove it if `lp-riscv-emu-guest` has everything:

```toml
lp-emu-guest = { path = "../../../lp-glsl/crates/lp-emu-guest", package = "lp-riscv-emu-guest" }
```

### 2. Update serial syscall wrapper (`lp-app/apps/fw-emu/src/serial/syscall.rs`)

```rust
//! Syscall-based SerialIo implementation
//!
//! Uses emulator syscalls for serial I/O communication with the host.

use fw_core::serial::{SerialError, SerialIo};
use lp_riscv_emu_guest::{
    sys_serial_has_data, sys_serial_read, sys_serial_write,
};

/// Syscall-based SerialIo implementation
///
/// Uses emulator syscalls to read/write serial data.
pub struct SyscallSerialIo;

impl SyscallSerialIo {
    /// Create a new syscall-based SerialIo instance
    pub fn new() -> Self {
        Self
    }
}

impl SerialIo for SyscallSerialIo {
    fn write(&mut self, data: &[u8]) -> Result<(), SerialError> {
        let result = sys_serial_write(data);
        if result < 0 {
            Err(SerialError::IoError)
        } else {
            Ok(())
        }
    }

    fn read_available(&mut self, buf: &mut [u8]) -> Result<usize, SerialError> {
        let result = sys_serial_read(buf);
        if result < 0 {
            Err(SerialError::IoError)
        } else {
            Ok(result as usize)
        }
    }

    fn has_data(&self) -> bool {
        sys_serial_has_data()
    }
}
```

### 3. Update time syscall wrapper (`lp-app/apps/fw-emu/src/time/syscall.rs`)

```rust
//! Syscall-based TimeProvider implementation
//!
//! Uses emulator syscalls to get time from the host.

use lp_riscv_emu_guest::{syscall, SYSCALL_ARGS, SYSCALL_TIME_MS};
use lp_shared::time::TimeProvider;

/// Syscall-based TimeProvider implementation
///
/// Uses emulator syscalls to get current time from the host.
pub struct SyscallTimeProvider;

impl SyscallTimeProvider {
    /// Create a new syscall-based TimeProvider instance
    pub fn new() -> Self {
        Self
    }
}

impl TimeProvider for SyscallTimeProvider {
    fn now_ms(&self) -> u64 {
        let args = [0i32; SYSCALL_ARGS];
        let result = syscall(SYSCALL_TIME_MS, &args);
        result as u64
    }
}
```

### 4. Update output syscall wrapper (`lp-app/apps/fw-emu/src/output/syscall.rs`)

```rust
//! Syscall-based OutputProvider implementation
//!
//! Uses emulator syscalls to send LED output data to the host.

extern crate alloc;

use alloc::{rc::Rc, vec::Vec};
use core::cell::RefCell;

use lp_riscv_emu_guest::println;
use lp_shared::output::{OutputChannelHandle, OutputFormat, OutputProvider};
use lp_shared::OutputError;

/// Syscall-based OutputProvider implementation
///
/// For now, uses print logging to indicate output changes.
/// Output syscalls will be added later if needed.
pub struct SyscallOutputProvider {
    handles: RefCell<Vec<OutputChannelHandle>>,
    next_handle: RefCell<u32>,
}

impl SyscallOutputProvider {
    /// Create a new syscall-based OutputProvider instance
    pub fn new() -> Self {
        Self {
            handles: RefCell::new(Vec::new()),
            next_handle: RefCell::new(1),
        }
    }
}

impl OutputProvider for SyscallOutputProvider {
    fn open(
        &self,
        pin: u32,
        byte_count: u32,
        format: OutputFormat,
    ) -> Result<OutputChannelHandle, OutputError> {
        let handle = *self.next_handle.borrow();
        *self.next_handle.borrow_mut() += 1;
        self.handles.borrow_mut().push(handle);

        println!(
            "[output] open: pin={}, bytes={}, format={:?}, handle={}",
            pin, byte_count, format, handle
        );

        Ok(handle)
    }

    fn write(&self, handle: OutputChannelHandle, data: &[u8]) -> Result<(), OutputError> {
        println!(
            "[output] write: handle={}, len={}",
            handle, data.len()
        );
        // TODO: Implement syscall for writing LED data to host
        // For now, just succeed
        Ok(())
    }

    fn close(&self, handle: OutputChannelHandle) -> Result<(), OutputError> {
        println!("[output] close: handle={}", handle);
        // TODO: Implement syscall for closing output channel
        // For now, just succeed
        Ok(())
    }
}
```

### 5. Update main.rs imports (`lp-app/apps/fw-emu/src/main.rs`)

Update to use `lp_riscv_emu_guest`:

```rust
// Re-export _print so macros can find it
pub use lp_riscv_emu_guest::print::_print;

use lp_riscv_emu_guest::allocator;
```

## Validate

Run from `lp-app/` directory:

```bash
cd lp-app
cargo check --package fw-emu
```

Then build for RISC-V target:

```bash
cd lp-app/apps/fw-emu
RUSTFLAGS="-C target-feature=-c" cargo build --target riscv32imac-unknown-none-elf --release
```

Ensure:

- All syscall implementations compile
- No `todo!()` calls (except in output provider where noted)
- No warnings (except for TODO comments)
- Binary builds successfully for RISC-V target
