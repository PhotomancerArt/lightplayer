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
}

impl SerialPortByteStream {
    /// Open `port_name` at `baud_rate`.
    pub fn open(port_name: &str, baud_rate: u32) -> Result<Self, ByteStreamError> {
        let port = open_serial_port(port_name, baud_rate)?;
        Ok(Self {
            port_name: port_name.to_string(),
            port,
        })
    }

    /// The OS port name this stream was opened on.
    pub fn port_name(&self) -> &str {
        &self.port_name
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
        let reopened = open_serial_port(&self.port_name, baud_rate)?;
        self.port = reopened;
        Ok(())
    }
}

/// Open a serial port with the transport's standard settings.
fn open_serial_port(
    port_name: &str,
    baud_rate: u32,
) -> Result<Box<dyn serialport::SerialPort>, ByteStreamError> {
    serialport::new(port_name, baud_rate)
        .data_bits(serialport::DataBits::Eight)
        .stop_bits(serialport::StopBits::One)
        .parity(serialport::Parity::None)
        .flow_control(serialport::FlowControl::None)
        .timeout(Duration::from_millis(100))
        .open()
        .map_err(|error| {
            ByteStreamError::io(format!("Failed to open serial port {port_name}: {error}"))
        })
}
