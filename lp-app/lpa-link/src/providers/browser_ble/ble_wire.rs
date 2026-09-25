//! One Bluetooth session's byte stream, shared by everything that reads it.
//!
//! The link (`device_link::browser_ble`) and a conversation that borrows the
//! wire (`BleClientIo`: a push, the editor lens) both drain the same JS
//! buffer — never at once, because the effects layer pauses the link's pump
//! for the length of a borrow. The ONE [`LineSplitter`] lives here, beside
//! the buffer, so a line that straddles the hand-over is still re-joined
//! whole instead of being cut in two between two splitters.

use std::cell::RefCell;

use crate::device_link::demux::LineSplitter;

use super::browser_ble;

/// A session's stream. Cheap to share (`Rc`); holds no JS value.
#[derive(Debug)]
pub struct BleWire {
    session: u32,
    splitter: RefCell<LineSplitter>,
}

impl BleWire {
    pub fn new(session: u32) -> Self {
        Self {
            session,
            splitter: RefCell::new(LineSplitter::default()),
        }
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

    /// Queue bytes for the board. `Err` for an unknown session; a link that
    /// is not up answers `Ok(false)` and says why through [`Self::take_errors`].
    pub fn write(&self, bytes: &[u8]) -> Result<bool, String> {
        browser_ble::write(self.session, bytes)
    }

    /// Every whole line the board has sent since the last call.
    pub fn take_lines(&self) -> Result<Vec<String>, String> {
        let bytes = browser_ble::take_bytes(self.session)?;
        if bytes.is_empty() {
            return Ok(Vec::new());
        }
        Ok(self.splitter.borrow_mut().push(&bytes))
    }

    /// Every error the session recorded since the last call.
    pub fn take_errors(&self) -> Result<Vec<String>, String> {
        browser_ble::take_errors(self.session)
    }

    /// Drop a partial line: a new connection is not the rest of the old
    /// one's last line.
    pub fn clear_partial(&self) {
        self.splitter.borrow_mut().clear();
    }
}

/// Whether a session error is the link itself dying (the phrase
/// `browser_ble.js` reserves for a drop), as opposed to one failed write.
pub fn is_link_lost(error: &str) -> bool {
    error.starts_with("bluetooth link lost")
}
