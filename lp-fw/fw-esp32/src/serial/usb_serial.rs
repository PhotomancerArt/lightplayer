//! ESP32 USB-serial SerialIo implementation
//!
//! Uses ESP32's native USB-serial for communication with the host.
//! This is not a hardware UART, but the USB-serial interface.
//!
//! Uses synchronous (blocking) operations for simplicity.

extern crate alloc;

use alloc::format;
use esp_hal::{Blocking, usb_serial_jtag::UsbSerialJtag};
use fw_core::serial::{SerialError, SerialIo};

/// ESP32 USB-serial SerialIo implementation
///
/// Uses synchronous USB serial operations directly.
pub struct Esp32UsbSerialIo {
    usb_serial: UsbSerialJtag<'static, Blocking>,
}

impl Esp32UsbSerialIo {
    /// Create a new USB-serial SerialIo instance
    ///
    /// # Arguments
    /// * `usb_serial` - Initialized USB-serial interface (synchronous/blocking)
    pub fn new(usb_serial: UsbSerialJtag<'static, Blocking>) -> Self {
        Self { usb_serial }
    }
}

impl SerialIo for Esp32UsbSerialIo {
    fn write(&mut self, data: &[u8]) -> Result<(), SerialError> {
        self.usb_serial
            .write(data)
            .map_err(|e| SerialError::WriteFailed(format!("USB-serial write error: {:?}", e)))
    }

    fn read_available(&mut self, buf: &mut [u8]) -> Result<usize, SerialError> {
        if buf.is_empty() {
            return Ok(0);
        }

        // Read bytes one at a time - read_byte() returns nb::Error::WouldBlock when no data
        // Since we're in blocking mode but want non-blocking behavior, we just break on any error
        let mut count = 0;
        for byte_slot in buf.iter_mut() {
            match self.usb_serial.read_byte() {
                Ok(byte) => {
                    *byte_slot = byte;
                    count += 1;
                }
                Err(_) => {
                    // WouldBlock or other error - no more data available
                    break;
                }
            }
        }

        Ok(count)
    }

    fn has_data(&self) -> bool {
        // We can't easily check without mutable access
        // The default implementation returns true, and read_available will return 0 if no data
        true
    }
}
