//! [`DeviceByteStream`] implementation over a native `serialport` port.

use std::time::Duration;

use crate::stream::{ByteStreamError, DeviceByteStream};

/// A native OS serial port as a [`DeviceByteStream`].
///
/// Opening (and reopening) uses the exact settings the hardware transport
/// always used: 8N1, no flow control, 100 ms read timeout. The 100 ms read
/// timeout is what turns blocking reads into the `Ok(0)`-style polling the
/// transport thread expects.
pub struct SerialPortByteStream {
    port_name: String,
    port: Box<dyn serialport::SerialPort>,
    /// Raw fd of the open port, for the whole-status modem-line ioctl (see
    /// [`DeviceByteStream::set_signals`] below). Captured at open/reopen —
    /// the boxed `SerialPort` trait object does not expose it.
    #[cfg(unix)]
    raw_fd: std::os::fd::RawFd,
}

impl SerialPortByteStream {
    /// Open `port_name` at `baud_rate`.
    pub fn open(port_name: &str, baud_rate: u32) -> Result<Self, ByteStreamError> {
        #[cfg(unix)]
        {
            let port = open_serial_port_native(port_name, baud_rate)?;
            let raw_fd = std::os::fd::AsRawFd::as_raw_fd(&port);
            Ok(Self {
                port_name: port_name.to_string(),
                port: Box::new(port),
                raw_fd,
            })
        }
        #[cfg(not(unix))]
        {
            let port = open_serial_port(port_name, baud_rate)?;
            Ok(Self {
                port_name: port_name.to_string(),
                port,
            })
        }
    }

    /// The OS port name this stream was opened on.
    pub fn port_name(&self) -> &str {
        &self.port_name
    }

    /// Set DTR and RTS in ONE `TIOCMSET` whole-status write.
    ///
    /// Load-bearing, not an optimization: the WCH CH34x macOS driver (the
    /// DOM-Z-102's CH340K bridge) silently ignores the single-bit
    /// `TIOCMBIS`/`TIOCMBIC` ioctls behind `write_data_terminal_ready` /
    /// `write_request_to_send`, while honoring `TIOCMSET` — verified on
    /// hardware (classic bring-up M3; a reset dance through the per-line
    /// calls never reset the chip, the identical sequence through `TIOCMSET`
    /// did). This is also why espflash carries `UnixTightReset` alongside
    /// its per-line `ClassicReset`.
    #[cfg(unix)]
    fn set_signals_whole_status(&mut self, dtr: bool, rts: bool) -> Result<(), ByteStreamError> {
        let fd = self.raw_fd;
        let mut status: libc::c_int = 0;
        if unsafe { libc::ioctl(fd, libc::TIOCMGET, &mut status) } != 0 {
            return Err(ByteStreamError::io(format!(
                "TIOCMGET on {}: {}",
                self.port_name,
                std::io::Error::last_os_error()
            )));
        }
        if dtr {
            status |= libc::TIOCM_DTR;
        } else {
            status &= !libc::TIOCM_DTR;
        }
        if rts {
            status |= libc::TIOCM_RTS;
        } else {
            status &= !libc::TIOCM_RTS;
        }
        if unsafe { libc::ioctl(fd, libc::TIOCMSET, &status) } != 0 {
            return Err(ByteStreamError::io(format!(
                "TIOCMSET on {}: {}",
                self.port_name,
                std::io::Error::last_os_error()
            )));
        }
        Ok(())
    }
}

impl DeviceByteStream for SerialPortByteStream {
    fn read_available(&mut self, buf: &mut [u8]) -> Result<usize, ByteStreamError> {
        match self.port.read(buf) {
            Ok(n) => Ok(n),
            // The 100 ms port timeout expires with no data: not an error,
            // just "nothing right now".
            Err(error) if error.kind() == std::io::ErrorKind::TimedOut => Ok(0),
            Err(error) => Err(ByteStreamError::io(error.to_string())),
        }
    }

    /// Hand `bytes` to the kernel's output queue. Deliberately does NOT
    /// `flush()`.
    ///
    /// `serialport`'s `flush` is `tcdrain(fd)`: "block until every queued
    /// byte has been transmitted", with no timeout (the port timeout there
    /// only bounds `EINTR` retries). A device that stops draining its
    /// receive FIFO — a C6 hung in its bootloader is the case that found
    /// this — backs the queue up and turns that into a permanent block. The
    /// framing thread that owns this stream then never reaches its shutdown
    /// check, `ClientTransport::close` gives up at its join budget, and the
    /// OS port stays open for the life of the process
    /// (`docs/defects/2026-09-08-serial-close-leaks-the-port-on-a-wedged-device.md`).
    ///
    /// Nothing is lost by skipping it: `write` has already handed the bytes
    /// to the driver, which transmits them on its own schedule, and no
    /// caller here needs "already on the wire" semantics — the only
    /// operation that would care (the reset dance, which cuts transmission
    /// short) runs before the first write. `write` itself stays bounded: it
    /// waits for writability under the port's 100 ms timeout, and a device
    /// that has stopped draining fills the kernel's output queue and trips
    /// that timeout — reported as [`ByteStreamError::WriteStalled`], a
    /// dropped frame rather than a dead stream, because a board that stopped
    /// reading is `Unresponsive`, not gone.
    fn write_all(&mut self, bytes: &[u8]) -> Result<(), ByteStreamError> {
        self.port.write_all(bytes).map_err(|error| {
            if error.kind() == std::io::ErrorKind::TimedOut {
                ByteStreamError::WriteStalled
            } else {
                ByteStreamError::io(error.to_string())
            }
        })
    }

