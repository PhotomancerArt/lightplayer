# Phase 3: Implement USB Serial SerialIo

## Scope of phase

Complete the USB serial SerialIo implementation. Bridge async USB serial to the synchronous SerialIo trait.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Update serial/usb_serial.rs

Complete the implementation. The challenge is bridging async USB serial to synchronous SerialIo trait.

Key considerations:
- USB serial uses Async driver mode
- SerialIo trait methods are synchronous
- We need to use `block_on` or similar to bridge async to sync
- For reads, we can check available data without blocking
- For writes, we may need to block until complete

```rust
//! ESP32 USB-serial SerialIo implementation
//!
//! Uses ESP32's native USB-serial for communication with the host.
//! This is not a hardware UART, but the USB-serial interface.

use esp_hal::{Async, usb_serial_jtag::UsbSerialJtag};
use fw_core::serial::{SerialError, SerialIo};

/// ESP32 USB-serial SerialIo implementation
///
/// Bridges async USB serial to synchronous SerialIo trait.
pub struct Esp32UsbSerialIo {
    rx: UsbSerialJtag<'static, Async>,
    tx: UsbSerialJtag<'static, Async>,
}

impl Esp32UsbSerialIo {
    /// Create a new USB-serial SerialIo instance
    ///
    /// # Arguments
    /// * `usb_serial` - Initialized USB-serial interface (will be split into rx/tx)
    pub fn new(usb_serial: UsbSerialJtag<'static, Async>) -> Self {
        // Split USB serial into rx/tx halves for async operations
        let (rx, tx) = usb_serial.split();
        Self { rx, tx }
    }
}

impl SerialIo for Esp32UsbSerialIo {
    fn write(&mut self, data: &[u8]) -> Result<(), SerialError> {
        // Blocking write using async USB serial
        // Use embassy executor to block on async write
        // Note: This must be called from async context or we need block_on
        
        // For now, we'll use a blocking approach
        // TODO: Investigate if esp-hal USB serial has blocking methods
        // If not, we may need to use embassy executor's block_on
        
        // Check esp-hal API - USB serial Async may have blocking methods
        // If not, we'll need to use async/await and block_on
        
        // Placeholder implementation
        // This will need to be adapted based on actual esp-hal API
        embassy_futures::block_on(async {
            self.tx.write(data).await
                .map_err(|e| SerialError::WriteFailed(format!("USB-serial write error: {:?}", e)))
        })
    }

    fn read_available(&mut self, buf: &mut [u8]) -> Result<usize, SerialError> {
        // Non-blocking read - check available data and read what's there
        // Use async read with timeout or check available first
        
        // Check if data is available
        if !self.has_data() {
            return Ok(0);
        }

        // Read available data (non-blocking)
        embassy_futures::block_on(async {
            let read_future = self.rx.read(buf);
            // Use timeout to make it non-blocking
            match embassy_time::with_timeout(embassy_time::Duration::from_millis(0), read_future).await {
                Ok(Ok(n)) => Ok(n),
                Ok(Err(e)) => Err(SerialError::ReadFailed(format!("USB-serial read error: {:?}", e))),
                Err(_) => Ok(0), // Timeout - no data available
            }
        })
    }

    fn has_data(&self) -> bool {
        // Check if USB-serial has data available
        // This may need to check the USB serial status
        // For now, we'll use a simple approach - try to peek
        
        // Note: esp-hal USB serial Async may have a method to check available
        // If not, we may need to use a different approach
        
        // Placeholder - will need to check actual API
        // May need to use unsafe or check internal state
        false // TODO: Implement actual check
    }
}
```

**Note**: The actual implementation will depend on the esp-hal USB serial Async API. We may need to:
1. Check if there are blocking methods available
2. Use `embassy_futures::block_on` to bridge async to sync
3. Use timeouts for non-blocking reads
4. Check internal state for `has_data()`

### 2. Update serial/mod.rs

```rust
#[cfg(feature = "esp32c6")]
pub mod usb_serial;

#[cfg(feature = "esp32c6")]
pub use usb_serial::Esp32UsbSerialIo;
```

### 3. Update Cargo.toml

Add `embassy-futures` dependency if needed:

```toml
[dependencies]
# ... existing dependencies ...
embassy-futures = "0.1"
```

## Notes

- The exact API for esp-hal USB serial Async may vary - adjust based on actual API
- We may need to use `embassy_futures::block_on` to bridge async to sync
- For `has_data()`, we may need to check internal USB serial state
- If blocking methods are available, prefer those for simplicity

## Validate

Run:
```bash
cd lp-fw/fw-esp32
cargo check --features esp32c6
```

Expected: Code compiles. The implementation may need adjustment based on actual esp-hal API.
