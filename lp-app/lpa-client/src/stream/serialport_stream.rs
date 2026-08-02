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

    fn write_all(&mut self, bytes: &[u8]) -> Result<(), ByteStreamError> {
        self.port
            .write_all(bytes)
            .map_err(|error| ByteStreamError::io(error.to_string()))?;
        self.port
            .flush()
            .map_err(|error| ByteStreamError::io(error.to_string()))
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
