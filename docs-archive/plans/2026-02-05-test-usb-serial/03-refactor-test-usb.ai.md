# Phase 3: Refactor test_usb to Use MessageRouter

## Scope of phase

Replace the current `test_usb` implementation with a new version that uses `MessageRouter`, implements the main loop pattern (`blink_led()` → `handle_messages()`), and includes frame counter tracking.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Replace test_usb.rs

Replace `lp-fw/fw-esp32/src/tests/test_usb.rs`:

```rust
//! USB serial test mode
//!
//! When `test_usb` feature is enabled, this tests USB serial communication
//! by running a main loop that blinks LEDs and handles messages via MessageRouter.

extern crate alloc;

use alloc::{format, string::String, vec::Vec};
use core::sync::atomic::{AtomicU32, Ordering};
use embassy_time::{Duration, Timer};
use embassy_sync::channel::Channel;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use esp_hal::{rmt::Rmt, time::Rate, usb_serial_jtag::UsbSerialJtag};

use crate::board::{init_board, start_runtime};
use crate::output::LedChannel;
use fw_core::message_router::MessageRouter;
use fw_core::test_messages::{TestCommand, TestResponse, serialize_response, deserialize_command};

/// Frame counter (atomic, incremented each main loop iteration)
static FRAME_COUNT: AtomicU32 = AtomicU32::new(0);

/// Message channels (static for MessageRouter)
static INCOMING_MSG: Channel<CriticalSectionRawMutex, String, 32> = Channel::new();
static OUTGOING_MSG: Channel<CriticalSectionRawMutex, String, 32> = Channel::new();

/// Run USB serial test
///
/// Sets up:
/// - LED blinking task (2Hz, visual indicator)
/// - Main loop (blink LED, handle messages, increment frame counter)
/// - I/O task (handles serial communication)
pub async fn run_usb_test() -> ! {
    // Initialize board (clock, heap, runtime) and get hardware peripherals
    let (sw_int, timg0, rmt_peripheral, usb_device, gpio18) = init_board();
    start_runtime(timg0, sw_int);

    // Initialize RMT driver for LED blinking
    let rmt = Rmt::new(rmt_peripheral, Rate::from_mhz(80))
        .expect("Failed to initialize RMT");
    let pin = gpio18;
    const NUM_LEDS: usize = 1; // Only need first LED for blinking
    let mut channel = LedChannel::new(rmt, pin, NUM_LEDS)
        .expect("Failed to initialize LED channel");

    // Create message router
    let router = MessageRouter::new(&INCOMING_MSG, &OUTGOING_MSG);

    // Spawn I/O task (handles serial communication)
    // TODO: Will be implemented in next phase
    // spawner.spawn(io_task(router, usb_device)).ok();

    // Main loop: blink LED, handle messages, increment frame counter
    let mut led_state = false;
    let mut last_led_toggle = embassy_time::Instant::now();
    
    loop {
        // Blink LED at 2Hz (500ms period)
        let now = embassy_time::Instant::now();
        if now.duration_since(last_led_toggle).as_millis() >= 500 {
            led_state = !led_state;
            
            let mut led_data = [0u8; 3];
            if led_state {
                led_data[0] = 5; // R
                led_data[1] = 5; // G
                led_data[2] = 5; // B
            } else {
                led_data[0] = 0;
                led_data[1] = 0;
                led_data[2] = 0;
            }
            
            let tx = channel.start_transmission(&led_data);
            channel = tx.wait_complete();
            last_led_toggle = now;
        }
        
        // Handle messages
        handle_messages(&router);
        
        // Increment frame counter
        FRAME_COUNT.fetch_add(1, Ordering::Relaxed);
        
        // Small delay to yield to other tasks
        Timer::after(Duration::from_millis(1)).await;
    }
}

/// Handle incoming messages from the router
///
/// Processes commands and sends responses.
fn handle_messages(router: &MessageRouter) {
    let messages = router.receive_all();
    
    for msg_line in messages {
        // Parse command
        let cmd = match deserialize_command(&msg_line) {
            Ok(Some(cmd)) => cmd,
            Ok(None) => continue, // Not a message line
            Err(e) => {
                // Parse error - ignore
                #[cfg(feature = "esp32c6")]
                log::warn!("Failed to parse command: {:?}", e);
                continue;
            }
        };
        
        // Handle command and send response
        let response = match cmd {
            TestCommand::GetFrameCount => {
                let count = FRAME_COUNT.load(Ordering::Relaxed);
                TestResponse::FrameCount { frame_count: count }
            }
            TestCommand::Echo { data } => {
                TestResponse::Echo { echo: data }
            }
        };
        
        // Serialize and send response
        if let Ok(resp_msg) = serialize_response(&response) {
            if let Err(_) = router.send(resp_msg) {
                // Channel full - log warning but continue
                #[cfg(feature = "esp32c6")]
                log::warn!("Outgoing channel full, dropping response");
            }
        }
    }
}
```

### 2. Update main.rs

Ensure `test_usb` feature properly calls the new implementation. The existing code should already work:

```rust
#[cfg(feature = "test_usb")]
{
    use tests::test_usb::run_usb_test;
    run_usb_test().await;
}
```

### 3. Add embassy-sync dependency

Update `lp-fw/fw-esp32/Cargo.toml`:
```toml
[dependencies]
# ... existing dependencies ...
embassy-sync = "0.7.2"
```

## Tests to Write

- Test that LED blinks at 2Hz (visual verification)
- Test that frame counter increments (can query via serial in next phase)
- Test message handling (will be verified in integration tests)

## Validate

Run from `lp-fw/fw-esp32/` directory:

```bash
cd lp-fw/fw-esp32
cargo check --package fw-esp32 --features test_usb,esp32c6 --target riscv32imac-unknown-none-elf
```

Ensure:
- Code compiles without warnings
- Main loop structure is correct (blink → handle → increment)
- Frame counter is atomic and thread-safe
- MessageRouter is used correctly
- LED blinking logic is separate from message handling

Note: Full functionality will be verified in next phase when I/O task is implemented.
