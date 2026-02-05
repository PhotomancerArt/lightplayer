//! ESP32 USB-serial SerialIo implementation
//!
//! Uses ESP32's native USB-serial for communication with the host.
//! This is not a hardware UART, but the USB-serial interface.

extern crate alloc;

use alloc::format;
use esp_hal::{
    Async,
    usb_serial_jtag::{UsbSerialJtag, UsbSerialJtagRx, UsbSerialJtagTx},
};
use fw_core::serial::{SerialError, SerialIo};

/// ESP32 USB-serial SerialIo implementation
///
/// Bridges async USB serial to synchronous SerialIo trait.
pub struct Esp32UsbSerialIo {
    rx: UsbSerialJtagRx<'static, Async>,
    tx: UsbSerialJtagTx<'static, Async>,
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

    /// Get mutable references to rx/tx for direct async operations
    /// This allows bypassing the SerialIo trait's block_on when already in async context
    #[allow(dead_code)]
    pub fn get_async_parts(
        &mut self,
    ) -> (
        &mut UsbSerialJtagRx<'static, Async>,
        &mut UsbSerialJtagTx<'static, Async>,
    ) {
        (&mut self.rx, &mut self.tx)
    }
}

impl SerialIo for Esp32UsbSerialIo {
    fn write(&mut self, data: &[u8]) -> Result<(), SerialError> {
        // Blocking write using async USB serial
        // Use embassy_futures::block_on to bridge async to sync
        embassy_futures::block_on(async {
            embedded_io_async::Write::write(&mut self.tx, data)
                .await
                .map_err(|e| {
                    SerialError::WriteFailed(format!("USB-serial write error: {:?}", e))
                })?;
            embedded_io_async::Write::flush(&mut self.tx)
                .await
                .map_err(|e| {
                    SerialError::WriteFailed(format!("USB-serial flush error: {:?}", e))
                })?;
            Ok(())
        })
    }

    fn read_available(&mut self, buf: &mut [u8]) -> Result<usize, SerialError> {
        // Non-blocking read - check available data and read what's there
        // Use async read with timeout to make it non-blocking

        if buf.is_empty() {
            return Ok(0);
        }

        // Try to read with a zero timeout to make it non-blocking
        embassy_futures::block_on(async {
            use embassy_time::{Duration, Timer};

            // Use a very short timeout to make it non-blocking
            // If data is available, it should read immediately
            match embassy_futures::select::select(
                Timer::after(Duration::from_millis(0)),
                embedded_io_async::Read::read(&mut self.rx, buf),
            )
            .await
            {
                embassy_futures::select::Either::First(_) => {
                    // Timeout - no data available
                    Ok(0)
                }
                embassy_futures::select::Either::Second(result) => result.map_err(|e| {
                    SerialError::ReadFailed(format!("USB-serial read error: {:?}", e))
                }),
            }
        })
    }

    fn has_data(&self) -> bool {
        // Check if USB-serial has data available
        // For now, we can't easily check without mutable access
        // The default implementation returns true, and read_available will return 0 if no data
        // This is acceptable - SerialIo trait allows this optimization hint
        true
    }
}