    fn set_signals(&mut self, dtr: Option<bool>, rts: Option<bool>) -> Result<(), ByteStreamError> {
        // Both lines at once → one whole-status write, where supported (see
        // set_signals_whole_status for the driver that requires it).
        // Single-line writes keep the per-line calls: the USB-Serial-JTAG
        // dance depends on their exact pin-write sequence.
        #[cfg(unix)]
        if let (Some(dtr), Some(rts)) = (dtr, rts) {
            return self.set_signals_whole_status(dtr, rts);
        }
        if let Some(dtr) = dtr {
            self.port
                .write_data_terminal_ready(dtr)
                .map_err(|error| ByteStreamError::io(error.to_string()))?;
        }
        if let Some(rts) = rts {
            self.port
                .write_request_to_send(rts)
                .map_err(|error| ByteStreamError::io(error.to_string()))?;
        }
        Ok(())
    }

    fn reopen(&mut self, baud_rate: u32) -> Result<(), ByteStreamError> {
        // The old port is closed by the assignment below, and a tty close
        // DRAINS — same hazard the `Drop` impl exists for, reached by a
        // different route.
        let _ = self.port.clear(serialport::ClearBuffer::Output);
        #[cfg(unix)]
        {
            let reopened = open_serial_port_native(&self.port_name, baud_rate)?;
            self.raw_fd = std::os::fd::AsRawFd::as_raw_fd(&reopened);
            self.port = Box::new(reopened);
        }
        #[cfg(not(unix))]
        {
            self.port = open_serial_port(&self.port_name, baud_rate)?;
        }
        Ok(())
    }
}

impl Drop for SerialPortByteStream {
    /// Throw away queued output before the fd closes.
    ///
    /// Load-bearing, and the second half of the same lesson as
    /// [`Self::write_all`]: on macOS `close(2)` of a tty DRAINS. The closing
    /// thread blocks until the output queue empties, and a device that has
    /// stopped reading never lets it — so the close never returns, the port
    /// stays held, and the next `open()` of it blocks too. Seen exactly that
    /// way on the bench (2026-09-08, C6 hung in its bootloader): a `sample`
    /// of the wedged process showed the framing thread parked in `close`
    /// inside this very drop while the main thread sat in `open` on the same
    /// port. `tcflush(TCOFLUSH)` leaves the close nothing to wait for.
    ///
    /// Discarding unsent bytes is right *here* and only here. The stream is
    /// being torn down: whatever is still queued is a frame nobody is waiting
    /// on any more (a close abandons the write backlog by design), the peer
    /// gets reset or reflashed next, and the link that follows is a fresh
    /// one. The alternative is not "the bytes arrive" — the peer is not
    /// reading them — it is a thread that never comes back.
    fn drop(&mut self) {
        let _ = self.port.clear(serialport::ClearBuffer::Output);
    }
}

/// The transport's standard port settings.
fn port_builder(port_name: &str, baud_rate: u32) -> serialport::SerialPortBuilder {
    serialport::new(port_name, baud_rate)
        .data_bits(serialport::DataBits::Eight)
        .stop_bits(serialport::StopBits::One)
        .parity(serialport::Parity::None)
        .flow_control(serialport::FlowControl::None)
        .timeout(Duration::from_millis(100))
}

/// Open a serial port as the platform-native type (which exposes the raw fd).
#[cfg(unix)]
fn open_serial_port_native(
    port_name: &str,
    baud_rate: u32,
) -> Result<serialport::TTYPort, ByteStreamError> {
    port_builder(port_name, baud_rate)
        .open_native()
        .map_err(|error| {
            ByteStreamError::io(format!("Failed to open serial port {port_name}: {error}"))
        })
}

/// Open a serial port with the transport's standard settings.
#[cfg(not(unix))]
fn open_serial_port(
    port_name: &str,
    baud_rate: u32,
) -> Result<Box<dyn serialport::SerialPort>, ByteStreamError> {
    port_builder(port_name, baud_rate).open().map_err(|error| {
        ByteStreamError::io(format!("Failed to open serial port {port_name}: {error}"))
    })
}
