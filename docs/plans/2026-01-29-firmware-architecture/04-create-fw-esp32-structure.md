# Phase 4: Create fw-esp32 app structure

## Scope of phase

Create the `fw-esp32` app structure with Cargo.toml, basic project layout, and board-specific code module. This sets up the foundation for ESP32 firmware.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later
- Board-specific code should be in a single file for easy copy-paste

## Implementation Details

### 1. Create app structure

Create `lp-app/apps/fw-esp32/` directory with:

- `Cargo.toml` - App configuration
- `src/main.rs` - Main entry point (stub for now)
- `src/board/mod.rs` - Board module
- `src/board/esp32c6.rs` - ESP32-C6 specific code
- `src/serial/mod.rs` - Serial module (empty for now)
- `src/output/mod.rs` - Output module (empty for now)
- `src/server_loop.rs` - Server loop (stub for now)

### 2. Cargo.toml

```toml
[package]
name = "fw-esp32"
version.workspace = true
authors.workspace = true
edition.workspace = true
license.workspace = true
rust-version.workspace = true

[features]
default = ["esp32c6"]
esp32c6 = [
    "esp-backtrace/esp32c6",
    "esp-bootloader-esp-idf/esp32c6",
    "esp-rtos/esp32c6",
    "esp-hal/esp32c6",
]

[dependencies]
embassy-executor = "0.9.1"
embassy-time = "0.4.0"
esp-backtrace = { version = "0.18.1", features = ["panic-handler", "println"] }
esp-bootloader-esp-idf = { version = "0.2.0" }
esp-hal = { version = "1.0.0", features = ["log-04", "unstable"] }
esp-rtos = { version = "0.2.0", features = ["embassy", "log-04"] }
esp-println = { version = "0.16.1", features = ["log-04"] }
esp-alloc = { version = "0.8.0", features = ["internal-heap-stats"] }

fw-core = { path = "../../crates/fw-core", default-features = false }
lp-server = { path = "../../crates/lp-server", default-features = false }
lp-shared = { path = "../../crates/lp-shared", default-features = false }
lp-model = { path = "../../crates/lp-model", default-features = false }
hashbrown = { workspace = true }
```

### 3. main.rs

Create a stub main entry point:

```rust
//! ESP32 firmware application
//!
//! Main entry point for ESP32 firmware running lp-server.

#![no_std]
#![no_main]

extern crate alloc;

mod board;
mod output;
mod serial;
mod server_loop;

use esp_backtrace as _;
use esp_println::println;

esp_bootloader_esp_idf::esp_app_desc!();

#[esp_rtos::main]
async fn main(_spawner: embassy_executor::Spawner) {
    esp_println::logger::init_logger_from_env();

    println!("fw-esp32 starting...");

    // TODO: Initialize hardware
    // TODO: Create server
    // TODO: Run server loop

    println!("fw-esp32 initialized (stub)");
}
```

### 4. board/mod.rs

```rust
#[cfg(feature = "esp32c6")]
pub mod esp32c6;

#[cfg(feature = "esp32c6")]
pub use esp32c6::*;
```

### 5. board/esp32c6.rs

Create ESP32-C6 specific initialization:

```rust
//! ESP32-C6 specific board initialization
//!
//! This module contains board-specific code for ESP32-C6.
//! To add support for another board (e.g., ESP32-C3), create a similar file
//! and add feature gates in board/mod.rs.

use esp_hal::clock::CpuClock;
use esp_hal::{interrupt::software::SoftwareInterruptControl, timer::timg::TimerGroup};

/// Initialize ESP32-C6 hardware
///
/// Sets up CPU clock, timers, and other board-specific hardware.
/// Returns peripherals and runtime components needed for Embassy.
pub fn init_board() -> (
    esp_hal::Peripherals,
    SoftwareInterruptControl<'static>,
    TimerGroup<'static>,
) {
    // Configure CPU clock to maximum speed (160MHz for ESP32-C6)
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // Allocate heap
    esp_alloc::heap_allocator!(size: 300_000);

    // Set up software interrupt and timer for Embassy runtime
    let sw_int = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    let timg0 = TimerGroup::new(peripherals.TIMG0);

    (peripherals, sw_int, timg0)
}

/// Start Embassy runtime
///
/// Starts the Embassy async runtime with the given timer and software interrupt.
pub fn start_runtime(timer: esp_hal::timer::timg::Timer<'static>, sw_int: esp_hal::interrupt::software::SoftwareInterrupt0) {
    esp_rtos::start(timer, sw_int);
}
```

### 6. serial/mod.rs

```rust
// Serial module - will be implemented in next phase
```

### 7. output/mod.rs

```rust
// Output module - will be implemented in later phase
```

### 8. server_loop.rs

Create a stub server loop:

```rust
//! Server loop for ESP32 firmware
//!
//! Main loop that handles hardware I/O and calls lp-server::tick().

// TODO: Implement server loop
// This will be implemented in a later phase
```

## Validate

Run from `lp-app/` directory:

```bash
cd lp-app
cargo check --package fw-esp32 --features esp32c6
```

Ensure:

- App structure compiles
- Feature flags work correctly
- No warnings (except for TODO stubs)

Note: Full compilation may require ESP32 toolchain setup, but structure should be valid.
