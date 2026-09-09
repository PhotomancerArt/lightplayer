//! [`DeviceByteStream`] over a WebSocket — the sibling of
//! [`TcpByteStream`](super::TcpByteStream).
//!
//! `lp-cli emu serve` exposes each emulated board's raw byte link as a
//! WebSocket endpoint (`/board/<id>/bytes`) rather than a TCP socket, because
//! a browser cannot open a TCP socket and the same door has to serve both.
//! This is the host side of that door, so `lp-cli upload`, `lp-cli emu` and
//! everything else that takes a host specifier reach an emulated board with
//! one command:
//!
//! ```text
//! lp-cli upload <project> serial:ws://127.0.0.1:5599/board/c6-a/bytes
//! ```
//!
//! **Bytes stay bytes.** A binary frame's payload is exactly the bytes, in
//! both directions; nothing here adds a length prefix, an envelope or a
//! control verb. A frame boundary carries no meaning — the framing transport
//! above this seam finds its own lines — so a read may return part of a frame
//! and a write may become one frame per call.
//!
//! No control lines: [`DeviceByteStream::set_signals`] is a no-op, exactly as
//! it is over TCP. The emulator's DTR/RTS live on its **control** endpoint
//! (`/board/<id>/control`), which is a different socket on purpose — in-band
//! control would be a dialect every byte client would have to speak. The
//! provider skips the reset-on-open for `ws://` for the same reason it skips
//! it for `tcp://`, and lets the readiness engine's periodic
//! `ClientRequest::Hello` establish the session instead.
//!
//! `ws://` only. `wss://` would want a TLS stack in a crate that is also
//! compiled for the browser, and the door this talks to is a dev-mode
//! loopback server; a `wss://` URL is refused by name rather than silently
//! downgraded.

use std::collections::VecDeque;
use std::io::ErrorKind;
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use tungstenite::client::IntoClientRequest;
use tungstenite::{Message, WebSocket};

use crate::stream::{ByteStreamError, DeviceByteStream};

/// How long the TCP connect under the handshake may take.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// A WebSocket connection as a [`DeviceByteStream`].
pub struct WsByteStream {
    url: String,
    socket: WebSocket<TcpStream>,
    /// Bytes received in a frame the caller's buffer could not hold in one
    /// go. A frame is not a read: the caller reads what it asked for and the
    /// rest waits here.
    pending: VecDeque<u8>,
}

impl WsByteStream {
    /// Connect to `url` (`ws://host:port/path`).
    pub fn connect(url: &str) -> Result<Self, ByteStreamError> {
        let socket = handshake(url)?;
        Ok(Self {
            url: url.to_string(),
            socket,
            pending: VecDeque::new(),
        })
    }
}

/// The TCP connect and the WebSocket handshake, both blocking, and then the
/// socket is put into non-blocking mode for the pump.
///
/// The handshake runs on a blocking socket deliberately: a non-blocking one
/// would make `tungstenite` return `WouldBlock` mid-handshake, and the retry
/// loop that answer needs would be a worse thing to own than five seconds of
/// waiting on a connect.
fn handshake(url: &str) -> Result<WebSocket<TcpStream>, ByteStreamError> {
    if url.starts_with("wss://") {
        return Err(ByteStreamError::io(format!(
            "{url}: wss:// is not supported on this seam — the emulator's byte door is a \
             loopback ws:// server"
        )));
    }
    let request = url
        .into_client_request()
        .map_err(|e| ByteStreamError::io(format!("{url}: {e}")))?;
    let authority = request
        .uri()
        .authority()
        .map(|a| a.to_string())
        .ok_or_else(|| ByteStreamError::io(format!("{url}: no host:port")))?;
    let port = request.uri().port_u16().unwrap_or(80);
    let host = request
        .uri()
        .host()
        .ok_or_else(|| ByteStreamError::io(format!("{url}: no host")))?;

    let mut last = None;
    let mut stream = None;
    for addr in (host, port)
        .to_socket_addrs()
        .map_err(|e| ByteStreamError::io(format!("resolve {authority}: {e}")))?
    {
        match TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT) {
            Ok(s) => {
                stream = Some(s);
                break;
            }
            Err(e) => last = Some(e),
        }
    }
    let stream = stream.ok_or_else(|| {
        ByteStreamError::io(format!(
            "connect {authority}: {}",
            last.map_or_else(|| "no addresses".to_string(), |e| e.to_string())
        ))
    })?;
    stream
        .set_nodelay(true)
        .map_err(|e| ByteStreamError::io(format!("nodelay {authority}: {e}")))?;

    let (socket, _response) = tungstenite::client::client(request, stream)
        .map_err(|e| ByteStreamError::io(format!("websocket handshake {url}: {e}")))?;
    socket
        .get_ref()
        .set_nonblocking(true)
        .map_err(|e| ByteStreamError::io(format!("nonblocking {authority}: {e}")))?;
    Ok(socket)
}

