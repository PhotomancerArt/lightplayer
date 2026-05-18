use anyhow::{Context, Result, bail};
use serialport::SerialPort;
use std::io::{Read, Write};
use std::time::{Duration, Instant};

use crate::client::serial_port::detect_serial_port;

pub struct SerialCalibrationTransport {
    port_name: String,
    baud_rate: u32,
    port: Box<dyn SerialPort>,
    line: Vec<u8>,
}

impl SerialCalibrationTransport {
    pub fn open(port_spec: Option<&str>, timeout: Duration) -> Result<Self> {
        let normalized = port_spec.map(|port| port.strip_prefix("serial:").unwrap_or(port));
        let config = detect_serial_port(normalized, None)?;
        let port = open_port(&config.port, config.baud_rate, timeout)?;
        Ok(Self {
            port_name: config.port,
            baud_rate: config.baud_rate,
            port,
            line: Vec::new(),
        })
    }

    pub fn send_line(&mut self, line: &str) -> Result<()> {
        self.port.write_all(line.as_bytes())?;
        self.port.write_all(b"\n")?;
        self.port.flush()?;
        Ok(())
    }

    pub fn read_line_until(&mut self, timeout: Duration) -> Result<Option<String>> {
        let deadline = Instant::now() + timeout;
        let mut buf = [0u8; 64];
        while Instant::now() < deadline {
            match self.port.read(&mut buf) {
                Ok(0) => {}
                Ok(count) => {
                    for byte in &buf[..count] {
                        if *byte == b'\n' || *byte == b'\r' {
                            if self.line.is_empty() {
                                continue;
                            }
                            let line = String::from_utf8_lossy(&self.line).to_string();
                            self.line.clear();
                            return Ok(Some(line));
                        }
                        self.line.push(*byte);
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::TimedOut => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(None)
    }

    pub fn reconnect(&mut self, timeout: Duration) -> Result<()> {
        let deadline = Instant::now() + timeout.max(Duration::from_secs(1));
        loop {
            match open_port(&self.port_name, self.baud_rate, timeout) {
                Ok(port) => {
                    self.port = port;
                    break;
                }
                Err(error) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(100));
                    let _ = error;
                }
                Err(error) => return Err(error),
            }
        }
        self.line.clear();
        Ok(())
    }
}

fn open_port(port_name: &str, baud_rate: u32, timeout: Duration) -> Result<Box<dyn SerialPort>> {
    serialport::new(port_name, baud_rate)
        .timeout(timeout.min(Duration::from_millis(100)))
        .open()
        .with_context(|| format!("failed to open serial port {port_name}"))
}

pub fn ensure_firmware_ready(
    transport: &mut SerialCalibrationTransport,
    timeout: Duration,
) -> Result<()> {
    transport.send_line("HELLO")?;
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if let Some(line) = transport.read_line_until(Duration::from_millis(100))? {
            if line.trim().starts_with("CAL READY") {
                return Ok(());
            }
        }
    }
    bail!("calibration firmware did not respond to HELLO within {timeout:?}")
}
