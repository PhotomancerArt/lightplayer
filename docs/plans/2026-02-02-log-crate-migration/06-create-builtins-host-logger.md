# Phase 6: Create Builtins Host Logger

## Scope of phase

Create logger implementation for GLSL builtins that works in both emulator and JIT contexts. Routes to `__host_log` function which delegates to syscalls (emulator) or log crate (JIT).

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Create Builtins Logger Module

**File**: `lp-glsl/lp-glsl-builtins/src/host/logger.rs` (NEW)

```rust
//! Logger implementation for GLSL builtins.
//!
//! Routes log calls to __host_log function which works in both
//! emulator (syscalls) and JIT (log crate) contexts.

extern crate alloc;

use alloc::{format, string::String};
use log::{Level, Log, Metadata, Record};

/// External function for logging (provided by emulator or JIT)
extern "C" {
    fn __host_log(
        level: u8,
        module_path_ptr: *const u8,
        module_path_len: usize,
        msg_ptr: *const u8,
        msg_len: usize,
    );
}

/// Logger that routes to __host_log
pub struct BuiltinsLogger;

impl Log for BuiltinsLogger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        // Always enabled - filtering happens on host side (emulator) or via log crate (JIT)
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
        let msg = format!("{}", record.args());
        let msg_bytes = msg.as_bytes();

        // Call __host_log
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

/// Initialize the builtins logger
///
/// Call this once before running GLSL code.
pub fn init() {
    let logger = alloc::boxed::Box::new(BuiltinsLogger);
    log::set_logger(alloc::boxed::Box::leak(logger))
        .map(|()| log::set_max_level(log::LevelFilter::Trace))
        .expect("Failed to set builtins logger");
}
```

### 2. Export Logger

**File**: `lp-glsl/lp-glsl-builtins/src/host/mod.rs`

Add:

```rust
pub mod logger;

pub use logger::{init as init_logger};
```

### 3. Update Cargo.toml

**File**: `lp-glsl/lp-glsl-builtins/Cargo.toml`

Ensure log dependency:

```toml
[dependencies]
log = { version = "0.4", default-features = false }
```

## Tests

No tests needed for this phase - logger will be tested when integrated with GLSL compiler.

## Validate

Run from workspace root:

```bash
cargo check --package lp-glsl-builtins
```

Ensure:
- Logger compiles
- `__host_log` function is declared correctly
- No compilation errors
