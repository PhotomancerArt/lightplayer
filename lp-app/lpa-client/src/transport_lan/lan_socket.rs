//! The WebSocket under a LAN link: one binary message is one lp-link frame
//! (the `ws()` preset is datagram framing), both ways.
//!
//! A blocking socket with a per-read timeout: a read waits no longer than
//! the link's next timer, a write is a plain blocking write (bounded by
//! [`WRITE_TIMEOUT`]). tungstenite keeps a partly read frame across a
//! timed-out read, so a timeout loses nothing.

use std::io::ErrorKind;
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use tungstenite::{Message, WebSocket};

use super::lan_error::LanError;
use super::lan_target::LanTarget;

/// How long the TCP connect may take.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// How long one write may block before the link counts as lost.
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);

/// One open WebSocket to a board's `/link`.
pub struct LanSocket {
    ws: WebSocket<TcpStream>,
}

impl LanSocket {
    /// Resolve `target`, connect, and upgrade to a WebSocket on `/link`.
    pub fn connect(target: &LanTarget) -> Result<Self, LanError> {
        let fail = |detail: String| LanError::Connect {
            target: target.to_string(),
            detail,
        };
        let addrs = (target.host.as_str(), target.port)
            .to_socket_addrs()
            .map_err(|e| fail(format!("the name did not resolve ({e})")))?;
        let mut last = None;
        let mut stream = None;
        for addr in addrs {
            match TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT) {
                Ok(s) => {
                    stream = Some(s);
                    break;
                }
                Err(e) => last = Some(e),
            }
        }
        let stream = stream.ok_or_else(|| {
            fail(last.map_or_else(|| "no address".to_string(), |e| e.to_string()))
        })?;
        stream
            .set_nodelay(true)
            .and_then(|()| stream.set_write_timeout(Some(WRITE_TIMEOUT)))
            .map_err(|e| fail(e.to_string()))?;
        let (ws, _response) = tungstenite::client::client(target.url(), stream)
            .map_err(|e| fail(format!("the WebSocket upgrade failed ({e})")))?;
        Ok(Self { ws })
    }

    /// Send one frame as one binary message.
    pub fn send(&mut self, frame: &[u8]) -> Result<(), LanError> {
        self.ws
            .get_ref()
            .set_nonblocking(false)
            .map_err(|e| LanError::Lost(e.to_string()))?;
        self.ws
            .send(Message::Binary(frame.to_vec()))
            .map_err(classify)
    }

    /// The next frame, waiting at most `wait` (`Duration::ZERO`: only what
    /// has already arrived). `Ok(None)`: nothing in time.
    pub fn recv(&mut self, wait: Duration) -> Result<Option<Vec<u8>>, LanError> {
        let socket = self.ws.get_ref();
        let set = if wait.is_zero() {
            socket.set_nonblocking(true)
        } else {
            socket
                .set_nonblocking(false)
                .and_then(|()| socket.set_read_timeout(Some(wait)))
        };
        set.map_err(|e| LanError::Lost(e.to_string()))?;
        loop {
            match self.ws.read() {
                Ok(Message::Binary(frame)) => return Ok(Some(frame)),
                // The board sends only binary; tungstenite answers pings.
                Ok(Message::Text(_) | Message::Ping(_) | Message::Pong(_) | Message::Frame(_)) => {}
                Ok(Message::Close(frame)) => {
                    return Err(LanError::Closed {
                        code: frame.map(|f| u16::from(f.code)),
                    });
                }
                Err(tungstenite::Error::Io(e))
                    if matches!(
                        e.kind(),
                        ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted
                    ) =>
                {
                    return Ok(None);
                }
                Err(error) => return Err(classify(error)),
            }
        }
    }

    /// Close the WebSocket (best effort: the board may be gone already).
    pub fn close(mut self) {
        self.shutdown();
    }

    /// [`Self::close`], for a caller that cannot give the socket up; nothing
    /// is read or written on it afterwards.
    pub fn shutdown(&mut self) {
        let _ = self.ws.get_ref().set_nonblocking(false);
        let _ = self
            .ws
            .get_ref()
            .set_read_timeout(Some(Duration::from_millis(200)));
        if self.ws.close(None).is_ok() {
            // Read until the board's close echo (or a timeout) so the close
            // handshake completes instead of resetting the connection.
            for _ in 0..10 {
                match self.ws.read() {
                    Ok(_) => {}
                    Err(tungstenite::Error::Io(e))
                        if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
                    Err(_) => break,
                }
            }
        }
        let _ = self.ws.get_ref().shutdown(std::net::Shutdown::Both);
    }
}

fn classify(error: tungstenite::Error) -> LanError {
    match error {
        tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed => {
            LanError::Closed { code: None }
        }
        other => LanError::Lost(other.to_string()),
    }
}
