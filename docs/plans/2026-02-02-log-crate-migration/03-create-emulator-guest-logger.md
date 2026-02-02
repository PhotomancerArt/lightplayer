# Phase 3: Create Emulator Guest Logger

## Scope of phase

Create a logger implementation for `no_std` emulator guest code that routes all log calls to `SYSCALL_LOG` syscall.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Create Logger Module

**File**: `lp-riscv/lp-riscv-emu-guest/src/log.rs` (NEW)

```rust
//! Logger implementation for emulator guest code.
//!
//! Routes all log calls to SYSCALL_LOG syscall.

use log::{Level, Log, Metadata, Record};

use crate::host::__host_log;

/// Logger that routes to syscalls
pub struct SyscallLogger;

impl Log for SyscallLogger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        // Always enabled - filtering happens on host side
        true
    }

    fn log(&self, record: &Record) {
        let level = match record.level() {
            Level::Error => 0,
            Level::Warn => 1,
            Level::Info => 2,
            Level::Debug => 3,
            Level::Trace => 3, // Map trace to debug
        };

        // Get module path
        let module_path = record.module_path().unwrap_or("unknown");
        let module_path_bytes = module_path.as_bytes();

        // Format message
        // We need to format the message, but we're in no_std
        // Use a static buffer or format into guest memory
        // For now, use a simple approach: format into a static buffer
        // TODO: Consider using alloc::format! if alloc is available
        
        // Format message using core::fmt
        // We'll need to format the record.args() into a string
        // This is tricky in no_std - we may need to use a static buffer
        // For now, let's use a simple approach with a fixed-size buffer
        
        // Actually, we can use the format_args! macro and format into guest memory
        // But that's complex. Let's use a simpler approach:
        // Format the message into a static buffer (limited size)
        
        // Use a static buffer for formatting (256 bytes should be enough for most messages)
        // This is a limitation, but acceptable for now
        let mut buf = [0u8; 256];
        let mut writer = BufferWriter::new(&mut buf);
        let _ = core::fmt::write(&mut writer, *record.args());
        
        let msg_len = writer.len();
        let msg_bytes = &buf[..msg_len];

        // Call syscall
        unsafe {
            __host_log(
                level,
                module_path_bytes.as_ptr(),
                module_path_bytes.len(),
                msg_bytes.as_ptr(),
                msg_bytes.len(),
            );
        }
    }

    fn flush(&self) {
        // No-op for syscalls
    }
}

/// Simple buffer writer for formatting
struct BufferWriter<'a> {
    buf: &'a mut [u8],
    pos: usize,
}

impl<'a> BufferWriter<'a> {
    fn new(buf: &'a mut [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    fn len(&self) -> usize {
        self.pos
    }
}

impl<'a> core::fmt::Write for BufferWriter<'a> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let bytes = s.as_bytes();
        let remaining = self.buf.len() - self.pos;
        let to_write = bytes.len().min(remaining);
        self.buf[self.pos..self.pos + to_write].copy_from_slice(&bytes[..to_write]);
        self.pos += to_write;
        if to_write < bytes.len() {
            // Buffer full - truncate
            Ok(())
        } else {
            Ok(())
        }
    }
}
```

Actually, formatting in no_std is complex. Let's use a simpler approach - use `alloc::format!` if alloc is available, or use a simpler message format:

**Simpler approach**:

```rust
//! Logger implementation for emulator guest code.
//!
//! Routes all log calls to SYSCALL_LOG syscall.

extern crate alloc;

use alloc::{format, string::String};
use log::{Level, Log, Metadata, Record};

use crate::host::__host_log;

/// Logger that routes to syscalls
pub struct SyscallLogger;

impl Log for SyscallLogger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        // Always enabled - filtering happens on host side
        true
    }

    fn log(&self, record: &Record) {
        let level = match record.level() {
            Level::Error => 0,
            Level::Warn => 1,
            Level::Info => 2,
            Level::Debug => 3,
            Level::Trace => 3, // Map trace to debug
        };

        // Get module path
        let module_path = record.module_path().unwrap_or("unknown");
        let module_path_bytes = module_path.as_bytes();

        // Format message using alloc::format!
        let msg = format!("{}", record.args());
        let msg_bytes = msg.as_bytes();

        // Call syscall
        unsafe {
            __host_log(
                level,
                module_path_bytes.as_ptr(),
                module_path_bytes.len(),
                msg_bytes.as_ptr(),
                msg_bytes.len(),
            );
        }
    }

    fn flush(&self) {
        // No-op for syscalls
    }
}

/// Initialize the syscall logger
///
/// Call this once at startup in emulator guest code.
pub fn init() {
    let logger = alloc::boxed::Box::new(SyscallLogger);
    log::set_logger(alloc::boxed::Box::leak(logger))
        .map(|()| log::set_max_level(log::LevelFilter::Trace))
        .expect("Failed to set syscall logger");
}
```

### 2. Export Logger

**File**: `lp-riscv/lp-riscv-emu-guest/src/lib.rs`

Add:

```rust
pub mod log;

pub use log::init as init_logger;
```

### 3. Update Cargo.toml

**File**: `lp-riscv/lp-riscv-emu-guest/Cargo.toml`

Ensure `log` dependency is present:

```toml
[dependencies]
log = { version = "0.4", default-features = false }
```

## Tests

No tests needed for this phase - the logger will be tested when integrated with fw-emu.

## Validate

Run from workspace root:

```bash
cargo check --package lp-riscv-emu-guest
```

Ensure:
- Logger compiles
- No allocation issues
- Logger can be initialized
