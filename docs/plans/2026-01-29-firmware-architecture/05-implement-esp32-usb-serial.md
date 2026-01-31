# Phase 5: Implement ESP32 USB-serial SerialIo

## Scope of phase

Implement the USB-serial `SerialIo` for ESP32. This uses ESP32's native USB-serial (not hardware UART) for communication with the host.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Update serial/mod.rs

```rust
#[cfg(feature = "esp32c6")]
pub mod usb_serial;

#[cfg(feature = "esp32c6")]
pub use usb_serial::Esp32UsbSerialIo;
```

### 2. Create serial/usb_serial.rs

Implement USB-serial `SerialIo`:

```rust
//! ESP32 USB-serial SerialIo implementation
//!
//! Uses ESP32's native USB-serial for communication with the host.
//! This is not a hardware UART, but the USB-serial interface.

use esp_hal::usb_serial_jtag::UsbSerialJtag;
use fw_core::serial::{SerialError, SerialIo};

/// ESP32 USB-serial SerialIo implementation
pub struct Esp32UsbSerialIo {
    usb_serial: UsbSerialJtag<'static>,
}

impl Esp32UsbSerialIo {
    /// Create a new USB-serial SerialIo instance
    ///
    /// # Arguments
    /// * `usb_serial` - Initialized USB-serial interface
    pub fn new(usb_serial: UsbSerialJtag<'static>) -> Self {
        Self { usb_serial }
    }
}

impl SerialIo for Esp32UsbSerialIo {
    fn write(&mut self, data: &[u8]) -> Result<(), SerialError> {
        // Blocking write - esp-hal USB-serial has blocking write methods
        // Write in chunks if needed
        for chunk in data.chunks(64) {
            self.usb_serial.write_bytes(chunk)
                .map_err(|e| SerialError::WriteFailed(format!("USB-serial write error: {:?}", e)))?;
        }
        Ok(())
    }

    fn read_available(&mut self, buf: &mut [u8]) -> Result<usize, SerialError> {
        // Non-blocking read - check if data available, read what's there
        let available = self.usb_serial.read_available();
        if available == 0 {
            return Ok(0);
        }

        let to_read = available.min(buf.len());
        self.usb_serial.read_bytes(&mut buf[..to_read])
            .map_err(|e| SerialError::ReadFailed(format!("USB-serial read error: {:?}", e)))?;
        Ok(to_read)
    }

    fn has_data(&self) -> bool {
        // Check if USB-serial has data available
        self.usb_serial.read_available() > 0
    }
}
```

### 3. Update main.rs

Add USB-serial initialization (stub for now, will be integrated in later phase):

```rust
// ... existing code ...

use board::init_board;
use serial::Esp32UsbSerialIo;

#[esp_rtos::main]
async fn main(_spawner: embassy_executor::Spawner) {
    esp_println::logger::init_logger_from_env();

    println!("fw-esp32 starting...");

    let (peripherals, sw_int, timg0) = init_board();
    board::start_runtime(timg0.timer0, sw_int.software_interrupt0);

    // TODO: Initialize USB-serial
    // let usb_serial = UsbSerialJtag::new(peripherals.USB_SERIAL_JTAG);
    // let serial_io = Esp32UsbSerialIo::new(usb_serial);

    println!("fw-esp32 initialized (stub)");
}
```

## Notes

- USB-serial initialization may require specific setup - reference `esp32-glsl-jit` prototype for patterns
- The exact API for `UsbSerialJtag` may vary - adjust based on actual esp-hal API
- Chunk size (64 bytes) is a reasonable default, but may need adjustment

## Validate

Run from `lp-app/` directory:

```bash
cd lp-app
cargo check --package fw-esp32 --features esp32c6
```

Ensure:

- USB-serial SerialIo compiles
- Implements SerialIo trait correctly
- No warnings (except for TODO stubs)

Note: Full compilation may require ESP32 toolchain setup, but structure should be valid.
