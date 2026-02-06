# Phase 5: Implement Server Loop

## Scope of phase

Create the async server loop that processes messages and calls `server.tick()`. Similar to fw-emu but adapted for async runtime.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Create server_loop.rs

Implement async server loop:

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

use crate::serial::Esp32UsbSerialIo;
use crate::time::Esp32TimeProvider;

/// Target frame time for 60 FPS (16.67ms per frame)
const TARGET_FRAME_TIME_MS: u32 = 16;

/// Run the server loop
///
/// This is the main async loop that processes incoming messages and sends responses.
/// Runs at ~60 FPS to maintain consistent frame timing.
/// Yields control back to Embassy runtime between iterations.
pub async fn run_server_loop(
    mut server: LpServer,
    mut transport: SerialTransport<Esp32UsbSerialIo>,
    time_provider: Esp32TimeProvider,
) -> ! {
    let mut last_tick = time_provider.now_ms();

    loop {
        let frame_start = time_provider.now_ms();

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
                        if let Err(e) = transport.send(server_msg) {
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

        // Yield to Embassy runtime (allows other tasks to run)
        // Use embassy_time::Timer to delay slightly, or just yield
        embassy_time::Timer::after(embassy_time::Duration::from_millis(1)).await;
    }
}
```

### 2. Update main.rs (stub for now)

Add server_loop module:

```rust
mod server_loop;

use server_loop::run_server_loop;
```

## Notes

- The server loop is async and yields to Embassy between iterations
- We use a small delay (1ms) to yield to other tasks
- Similar structure to fw-emu, but async
- `server.tick()` is synchronous, so we call it from async context (this is fine)

## Validate

Run:
```bash
cd lp-fw/fw-esp32
cargo check --features esp32c6
```

Expected: Code compiles without errors.
