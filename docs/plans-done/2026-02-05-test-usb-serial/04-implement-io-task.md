# Phase 4: Implement I/O Task with Serial State Management

## Scope of phase

Create the I/O task that handles serial communication, drains message queues, manages serial state (Ready/Disconnected/Error), and filters messages with `M!` prefix.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Add serial state management

Add to `lp-fw/fw-esp32/src/tests/test_usb.rs`:

```rust
use embedded_io_async::{Read, Write};

/// Serial connection state
#[derive(Debug, Clone, Copy, PartialEq)]
enum SerialState {
    /// Serial not yet initialized
    Uninitialized,
    /// Serial ready and working
    Ready,
    /// Serial disconnected (will retry)
    Disconnected,
    /// Serial error (will retry)
    Error,
}

/// I/O task for handling serial communication
///
/// Responsibilities:
/// - Drain outgoing queue and send via serial
/// - Read from serial and push to incoming queue (filter M! prefix)
/// - Handle serial state (Ready/Disconnected/Error)
/// - Retry serial initialization if disconnected
#[embassy_executor::task]
async fn io_task(
    router: MessageRouter,
    usb_device: esp_hal::usb_serial_jtag::UsbSerialJtagDevice,
) {
    let mut serial_state = SerialState::Uninitialized;
    let mut read_buffer = Vec::new();
    let mut line_buffer = String::new();
    
    loop {
        match serial_state {
            SerialState::Uninitialized | SerialState::Disconnected | SerialState::Error => {
                // Try to initialize serial
                match try_init_serial(usb_device) {
                    Ok((rx, tx)) => {
                        serial_state = SerialState::Ready;
                        // Continue with serial operations
                        handle_serial_io(rx, tx, &router, &mut read_buffer, &mut line_buffer).await;
                    }
                    Err(_) => {
                        // Initialization failed - wait and retry
                        serial_state = SerialState::Disconnected;
                        Timer::after(Duration::from_millis(100)).await;
                    }
                }
            }
            SerialState::Ready => {
                // Serial is ready - handle I/O
                // This will be implemented in handle_serial_io
                Timer::after(Duration::from_millis(1)).await;
            }
        }
    }
}

/// Try to initialize USB serial
///
/// Returns split async serial if successful.
fn try_init_serial(
    usb_device: esp_hal::usb_serial_jtag::UsbSerialJtagDevice,
) -> Result<
    (
        impl Read,
        impl Write,
    ),
    (),
> {
    // Create USB serial
    let usb_serial = UsbSerialJtag::new(usb_device);
    let usb_serial_async = usb_serial.into_async();
    let (rx, tx) = usb_serial_async.split();
    Ok((rx, tx))
}

/// Handle serial I/O operations
///
/// Drains outgoing queue, reads from serial, manages state.
async fn handle_serial_io(
    mut rx: impl Read,
    mut tx: impl Write,
    router: &MessageRouter,
    read_buffer: &mut Vec<u8>,
    line_buffer: &mut String,
) {
    // Drain outgoing queue and send via serial
    let outgoing = router.outgoing();
    let sender = outgoing.receiver();
    
    loop {
        // Try to receive message from outgoing queue
        match sender.try_receive() {
            Ok(msg) => {
                // Send via serial
                if let Err(_) = tx.write(msg.as_bytes()).await {
                    // Write error - mark as disconnected
                    return;
                }
            }
            Err(_) => {
                // No messages - continue to read
                break;
            }
        }
    }
    
    // Read from serial (non-blocking with timeout)
    let mut temp_buf = [0u8; 64];
    match embassy_futures::select::select(
        Timer::after(Duration::from_millis(1)),
        Read::read(&mut rx, &mut temp_buf),
    )
    .await
    {
        embassy_futures::select::Either::Second(Ok(n)) if n > 0 => {
            // Append to read buffer
            read_buffer.extend_from_slice(&temp_buf[..n]);
            
            // Process complete lines
            process_read_buffer(read_buffer, line_buffer, router);
        }
        embassy_futures::select::Either::Second(Err(_)) => {
            // Read error - mark as disconnected
            return;
        }
        _ => {
            // Timeout or no data - continue
        }
    }
}

/// Process read buffer and extract complete lines
///
/// Looks for newlines, extracts lines starting with `M!`, and pushes to incoming queue.
fn process_read_buffer(
    read_buffer: &mut Vec<u8>,
    line_buffer: &mut String,
    router: &MessageRouter,
) {
    // Find newlines and process complete lines
    while let Some(newline_pos) = read_buffer.iter().position(|&b| b == b'\n') {
        // Extract line (including newline)
        let line_bytes: Vec<u8> = read_buffer.drain(..=newline_pos).collect();
        
        // Convert to string
        if let Ok(line_str) = core::str::from_utf8(&line_bytes[..line_bytes.len() - 1]) {
            // Check for M! prefix
            if line_str.starts_with("M!") {
                // Push to incoming queue
                let incoming = router.incoming();
                if let Err(_) = incoming.sender().try_send(line_str.to_string()) {
                    // Queue full - drop message (or could implement drop oldest)
                    #[cfg(feature = "esp32c6")]
                    log::warn!("Incoming queue full, dropping message");
                }
            }
            // Non-M! lines are ignored (debug output, etc.)
        }
        
        // Clear line buffer for next line
        line_buffer.clear();
    }
}
```

### 2. Update run_usb_test to spawn I/O task

Update `run_usb_test` in `lp-fw/fw-esp32/src/tests/test_usb.rs`:

```rust
pub async fn run_usb_test(spawner: embassy_executor::Spawner) -> ! {
    // ... existing initialization code ...
    
    // Create message router
    let router = MessageRouter::new(&INCOMING_MSG, &OUTGOING_MSG);
    
    // Spawn I/O task
    spawner.spawn(io_task(router, usb_device)).ok();
    
    // ... rest of main loop ...
}
```

### 3. Update main.rs to pass spawner

Update `lp-fw/fw-esp32/src/main.rs`:

```rust
#[cfg(feature = "test_usb")]
{
    use tests::test_usb::run_usb_test;
    run_usb_test(spawner).await;
}
```

### 4. Add embassy-futures dependency

Ensure `embassy-futures` is in `Cargo.toml` (should already be there):
```toml
embassy-futures = "0.1"
```

## Tests to Write

- Test that outgoing messages are sent via serial
- Test that incoming messages with `M!` prefix are queued
- Test that non-`M!` lines are ignored
- Test serial state transitions (Uninitialized → Ready → Disconnected → Ready)
- Test that serial errors are handled gracefully

## Validate

Run from `lp-fw/fw-esp32/` directory:

```bash
cd lp-fw/fw-esp32
cargo check --package fw-esp32 --features test_usb,esp32c6 --target riscv32imac-unknown-none-elf
```

Ensure:
- Code compiles without warnings
- I/O task is spawned correctly
- Serial state management works
- Message filtering (`M!` prefix) works
- Queue operations are non-blocking
- Error handling is graceful (doesn't panic)

Note: Full integration testing will be done in next phase with host-side tests.