/// `WouldBlock` is the ordinary answer on a non-blocking socket with nothing
/// to say; a closed connection is [`ByteStreamError::Closed`] rather than an
/// error, because a port that went away is a state and not a fault.
fn classify(error: tungstenite::Error) -> Option<ByteStreamError> {
    match error {
        tungstenite::Error::Io(e)
            if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::Interrupted) =>
        {
            None
        }
        tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed => {
            Some(ByteStreamError::Closed)
        }
        tungstenite::Error::Io(e)
            if matches!(
                e.kind(),
                ErrorKind::ConnectionReset | ErrorKind::ConnectionAborted | ErrorKind::BrokenPipe
            ) =>
        {
            Some(ByteStreamError::Closed)
        }
        other => Some(ByteStreamError::io(other.to_string())),
    }
}

impl DeviceByteStream for WsByteStream {
    fn read_available(&mut self, buf: &mut [u8]) -> Result<usize, ByteStreamError> {
        while self.pending.is_empty() {
            match self.socket.read() {
                Ok(Message::Binary(bytes)) => self.pending.extend(bytes),
                // A text frame's UTF-8 is bytes too. The door sends binary,
                // but refusing text here would make this seam pickier than
                // the thing it talks to.
                Ok(Message::Text(text)) => self.pending.extend(text.into_bytes()),
                // tungstenite queues the pong itself; nothing to do, and a
                // ping is not a byte.
                Ok(Message::Ping(_) | Message::Pong(_) | Message::Frame(_)) => continue,
                Ok(Message::Close(_)) => return Err(ByteStreamError::Closed),
                Err(e) => match classify(e) {
                    None => return Ok(0),
                    Some(e) => return Err(e),
                },
            }
        }
        let n = buf.len().min(self.pending.len());
        for slot in buf.iter_mut().take(n) {
            *slot = self.pending.pop_front().expect("checked");
        }
        Ok(n)
    }

    fn write_all(&mut self, bytes: &[u8]) -> Result<(), ByteStreamError> {
        if bytes.is_empty() {
            return Ok(());
        }
        self.socket
            .write(Message::Binary(bytes.to_vec()))
            .map_err(|e| classify(e).unwrap_or(ByteStreamError::Closed))?;
        // The socket is non-blocking, so a flush can be partial: tungstenite
        // keeps the rest and the next call sends it.
        loop {
            match self.socket.flush() {
                Ok(()) => return Ok(()),
                Err(e) => match classify(e) {
                    None => std::thread::sleep(Duration::from_millis(1)),
                    Some(e) => return Err(e),
                },
            }
        }
    }

    /// A WebSocket has no modem lines; a reset dance is accepted and ignored,
    /// exactly as over TCP. The emulator's DTR/RTS are on its control
    /// endpoint.
    fn set_signals(
        &mut self,
        _dtr: Option<bool>,
        _rts: Option<bool>,
    ) -> Result<(), ByteStreamError> {
        Ok(())
    }

    /// Reconnect. The rate is meaningless over a WebSocket and ignored — and
    /// a reconnect is an application closing the port and opening it again,
    /// which is exactly what the emulator's coupling rule reads it as.
    fn reopen(&mut self, _baud_rate: u32) -> Result<(), ByteStreamError> {
        self.socket = handshake(&self.url)?;
        self.pending.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wss_is_refused_by_name_rather_than_downgraded() {
        let Err(e) = WsByteStream::connect("wss://example.invalid/board/c6-a/bytes") else {
            panic!("wss:// has no TLS on this seam and must be refused");
        };
        assert!(format!("{e}").contains("wss://"), "{e}");
    }

    #[test]
    fn a_url_with_no_host_is_an_error_not_a_panic() {
        assert!(WsByteStream::connect("not-a-url").is_err());
    }
}
