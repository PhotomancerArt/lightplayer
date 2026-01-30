//! ESP32 USB-serial SerialIo implementation
//!
//! Uses ESP32's native USB-serial for communication with the host.
//! This is not a hardware UART, but the USB-serial interface.
//!
//! Note: This implementation uses the Async driver mode. The SerialIo trait
//! methods will need to be called from async context or wrapped appropriately.

use esp_hal::{Async, usb_serial_jtag::UsbSerialJtag};
use fw_core::serial::{SerialError, SerialIo};

/// ESP32 USB-serial SerialIo implementation
///
/// Uses Async driver mode. The SerialIo methods should be called from
/// async context or wrapped to handle async operations.
pub struct Esp32UsbSerialIo {
    // Store the USB serial in a way that allows both read and write
    // For now, we'll use a split approach when needed
    _marker: core::marker::PhantomData<UsbSerialJtag<'static, Async>>,
}

impl Esp32UsbSerialIo {
    /// Create a new USB-serial SerialIo instance
    ///
    /// # Arguments
    /// * `usb_serial` - Initialized USB-serial interface
    ///
    /// Note: The USB serial will need to be split into rx/tx for async operations.
    /// This is a placeholder implementation - actual usage will be determined
    /// when integrating with the server loop.
    #[allow(
        dead_code,
        reason = "Placeholder function for future async integration"
    )]
    pub fn new(_usb_serial: UsbSerialJtag<'static, Async>) -> Self {
        Self {
            _marker: core::marker::PhantomData,
        }
    }
}

impl SerialIo for Esp32UsbSerialIo {
    fn write(&mut self, _data: &[u8]) -> Result<(), SerialError> {
        // TODO: Implement blocking write using async USB serial
        // This will need to be integrated with the async runtime
        // For now, return an error to indicate it's not yet implemented
        Err(SerialError::WriteFailed(
            "USB-serial write not yet implemented - needs async integration".into(),
        ))
    }

    fn read_available(&mut self, _buf: &mut [u8]) -> Result<usize, SerialError> {
        // TODO: Implement non-blocking read using async USB serial
        // This will need to be integrated with the async runtime
        // For now, return 0 to indicate no data available
        Ok(0)
    }

    fn has_data(&self) -> bool {
        // TODO: Check if USB-serial has data available
        // This will need async integration
        false
    }
}
