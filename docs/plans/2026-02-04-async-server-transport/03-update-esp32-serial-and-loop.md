# Phase 3: Update ESP32 Serial Implementation and Server Loop

## Scope of phase

Update ESP32 to use async USB serial directly in `SerialTransport`, and update the server loop to use async transport with `.await`.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Update `lp-fw/fw-esp32/src/serial/usb_serial.rs`

Remove the `SerialIo` implementation. Instead, we'll split USB serial into tx/rx and pass them directly to `SerialTransport`:

```rust
//! ESP32 USB-serial async I/O
//!
//! Provides async USB serial halves for use with SerialTransport.

use esp_hal::{
    Async,
    usb_serial_jtag::{UsbSerialJtag, UsbSerialJtagRx, UsbSerialJtagTx},
};

/// ESP32 USB-serial async I/O halves
///
/// Split USB serial into rx/tx halves for async operations.
pub struct Esp32UsbSerial {
    /// RX (read) half
    pub rx: UsbSerialJtagRx<'static, Async>,
    /// TX (write) half
    pub tx: UsbSerialJtagTx<'static, Async>,
}

impl Esp32UsbSerial {
    /// Create a new USB-serial instance and split into rx/tx
    ///
    /// # Arguments
    /// * `usb_serial` - Initialized USB-serial interface (will be split into rx/tx)
    pub fn new(usb_serial: UsbSerialJtag<'static, Async>) -> Self {
        let (rx, tx) = usb_serial.split();
        Self { rx, tx }
    }
}
```

**Key changes:**
- Removed `SerialIo` implementation
- Simple struct that holds split rx/tx halves
- No `block_on` - direct async types

### 2. Update `lp-fw/fw-esp32/src/main.rs`

Update to create `SerialTransport` with async halves:

```rust
// ... existing code ...

// Initialize USB-serial - split into rx/tx for async transport
let usb_serial = UsbSerialJtag::new(usb_device);
let usb_serial_async = usb_serial.into_async();
let serial_io = Esp32UsbSerial::new(usb_serial_async);

// Create serial transport with async halves
let transport = SerialTransport::new(serial_io.tx, serial_io.rx);

// ... rest of initialization ...
```

**Note:** We need to ensure `serial_io` is stored somewhere accessible, or we need to restructure so the transport owns the halves.

### 3. Update `lp-fw/fw-esp32/src/server_loop.rs`

Update to use async transport with `.await`:

```rust
//! Server loop for ESP32 firmware
//!
//! Main async loop that handles hardware I/O and calls lp-server::tick().

extern crate alloc;

use alloc::vec::Vec;
use fw_core::transport::SerialTransport;
use lp_model::Message;
use lp_server::LpServer;
use lp_shared::time::TimeProvider;
use lp_shared::transport::ServerTransport;

use crate::serial::Esp32UsbSerial;
use crate::time::Esp32TimeProvider;

/// FPS logging interval (log every N frames)
const FPS_LOG_INTERVAL: u32 = 60;

/// Run the server loop
///
/// This is the main async loop that processes incoming messages and sends responses.
/// Runs at ~60 FPS to maintain consistent frame timing.
/// Yields control back to Embassy runtime between iterations.
pub async fn run_server_loop(
    mut server: LpServer,
    mut transport: SerialTransport<Esp32UsbSerial::tx_type, Esp32UsbSerial::rx_type>,
    time_provider: Esp32TimeProvider,
) -> ! {
    let mut last_tick = time_provider.now_ms();
    let mut frame_count = 0u32;
    let mut fps_last_log_time = time_provider.now_ms();

    loop {
        let frame_start = time_provider.now_ms();

        // Collect incoming messages (non-blocking)
        let mut incoming_messages = Vec::new();
        loop {
            match transport.receive().await {
                Ok(Some(msg)) => {
                    incoming_messages.push(Message::Client(msg));
                }
                Ok(None) => {
                    // No more messages available
                    break;
                }
                Err(e) => {
                    // Transport error - log and continue
                    log::warn!("run_server_loop: Transport error: {:?}", e);
                    break;
                }
            }
        }

        // Calculate delta time since last tick
        let delta_time = time_provider.elapsed_ms(last_tick);
        let delta_ms = delta_time.min(u32::MAX as u64) as u32;

        // Tick server (synchronous)
        match server.tick(delta_ms.max(1), incoming_messages) {
            Ok(responses) => {
                // Send responses
                for response in responses {
                    if let Message::Server(server_msg) = response {
                        if let Err(e) = transport.send(server_msg).await {
                            log::warn!("run_server_loop: Failed to send response: {:?}", e);
                            // Transport error - continue with next message
                        }
                    }
                }
            }
            Err(e) => {
                log::warn!("run_server_loop: Server tick error: {:?}", e);
                // Server error - continue
            }
        }

        last_tick = frame_start;
        frame_count += 1;

        // Log FPS periodically
        if frame_count % FPS_LOG_INTERVAL == 0 {
            let current_time = time_provider.now_ms();
            let elapsed_ms = current_time.saturating_sub(fps_last_log_time);
            if elapsed_ms > 0 {
                let fps = (FPS_LOG_INTERVAL as u64 * 1000) / elapsed_ms;
                log::info!(
                    "FPS: {} (frame_count: {}, elapsed: {}ms)",
                    fps,
                    frame_count,
                    elapsed_ms
                );
                fps_last_log_time = current_time;
            }
        }

        // Yield to Embassy runtime (allows other tasks to run)
        embassy_time::Timer::after(embassy_time::Duration::from_millis(1)).await;
    }
}
```

**Key changes:**
- `transport.receive()` now uses `.await`
- `transport.send()` now uses `.await`
- All async operations properly awaited

### 4. Update logger to use async (if needed)

The logger may still need updates. Check if it's calling transport methods. If the logger is separate from transport, it may not need changes in this phase.

## Tests

Update any ESP32-specific tests to use async:

```rust
#[tokio::test]  // or embassy test runtime
async fn test_esp32_transport() {
    // Test implementation
}
```

## Validate

Run:
```bash
cd lp-fw/fw-esp32
cargo check --target riscv32imac-unknown-none-elf --features esp32c6
```

**Expected:** Code compiles. ESP32 should now use async transport directly without `block_on` deadlocks.
