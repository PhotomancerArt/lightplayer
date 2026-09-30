//! A host on a board's link through the `emu serve` door, on the wall clock:
//! the product's own `lpc_wire::WireLinkPort` over the board's
//! `/board/<id>/bytes` WebSocket.
//!
//! Since wire proto 30 a board served from the current tree speaks lp-link on
//! USB, so a door test that talks to it needs a link host, not `M!` lines.
//! This is the smallest one: every read is fed to the port, every frame the
//! port wants to send is written, and what the port read comes back as
//! [`DoorRead`]s the test can look through.

use std::net::TcpStream;
use std::time::{Duration, Instant};

use lpc_wire::{ClientMessage, PortRead, WireLinkPort, WireServerMessage};
use tungstenite::{Message, WebSocket};

use super::NET;

/// One thing the host read, in order.
#[derive(Debug)]
pub enum DoorRead {
    /// A wire message: its JSON as sent, whether it came packed, and the
    /// message.
    Message {
        json: String,
        packed: bool,
        message: WireServerMessage,
    },
    /// A console line (raw text or a log record).
    Console(String),
    /// A link event or port note, as `[link] …`.
    Link(String),
}

pub struct DoorLink {
    socket: WebSocket<TcpStream>,
    pub port: WireLinkPort,
    start: Instant,
    /// Everything read so far.
    pub reads: Vec<DoorRead>,
    /// Every byte the board sent, as it arrived.
    pub raw: Vec<u8>,
}

impl DoorLink {
    /// Host the link on an open byte socket (`Serve::bytes`).
    pub fn new(mut socket: WebSocket<TcpStream>, want_packed: bool) -> Self {
        socket
            .get_mut()
            .set_read_timeout(Some(Duration::from_millis(10)))
            .expect("a read timeout");
        Self {
            socket,
            port: WireLinkPort::new(
                lpc_wire::lp_link::LinkConfig::usb(),
                0x0D00_C6A1,
                want_packed,
            ),
            start: Instant::now(),
            reads: Vec::new(),
            raw: Vec::new(),
        }
    }

    fn now(&self) -> u64 {
        self.start.elapsed().as_micros() as u64
    }

    /// Send one request.
    pub fn send(&mut self, message: &ClientMessage) {
        self.port
            .send_client(message)
            .unwrap_or_else(|e| panic!("the link refused the request: {e:?}"));
        self.flush();
    }

    /// One read (up to 10 ms), then the port's side of the link.
    pub fn pump(&mut self) {
        match self.socket.read() {
            Ok(Message::Binary(bytes)) => {
                self.raw.extend_from_slice(&bytes);
                let now = self.now();
                self.port.on_bytes(now, &bytes);
            }
            Ok(Message::Close(_)) => panic!("the byte endpoint closed"),
            Ok(_) => {}
            Err(tungstenite::Error::Io(e))
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(e) => panic!("reading the byte endpoint: {e}"),
        }
        self.flush();
        while let Some(read) = self.port.poll_read() {
            self.reads.push(match read {
                PortRead::Message(payload) => DoorRead::Message {
                    message: payload
                        .message
                        .unwrap_or_else(|e| panic!("a message did not parse: {e}")),
                    json: payload.json,
                    packed: payload.packed,
                },
                PortRead::Log(line) => DoorRead::Console(line),
                PortRead::Up { generation } => DoorRead::Link(format!("up (session {generation})")),
                PortRead::Reset { reason } => DoorRead::Link(format!("reset ({reason:?})")),
                PortRead::Note(note) => DoorRead::Link(note),
            });
        }
    }

    /// Pump until `done` holds over everything read, within the wall net.
    pub fn pump_until(&mut self, what: &str, done: impl Fn(&[DoorRead]) -> bool) {
        let deadline = Instant::now() + NET;
        while !done(&self.reads) {
            assert!(
                Instant::now() < deadline,
                "{what} did not happen within the wall net; read so far:\n{:#?}",
                self.reads
            );
            self.pump();
        }
    }

    /// The message answering request `id`, once it has been read.
    pub fn answer(&self, id: u64) -> Option<(&str, bool, &WireServerMessage)> {
        self.reads.iter().find_map(|read| match read {
            DoorRead::Message {
                json,
                packed,
                message,
            } if message.id == id => Some((json.as_str(), *packed, message)),
            _ => None,
        })
    }

    fn flush(&mut self) {
        let now = self.now();
        while let Some(frame) = self.port.poll_transmit(now) {
            let frame = frame.to_vec();
            self.socket
                .send(Message::Binary(frame.into()))
                .expect("writing a frame");
        }
    }
}
