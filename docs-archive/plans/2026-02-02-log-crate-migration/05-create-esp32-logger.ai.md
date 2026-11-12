# Phase 5: Create ESP32 Logger

## Scope of phase

Create a logger implementation for ESP32 `no_std` environment that routes to `esp_println` with hardcoded `info` level filtering.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Create ESP32 Logger Module

**File**: `lp-fw/fw-core/src/log/esp32.rs` (NEW)

```rust
//! ESP32 logger implementation.
//!
//! Routes log calls to esp_println with hardcoded info level filtering.

extern crate alloc;

use alloc::format;
use log::{Level, LevelFilter, Log, Metadata, Record};

/// ESP32 logger that routes to esp_println
pub struct Esp32Logger {
    max_level: LevelFilter,
}

impl Esp32Logger {
    /// Create a new ESP32 logger with the given max level
    pub fn new(max_level: LevelFilter) -> Self {
        Self { max_level }
    }

    /// Create a new ESP32 logger with default info level
    pub fn default() -> Self {
        Self::new(LevelFilter::Info)
    }
}

impl Log for Esp32Logger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= self.max_level
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        let module_path = record.module_path().unwrap_or("unknown");
        let level_str = match record.level() {
            Level::Error => "ERROR",
            Level::Warn => "WARN",
            Level::Info => "INFO",
            Level::Debug => "DEBUG",
            Level::Trace => "TRACE",
        };

        // Format message
        let msg = format!("{}", record.args());
        
        // Print using esp_println
        // Note: This requires esp_println to be available
        // We'll use a feature gate or conditional compilation
        #[cfg(feature = "esp32")]
        {
            esp_println::println!("[{}] {}: {}", level_str, module_path, msg);
        }
        
        // For non-ESP32 builds (tests), use a no-op or different output
        #[cfg(not(feature = "esp32"))]
        {
            // In tests or non-ESP32 builds, we can't use esp_println
            // This is okay - the logger won't be used in those contexts
        }
    }

    fn flush(&self) {
        // No-op for esp_println
    }
}

/// Initialize the ESP32 logger
///
/// Call this once at startup in ESP32 firmware.
#[cfg(feature = "esp32")]
pub fn init() {
    let logger = alloc::boxed::Box::new(Esp32Logger::default());
    log::set_logger(alloc::boxed::Box::leak(logger))
        .map(|()| log::set_max_level(LevelFilter::Info))
        .expect("Failed to set ESP32 logger");
}
```

Actually, we can't use `esp_println` from `fw-core` since it's a generic crate. Let's use a different approach - provide a function pointer or trait:

**Better approach**:

```rust
//! ESP32 logger implementation.
//!
//! Routes log calls to a provided print function (typically esp_println).

extern crate alloc;

use alloc::format;
use log::{Level, LevelFilter, Log, Metadata, Record};

/// Function type for printing log messages
pub type PrintFn = fn(&str);

/// ESP32 logger that routes to a print function
pub struct Esp32Logger {
    max_level: LevelFilter,
    print_fn: PrintFn,
}

impl Esp32Logger {
    /// Create a new ESP32 logger with the given max level and print function
    pub fn new(max_level: LevelFilter, print_fn: PrintFn) -> Self {
        Self { max_level, print_fn }
    }

    /// Create a new ESP32 logger with default info level
    pub fn default(print_fn: PrintFn) -> Self {
        Self::new(LevelFilter::Info, print_fn)
    }
}

impl Log for Esp32Logger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= self.max_level
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        let module_path = record.module_path().unwrap_or("unknown");
        let level_str = match record.level() {
            Level::Error => "ERROR",
            Level::Warn => "WARN",
            Level::Info => "INFO",
            Level::Debug => "DEBUG",
            Level::Trace => "TRACE",
        };

        // Format message
        let msg = format!("[{}] {}: {}", level_str, module_path, record.args());
        
        // Call print function
        (self.print_fn)(&msg);
    }

    fn flush(&self) {
        // No-op
    }
}

/// Initialize the ESP32 logger with a print function
///
/// Call this once at startup in ESP32 firmware.
pub fn init(print_fn: PrintFn) {
    let logger = alloc::boxed::Box::new(Esp32Logger::default(print_fn));
    log::set_logger(alloc::boxed::Box::leak(logger))
        .map(|()| log::set_max_level(LevelFilter::Info))
        .expect("Failed to set ESP32 logger");
}
```

### 2. Export Logger

**File**: `lp-fw/fw-core/src/log/mod.rs` (NEW)

```rust
#[cfg(feature = "esp32")]
pub mod esp32;

#[cfg(feature = "esp32")]
pub use esp32::{init as init_esp32_logger, PrintFn};
```

### 3. Update fw-esp32 to Use Logger

**File**: `lp-fw/fw-esp32/src/main.rs`

Add logger initialization:

```rust
use fw_core::log::init_esp32_logger;

#[esp_rtos::main]
async fn main(_spawner: embassy_executor::Spawner) {
    // Initialize logger with esp_println
    init_esp32_logger(|s| {
        esp_println::println!("{}", s);
    });

    // ... rest of main
}
```

### 4. Update Cargo.toml

**File**: `lp-fw/fw-core/Cargo.toml`

Add log dependency and esp32 feature:

```toml
[dependencies]
log = { version = "0.4", default-features = false }

[features]
default = []
std = []
esp32 = []
```

## Tests

No tests needed for this phase - ESP32 logger will be tested when integrated with fw-esp32.

## Validate

Run from workspace root:

```bash
cargo check --package fw-core --features esp32
cargo check --package fw-esp32
```

Ensure:
- Logger compiles
- fw-esp32 can initialize logger
- No compilation errors
