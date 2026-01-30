# Phase 8: Implement ESP32 server loop and main

## Scope of phase

Implement the ESP32 server loop and complete the main entry point. This integrates all components (SerialTransport, LpServer, OutputProvider) and runs the main loop using Embassy async runtime.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Update server_loop.rs

Implement the server loop:

```rust
//! Server loop for ESP32 firmware
//!
//! Main loop that handles hardware I/O and calls lp-server::tick().

use embassy_time::{Duration, Instant, Timer};
use fw_core::transport::SerialTransport;
use lp_model::Message;
use lp_server::LpServer;
use lp_shared::fs::LpFsMemory;
use lp_shared::time::TimeProvider;

use crate::output::Esp32OutputProvider;
use crate::serial::Esp32UsbSerialIo;

/// Target frame time for 60 FPS (16.67ms per frame)
const TARGET_FRAME_TIME_MS: u64 = 16;

/// Run the server loop
///
/// This is the main loop that processes incoming messages and sends responses.
/// Runs at ~60 FPS to maintain consistent frame timing.
pub async fn run_server_loop(
    mut server: LpServer,
    mut transport: SerialTransport<Esp32UsbSerialIo>,
) -> Result<(), lp_server::ServerError> {
    let mut last_tick = Instant::now();

    loop {
        let frame_start = Instant::now();

        // Collect incoming messages (non-blocking)
        let mut incoming_messages = Vec::new();
        loop {
            match transport.receive() {
                Ok(Some(msg)) => {
                    incoming_messages.push(Message::Client(msg));
                }
                Ok(None) => {
                    // No more messages available
                    break;
                }
                Err(e) => {
                    // Transport error - log and continue
                    // In no_std, we can't easily log, so just break
                    break;
                }
            }
        }

        // Calculate delta time since last tick
        let delta_time = last_tick.elapsed();
        let delta_ms = delta_time.as_millis().min(u32::MAX as u64) as u32;

        // Tick server (synchronous)
        match server.tick(delta_ms.max(1), incoming_messages) {
            Ok(responses) => {
                // Send responses
                for response in responses {
                    if let Message::Server(server_msg) = response {
                        if let Err(_e) = transport.send(server_msg) {
                            // Transport error - continue with next message
                        }
                    }
                }
            }
            Err(e) => {
                // Server error - log and continue
                // In no_std, we can't easily log, so just continue
            }
        }

        last_tick = frame_start;

        // Sleep to maintain ~60 FPS
        let frame_duration = frame_start.elapsed();
        if frame_duration < Duration::from_millis(TARGET_FRAME_TIME_MS) {
            let sleep_duration = Duration::from_millis(TARGET_FRAME_TIME_MS) - frame_duration;
            Timer::after(sleep_duration).await;
        } else {
            // Frame took too long - yield to other tasks
            embassy_futures::yield_now().await;
        }
    }
}
```

### 2. Update main.rs

Complete the main entry point:

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

use alloc::{boxed::Box, rc::Rc};
use core::cell::RefCell;

use esp_backtrace as _;
use esp_println::println;
use fw_core::transport::SerialTransport;
use lp_model::AsLpPath;
use lp_server::LpServer;
use lp_shared::fs::LpFsMemory;
use lp_shared::output::OutputProvider;

use board::{init_board, start_runtime};
use output::Esp32OutputProvider;
use serial::Esp32UsbSerialIo;

esp_bootloader_esp_idf::esp_app_desc!();

#[embassy_executor::task]
async fn server_task() {
    println!("Server task starting...");

    // Create filesystem (in-memory for now)
    let base_fs = Box::new(LpFsMemory::new());

    // Create output provider
    let output_provider: Rc<RefCell<dyn OutputProvider>> =
        Rc::new(RefCell::new(Esp32OutputProvider::new()));

    // Create server
    let mut server = LpServer::new(
        output_provider,
        base_fs,
        "projects/".as_path(),
    );

    // TODO: Initialize USB-serial and create transport
    // For now, this is a stub - will need to get USB-serial from peripherals
    // let usb_serial = UsbSerialJtag::new(peripherals.USB_SERIAL_JTAG);
    // let serial_io = Esp32UsbSerialIo::new(usb_serial);
    // let transport = SerialTransport::new(serial_io);

    // TODO: Run server loop once transport is set up
    // run_server_loop(server, transport).await;

    println!("Server task running (stub)");
}

#[esp_rtos::main]
async fn main(spawner: embassy_executor::Spawner) {
    esp_println::logger::init_logger_from_env();

    println!("fw-esp32 starting...");

    let (peripherals, sw_int, timg0) = init_board();
    start_runtime(timg0.timer0, sw_int.software_interrupt0);

    // Spawn server task
    spawner.spawn(server_task()).ok();

    println!("fw-esp32 initialized");
}
```

### 3. Add embassy-futures dependency

Update `Cargo.toml`:

```toml
[dependencies]
# ... existing dependencies ...
embassy-futures = "0.1.0"
```

## Notes

- USB-serial initialization needs to be integrated properly - may need to pass peripherals to the task
- Server loop runs in a separate task to allow other tasks if needed
- Frame timing uses Embassy's `Timer` for async sleep
- Error handling is minimal in `no_std` - adjust as needed

## Validate

Run from `lp-app/` directory:

```bash
cd lp-app
cargo check --package fw-esp32 --features esp32c6
```

Ensure:
- Server loop compiles
- Main entry point integrates all components
- No warnings (except for TODO stubs for USB-serial integration)

Note: Full compilation may require ESP32 toolchain setup, but structure should be valid.
