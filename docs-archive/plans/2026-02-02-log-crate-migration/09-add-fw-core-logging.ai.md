# Phase 9: Add fw-core Logging Infrastructure

## Scope of phase

Add `log` crate dependency to `fw-core`, create logger module structure, and add a few example debug logs in `SerialTransport` to verify logging works.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Add log Dependency

**File**: `lp-fw/fw-core/Cargo.toml`

Add log dependency:

```toml
[dependencies]
log = { version = "0.4", default-features = false }
# ... existing dependencies ...
```

### 2. Create Logger Module Structure

**File**: `lp-fw/fw-core/src/log/mod.rs` (NEW)

```rust
//! Logging infrastructure for fw-core.
//!
//! Provides logger implementations for different environments:
//! - Emulator: Routes to syscalls
//! - ESP32: Routes to esp_println

#[cfg(feature = "emu")]
pub mod emu;

#[cfg(feature = "esp32")]
pub mod esp32;

// Re-export initialization functions
#[cfg(feature = "emu")]
pub use emu::init as init_emu_logger;

#[cfg(feature = "esp32")]
pub use esp32::{init as init_esp32_logger, PrintFn};
```

### 3. Create Emulator Logger

**File**: `lp-fw/fw-core/src/log/emu.rs` (NEW)

```rust
//! Emulator logger implementation.
//!
//! Routes log calls to syscalls via __host_log.

extern crate alloc;

use alloc::{format, string::String};
use log::{Level, LevelFilter, Log, Metadata, Record};

/// External function for logging (provided by lp-riscv-emu-guest)
extern "C" {
    fn __host_log(
        level: u8,
        module_path_ptr: *const u8,
        module_path_len: usize,
        msg_ptr: *const u8,
        msg_len: usize,
    );
}

/// Logger that routes to syscalls
pub struct EmuLogger;

impl Log for EmuLogger {
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
            Level::Trace => 3,
        };

        let module_path = record.module_path().unwrap_or("unknown");
        let module_path_bytes = module_path.as_bytes();

        let msg = format!("{}", record.args());
        let msg_bytes = msg.as_bytes();

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
        // No-op
    }
}

/// Initialize the emulator logger
pub fn init() {
    let logger = alloc::boxed::Box::new(EmuLogger);
    log::set_logger(alloc::boxed::Box::leak(logger))
        .map(|()| log::set_max_level(LevelFilter::Trace))
        .expect("Failed to set emulator logger");
}
```

### 4. Export Logger Module

**File**: `lp-fw/fw-core/src/lib.rs`

Add:

```rust
#[cfg(any(feature = "emu", feature = "esp32"))]
pub mod log;
```

### 5. Add Example Logs to SerialTransport

**File**: `lp-fw/fw-core/src/transport/serial.rs`

Add a few debug logs:

```rust
use log::debug;

impl<Io: SerialIo> ServerTransport for SerialTransport<Io> {
    fn send(&mut self, msg: ServerMessage) -> Result<(), TransportError> {
        debug!("SerialTransport: Sending message");
        
        // ... existing code ...
        
        debug!("SerialTransport: Sent {} bytes", json_bytes.len() + 1);
        Ok(())
    }

    fn receive(&mut self) -> Result<Option<ClientMessage>, TransportError> {
        // ... existing code ...
        
        if let Some(newline_pos) = self.read_buffer.iter().position(|&b| b == b'\n') {
            debug!("SerialTransport: Received complete message ({} bytes)", newline_pos + 1);
            // ... existing code ...
        } else {
            debug!("SerialTransport: No complete message yet ({} bytes buffered)", self.read_buffer.len());
        }
        
        // ... existing code ...
    }
}
```

### 6. Update fw-emu to Initialize Logger

**File**: `lp-fw/fw-emu/src/main.rs`

Add logger initialization:

```rust
use fw_core::log::init_emu_logger;

#[unsafe(no_mangle)]
pub extern "C" fn _lp_main() -> ! {
    // Initialize logger first
    init_emu_logger();
    
    // ... rest of initialization ...
}
```

### 7. Update Cargo.toml Features

**File**: `lp-fw/fw-core/Cargo.toml`

Add features:

```toml
[features]
default = []
std = []
emu = []
esp32 = []
```

**File**: `lp-fw/fw-emu/Cargo.toml`

Enable emu feature:

```toml
[dependencies]
fw-core = { path = "../fw-core", default-features = false, features = ["emu"] }
```

## Tests

No tests needed for this phase - logging will be verified when running fw-emu.

## Validate

Run from workspace root:

```bash
cargo check --package fw-core --features emu
cargo check --package fw-emu
```

Ensure:
- fw-core compiles with emu feature
- fw-emu compiles and can initialize logger
- Example logs are added to SerialTransport
- No compilation errors
