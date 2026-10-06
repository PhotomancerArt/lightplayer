//! A handle on one LAN session's wire, shared by everything that drains it.
//!
//! The model's link (`device_link::browser_websocket`) and a conversation
//! that borrows the wire (`WsClientIo`: a push, the editor lens) drain the
//! same session's lp-link end — never at once, because the effects layer
//! pauses the link's pump for the length of a borrow. That end lives in
//! `ws_link_port.rs`, one per session, so any number of handles share one
//! link (the Bluetooth wire's rule, `browser_ble/ble_wire.rs`). Creating a
//! handle starts servicing the session.

use super::{browser_websocket, ws_link_port};
use crate::device_link::wire_reader::WireRead;

/// A session's wire. Cheap: holds only the session id.
#[derive(Debug)]
pub struct WsWire {
    session: u32,
}

impl WsWire {
    /// A handle on `session`'s wire. Its link is serviced from now on.
    pub fn new(session: u32) -> Self {
        ws_link_port::attach(session);
        Self { session }
    }

    /// The JS session id.
    pub fn session(&self) -> u32 {
        self.session
    }

    /// Whether the socket is open right now.
    pub fn is_connected(&self) -> bool {
        browser_websocket::is_connected(self.session)
    }

    /// Connect (bounded, 10 s) or confirm the socket.
    pub async fn connect(&self) -> Result<(), String> {
        browser_websocket::connect(self.session).await
    }

    /// Close the socket by request (no reconnect follows).
    pub async fn disconnect(&self) {
        browser_websocket::disconnect(self.session).await;
    }

    /// Queue one request — its JSON, no `M!`, no newline — as one lp-link
    /// message. `Err` when the link is not connected or will not take it.
    pub fn send_client_json(&self, json: &str) -> Result<(), String> {
        ws_link_port::send_client_json(self.session, json)
    }

    /// Everything the board said since the last drain, in order.
    pub fn take_reads(&self) -> Vec<WireRead> {
        ws_link_port::take_reads(self.session)
    }

    /// What the link said about itself since the last ask (up, a refusal, a
    /// stall, the packed opt-in's outcome), for the device journal.
    pub fn take_notes(&self) -> Vec<String> {
        ws_link_port::take_notes(self.session)
    }

    /// Every error the session recorded since the last call.
    pub fn take_errors(&self) -> Result<Vec<String>, String> {
        browser_websocket::take_errors(self.session)
    }

    /// Whether the session's secure lp-link is up (its handshake is done).
    pub fn is_link_up(&self) -> bool {
        ws_link_port::is_up(self.session)
    }
}

/// Whether a session error is the link itself dying (the phrase
/// `browser_websocket.js` reserves for a drop), as opposed to one failed
/// send.
pub fn is_link_lost(error: &str) -> bool {
    error.starts_with("wi-fi link lost")
}
