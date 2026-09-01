//! ESP32 USB-serial SerialIo implementation
//!
//! Uses ESP32's native USB-serial for communication with the host.
//! This is not a hardware UART, but the USB-serial interface.
//!
//! Synchronous, **best-effort** writes: USB-Serial-JTAG output only leaves the
//! chip when a host reads the port, so a write is bounded by [`DRAIN_TIMEOUT`]
//! and dropped — never waited on indefinitely — when nothing is draining.
//! esp-hal's blocking `UsbSerialJtag::write` spins forever on an unread
//! endpoint buffer, which wedged every harness that logged before its first
//! frame of real work (docs/defects/2026-08-31-c6-rmt-ws281x-dark.md: test_rmt
//! sat "dark" in that spin until any process opened the port). The app path is
//! immune — its logs ride `io_task`'s `ChunkedWriter`, which already bounds
//! and latches — so this type owns the same policy for the harnesses.

use esp_hal::time::{Duration, Instant};
use esp_hal::{Blocking, usb_serial_jtag::UsbSerialJtag};
use fw_core::serial::{SerialError, SerialIo};

/// How long one write will wait for the host to drain the 64-byte endpoint
/// buffer before latching not-draining and dropping output.
///
/// Mirrors `WritePolicy::USB_SERIAL_JTAG`'s 250 ms chunk timeout: a healthy
/// full-speed host drains the buffer in well under a millisecond, so reaching
/// this deadline means nobody is reading, not that the host is slow.
#[allow(dead_code, reason = "used by the harness entry points, like the type itself")]
const DRAIN_TIMEOUT: Duration = Duration::from_millis(250);

/// ESP32 USB-serial SerialIo implementation
///
/// Uses synchronous USB serial operations directly.
#[allow(dead_code, reason = "public API reserved for future use")]
pub struct Esp32UsbSerialIo {
    usb_serial: UsbSerialJtag<'static, Blocking>,
    /// Whether the last write found a host draining the endpoint buffer.
    /// While false, writes probe with their first byte and drop the rest
    /// instead of paying [`DRAIN_TIMEOUT`] per log line.
    host_draining: bool,
}

impl Esp32UsbSerialIo {
    /// Create a new USB-serial SerialIo instance
    ///
    /// # Arguments
    /// * `usb_serial` - Initialized USB-serial interface (synchronous/blocking)
    #[allow(dead_code, reason = "public API reserved for future use")]
    pub fn new(usb_serial: UsbSerialJtag<'static, Blocking>) -> Self {
        Self {
            usb_serial,
            // Optimistic: the first write pays at most one DRAIN_TIMEOUT to
            // find out, then the latch keeps every later write cheap.
            host_draining: true,
        }
    }
}

impl SerialIo for Esp32UsbSerialIo {
    /// Best-effort write: bytes are dropped, not queued, when no host drains
    /// the port. `Ok(())` therefore means "handed to the peripheral or
    /// deliberately dropped", never "delivered".
    fn write(&mut self, data: &[u8]) -> Result<(), SerialError> {
        let mut bytes = data.iter().copied();

        if !self.host_draining {
            // Latched: probe with the first real byte. Room in the endpoint
            // buffer means something drained it since we latched — a host is
            // back. No room: drop the write without spinning, so a harness
            // logging into an unread port never stalls the work it is
            // instrumenting.
            let Some(first) = bytes.next() else {
                return Ok(());
            };
            if self.usb_serial.write_byte_nb(first).is_err() {
                return Ok(());
            }
            self.host_draining = true;
        }

        let started = Instant::now();
        for byte in bytes {
            // The only realistic error is WouldBlock (endpoint buffer full);
            // treat every Err as "no room yet", bounded by the deadline.
            while self.usb_serial.write_byte_nb(byte).is_err() {
                if started.elapsed() > DRAIN_TIMEOUT {
                    self.host_draining = false;
                    return Ok(());
                }
            }
        }
        // Push a partial trailing chunk to the host (full 64-byte chunks
        // auto-flush). Fire-and-forget: whether it has left by now is the
        // next write's problem, not a reason to spin here.
        let _ = self.usb_serial.flush_tx_nb();
        Ok(())
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
