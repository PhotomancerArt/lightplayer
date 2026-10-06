//! One leg's socket loop, shared by both legs.
//!
//! A leg's task reads its socket and its queue at once: binary messages go
//! to the caller's handler, queued messages go out, a ping goes out every
//! [`PING_INTERVAL_S`] seconds, and the leg ends when the peer closes, the
//! queue asks it to, the queue ends (the registry dropped it as
//! overloaded), nothing has been heard for [`SILENT_CLOSE_S`] seconds, or
//! the process is going away.

use std::time::Duration;

use axum::body::Bytes;
use axum::extract::ws::{CloseFrame, Message, Utf8Bytes, WebSocket};
use lpc_relay::{PING_INTERVAL_S, RelayCloseCode, SILENT_CLOSE_S};
use tokio::sync::{mpsc, watch};
use tokio::time::Instant;

use super::relay_registry::LegCommand;

/// How long a leg may stay silent before it is closed as dead.
pub const SILENCE: Duration = Duration::from_secs(SILENT_CLOSE_S as u64);

/// How a leg ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegEnd {
    /// The peer closed, or the socket failed.
    PeerClosed,
    /// Nothing heard for [`SILENCE`].
    Silent,
    /// This end closed it, with this code.
    Closed(RelayCloseCode),
}

/// Bytes each way, for the leg's one log line (never the bytes themselves).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LegTraffic {
    pub bytes_in: u64,
    pub bytes_out: u64,
}

/// Run the leg until it ends. `on_binary` takes each binary message and
/// answers `Err(code)` to close the leg with that code.
pub async fn pump_leg(
    socket: &mut WebSocket,
    inbox: &mut mpsc::Receiver<LegCommand>,
    shutdown: &mut watch::Receiver<bool>,
    mut on_binary: impl FnMut(&[u8]) -> Result<(), RelayCloseCode>,
) -> (LegEnd, LegTraffic) {
    let mut traffic = LegTraffic::default();
    let ping_every = Duration::from_secs(u64::from(PING_INTERVAL_S));
    let mut ping = tokio::time::interval_at(Instant::now() + ping_every, ping_every);
    let mut last_heard = Instant::now();
    if *shutdown.borrow_and_update() {
        close(socket, RelayCloseCode::GoingAway).await;
        return (LegEnd::Closed(RelayCloseCode::GoingAway), traffic);
    }
    let end = loop {
        tokio::select! {
            message = socket.recv() => match message {
                Some(Ok(Message::Binary(bytes))) => {
                    last_heard = Instant::now();
                    traffic.bytes_in += bytes.len() as u64;
                    if let Err(code) = on_binary(&bytes) {
                        close(socket, code).await;
                        break LegEnd::Closed(code);
                    }
                }
                Some(Ok(Message::Ping(_) | Message::Pong(_))) => last_heard = Instant::now(),
                Some(Ok(Message::Text(_))) => {
                    close(socket, RelayCloseCode::UnsupportedData).await;
                    break LegEnd::Closed(RelayCloseCode::UnsupportedData);
                }
                Some(Ok(Message::Close(_)) | Err(_)) | None => break LegEnd::PeerClosed,
            },
            command = inbox.recv() => match command {
                Some(LegCommand::Send(bytes)) => {
                    traffic.bytes_out += bytes.len() as u64;
                    if socket.send(Message::Binary(Bytes::from(bytes))).await.is_err() {
                        break LegEnd::PeerClosed;
                    }
                }
                Some(LegCommand::Close(code)) => {
                    close(socket, code).await;
                    break LegEnd::Closed(code);
                }
                None => {
                    close(socket, RelayCloseCode::Overloaded).await;
                    break LegEnd::Closed(RelayCloseCode::Overloaded);
                }
            },
            _ = ping.tick() => {
                if socket.send(Message::Ping(Bytes::new())).await.is_err() {
                    break LegEnd::PeerClosed;
                }
            }
            () = tokio::time::sleep_until(last_heard + SILENCE) => break LegEnd::Silent,
            _ = shutdown.changed() => {
                close(socket, RelayCloseCode::GoingAway).await;
                break LegEnd::Closed(RelayCloseCode::GoingAway);
            }
        }
    };
    (end, traffic)
}

/// Send a close frame with `code`. A failure means the socket is already
/// gone, which is what closing wanted.
pub async fn close(socket: &mut WebSocket, code: RelayCloseCode) {
    let _ = socket
        .send(Message::Close(Some(CloseFrame {
            code: code.code(),
            reason: Utf8Bytes::from_static(code.reason()),
        })))
        .await;
}

/// The next binary message, skipping pings and pongs. `None` when the
/// socket closes, fails, or sends text.
pub async fn next_binary(socket: &mut WebSocket) -> Option<Bytes> {
    loop {
        match socket.recv().await? {
            Ok(Message::Binary(bytes)) => return Some(bytes),
            Ok(Message::Ping(_) | Message::Pong(_)) => {}
            Ok(Message::Text(_) | Message::Close(_)) | Err(_) => return None,
        }
    }
}

impl std::fmt::Display for LegEnd {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PeerClosed => f.write_str("peer closed"),
            Self::Silent => f.write_str("silent"),
            Self::Closed(code) => write!(f, "closed {} {}", code.code(), code.reason()),
        }
    }
}
