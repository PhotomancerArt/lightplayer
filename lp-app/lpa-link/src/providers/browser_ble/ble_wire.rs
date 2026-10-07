//! A handle on one Bluetooth session's wire, shared by everything that
//! drains it.
//!
//! The link (`device_link::browser_ble`) and a conversation that borrows the
//! wire (`BleClientIo`: a push, the editor lens) both drain the same
//! session's lp-link end — never at once, because the effects layer pauses
//! the link's pump for the length of a borrow. That end lives in
//! `ble_link_port.rs`, one per session and not one per handle, so any number
//! of handles on a session share one link: a message is read once, whole,
//! by whichever drainer holds the wire, and nothing is cut between two
//! readers. Creating a handle starts servicing the session.

use super::{ble_link_port, browser_ble};
use crate::device_link::wire_reader::WireRead;

/// A session's wire. Cheap: holds only the session id, so it can be shared
/// (`Rc`) or made again for the same session.
#[derive(Debug)]
pub struct BleWire {
    session: u32,
}

impl BleWire {
    /// A handle on `session`'s wire. Its link is serviced from now on (the
    /// board's hello waits, read, for the first drain).
    pub fn new(session: u32) -> Self {
        ble_link_port::attach(session);
        Self { session }
    }

    /// The JS session id.
    pub fn session(&self) -> u32 {
        self.session
    }

    /// Whether the GATT link is up right now.
    pub fn is_connected(&self) -> bool {
        browser_ble::is_connected(self.session)
    }

    /// Connect (bounded, 10 s) or confirm the link.
    pub async fn connect(&self) -> Result<(), String> {
        browser_ble::connect(self.session).await
    }

    /// Close the link by request (no reconnect follows).
    pub async fn disconnect(&self) {
        browser_ble::disconnect(self.session).await;
    }

    /// Queue one request — its JSON, no `M!`, no newline — as one lp-link
    /// message. `Err` when the link is not connected or will not take it.
    pub fn send_client_json(&self, json: &str) -> Result<(), String> {
        ble_link_port::send_client_json(self.session, json)
    }

    /// Queue one channel-3 (update) message as one lp-link message.
    /// `Ok(false)`: the board has not announced the update channel on this
    /// connection, so nothing was queued (DS9). `Err` like
    /// [`Self::send_client_json`].
    pub fn send_update(&self, message: &[u8]) -> Result<bool, String> {
        ble_link_port::send_update(self.session, message)
    }

    /// The board's channel-3 (update) messages since the last drain, this
    /// connection's only. Only the model's link drains them.
    pub fn take_updates(&self) -> Vec<Vec<u8>> {
        ble_link_port::take_updates(self.session)
    }

    /// Everything the board said since the last drain, in order: wire
    /// messages (decoded once, JSON or packed) and link resets.
    pub fn take_reads(&self) -> Vec<WireRead> {
        ble_link_port::take_reads(self.session)
    }

    /// What the link said about itself since the last ask (up, a stall, the
    /// packed opt-in's outcome), for the device journal.
    pub fn take_notes(&self) -> Vec<String> {
        ble_link_port::take_notes(self.session)
    }

    /// Every error the session recorded since the last call.
    pub fn take_errors(&self) -> Result<Vec<String>, String> {
        browser_ble::take_errors(self.session)
    }

    /// Whether the session's lp-link is up (its handshake is done).
    pub fn is_link_up(&self) -> bool {
        ble_link_port::is_up(self.session)
    }
}

/// Whether a session error is the link itself dying (the phrase
/// `browser_ble.js` reserves for a drop), as opposed to one failed write.
pub fn is_link_lost(error: &str) -> bool {
    error.starts_with("bluetooth link lost")
}
