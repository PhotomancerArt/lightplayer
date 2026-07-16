use anyhow::{Context, Result, bail};
use lpa_client::stream::{DeviceByteStream, SerialPortByteStream};
use std::path::Path;
use std::time::{Duration, Instant};

use crate::client::serial_port::detect_serial_port;

/// Line-protocol calibration transport over the shared byte-stream seam.
///
/// Calibration firmware speaks a plain text protocol (`HELLO` →
/// `CAL READY`, `CAL PULSE …`), not the `M!` app protocol, so this sits
/// directly on [`SerialPortByteStream`] — the same stream the hardware
/// transport uses — rather than a `DeviceSession`.
pub struct SerialCalibrationTransport {
    port_spec: Option<String>,
    port_name: String,
    baud_rate: u32,
    stream: Option<SerialPortByteStream>,
    line: Vec<u8>,
}

impl SerialCalibrationTransport {
    pub fn open(port_spec: Option<&str>) -> Result<Self> {
        let normalized = port_spec.map(|port| port.strip_prefix("serial:").unwrap_or(port));
        let config = detect_serial_port(normalized, None)?;
        let stream = open_stream(&config.port, config.baud_rate)?;
        Ok(Self {
            port_spec: normalized.map(str::to_string),
            port_name: config.port,
            baud_rate: config.baud_rate,
            stream: Some(stream),
            line: Vec::new(),
        })
    }

    pub fn send_line(&mut self, line: &str) -> Result<()> {
        let mut bytes = line.as_bytes().to_vec();
        bytes.push(b'\n');
        self.stream()?
            .write_all(&bytes)
            .context("write to serial port")?;
        Ok(())
    }

    pub fn read_line_until(&mut self, timeout: Duration) -> Result<Option<String>> {
        let deadline = Instant::now() + timeout;
        let mut buf = [0u8; 64];
        while Instant::now() < deadline {
            // `read_available` polls with the stream's 100 ms timeout and
            // reports "nothing right now" as Ok(0).
            let count = self.stream()?.read_available(&mut buf)?;
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
        Ok(None)
    }

    pub fn reconnect(&mut self, timeout: Duration) -> Result<()> {
        self.stream = None;
        let deadline = Instant::now() + timeout.max(Duration::from_secs(10));
        loop {
            match self.detect_reconnect_port() {
                Ok(port_name) => match open_stream(&port_name, self.baud_rate) {
                    Ok(stream) => {
                        self.port_name = port_name;
                        self.stream = Some(stream);
                        break;
                    }
                    Err(error) if Instant::now() >= deadline => return Err(error),
                    Err(_) => {}
                },
                Err(error) if Instant::now() >= deadline => return Err(error),
                Err(_) => {}
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        self.line.clear();
        Ok(())
    }

    fn detect_reconnect_port(&self) -> Result<String> {
        if self.port_spec.is_some() || Path::new(&self.port_name).exists() {
            return Ok(self.port_name.clone());
        }
        detect_serial_port(None, Some(self.baud_rate))
            .map(|config| config.port)
            .or_else(|_| Ok(self.port_name.clone()))
    }

    fn stream(&mut self) -> Result<&mut SerialPortByteStream> {
        match self.stream.as_mut() {
            Some(stream) => Ok(stream),
            None => bail!("serial port is not open"),
        }
    }
}

fn open_stream(port_name: &str, baud_rate: u32) -> Result<SerialPortByteStream> {
    SerialPortByteStream::open(port_name, baud_rate)
        .with_context(|| format!("failed to open serial port {port_name}"))
}

pub fn ensure_firmware_ready(
    transport: &mut SerialCalibrationTransport,
    timeout: Duration,
) -> Result<()> {
    let deadline = Instant::now() + timeout;
    let mut next_hello = Instant::now();
    while Instant::now() < deadline {
        if Instant::now() >= next_hello {
            transport.send_line("HELLO")?;
            next_hello = Instant::now() + Duration::from_millis(250);
        }
        if let Some(line) = transport.read_line_until(Duration::from_millis(100))? {
            if line.trim().starts_with("CAL READY") {
                return Ok(());
            }
        }
    }
    bail!("calibration firmware did not respond to HELLO within {timeout:?}")
}
