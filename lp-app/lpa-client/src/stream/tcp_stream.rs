//! [`DeviceByteStream`] over a TCP connection — an emulator's UART.
//!
//! Espressif's `esp-emu` (and QEMU's `-serial tcp::PORT,server`) expose the
//! emulated UART0 as a TCP *server*. On macOS a pty cannot stand in for the
//! port: `serialport` sets the rate through `IOSSIOSPEED`, which the pty
//! driver refuses with `ENOTTY`, so a pty bridge never gets past open. A
//! byte stream that speaks TCP directly is the only path that lets the same
//! host tooling (`lp-cli upload serial:tcp://127.0.0.1:5555`) drive a
//! device with no cable.
//!
//! No control lines: [`DeviceByteStream::set_signals`] is a no-op, so the
//! caller must not rely on a DTR/RTS reset — the provider skips it for
//! `tcp://` endpoints and lets the readiness engine's periodic
//! `ClientRequest::Hello` establish the session instead.

use std::io::{ErrorKind, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use crate::stream::{ByteStreamError, DeviceByteStream};

/// A TCP connection as a [`DeviceByteStream`].
pub struct TcpByteStream {
    addr: String,
    stream: TcpStream,
}

impl TcpByteStream {
    /// Connect to `addr` (`host:port`).
    pub fn connect(addr: &str) -> Result<Self, ByteStreamError> {
        let stream = connect_nonblocking(addr)?;
        Ok(Self {
            addr: addr.to_string(),
            stream,
        })
    }
}

fn connect_nonblocking(addr: &str) -> Result<TcpStream, ByteStreamError> {
    let mut last = None;
    for sock_addr in addr
        .to_socket_addrs()
        .map_err(|e| ByteStreamError::io(format!("resolve {addr}: {e}")))?
    {
        match TcpStream::connect_timeout(&sock_addr, Duration::from_secs(5)) {
            Ok(stream) => {
                stream
                    .set_nodelay(true)
                    .map_err(|e| ByteStreamError::io(format!("nodelay {addr}: {e}")))?;
                stream
                    .set_nonblocking(true)
                    .map_err(|e| ByteStreamError::io(format!("nonblocking {addr}: {e}")))?;
                return Ok(stream);
            }
            Err(e) => last = Some(e),
        }
    }
    Err(ByteStreamError::io(format!(
        "connect {addr}: {}",
        last.map_or_else(|| "no addresses".to_string(), |e| e.to_string())
    )))
}

impl DeviceByteStream for TcpByteStream {
    fn read_available(&mut self, buf: &mut [u8]) -> Result<usize, ByteStreamError> {
        match self.stream.read(buf) {
            Ok(0) => Err(ByteStreamError::Closed),
            Ok(n) => Ok(n),
            Err(e) if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::Interrupted => {
                Ok(0)
            }
            Err(e) if matches!(
                e.kind(),
                ErrorKind::ConnectionReset | ErrorKind::ConnectionAborted | ErrorKind::BrokenPipe
            ) =>
            {
                Err(ByteStreamError::Closed)
            }
            Err(e) => Err(ByteStreamError::io(e.to_string())),
        }
    }

    fn write_all(&mut self, bytes: &[u8]) -> Result<(), ByteStreamError> {
        let mut rest = bytes;
        while !rest.is_empty() {
            match self.stream.write(rest) {
                Ok(0) => return Err(ByteStreamError::Closed),
                Ok(n) => rest = &rest[n..],
                Err(e)
                    if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::Interrupted =>
                {
                    std::thread::sleep(Duration::from_millis(1));
                }
                Err(e) if matches!(
                    e.kind(),
                    ErrorKind::ConnectionReset | ErrorKind::ConnectionAborted | ErrorKind::BrokenPipe
                ) =>
                {
                    return Err(ByteStreamError::Closed);
                }
                Err(e) => return Err(ByteStreamError::io(e.to_string())),
            }
        }
        self.stream
            .flush()
            .map_err(|e| ByteStreamError::io(e.to_string()))
    }

    /// TCP has no modem lines; a reset dance is accepted and ignored.
    fn set_signals(&mut self, _dtr: Option<bool>, _rts: Option<bool>) -> Result<(), ByteStreamError> {
        Ok(())
    }

    /// Reconnect; the rate is meaningless over TCP and ignored.
    fn reopen(&mut self, _baud_rate: u32) -> Result<(), ByteStreamError> {
        self.stream = connect_nonblocking(&self.addr)?;
        Ok(())
    }
}
