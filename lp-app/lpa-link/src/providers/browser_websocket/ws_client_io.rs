//! `lpa-client`'s `ClientIo` over a LAN session: what a push, a project
//! removal, a manifest write and the editor lens speak through.
//!
//! The exclusive-borrow io, the Bluetooth io's shape (`ble_client_io.rs`):
//! the effects layer has paused the link's pump, so this is the only drainer
//! of the session's lp-link end while it lives, and `close` is a no-op
//! because the session belongs to the model's link. Everything it drains
//! goes to the tap first, so the device fold keeps hearing the board.
//!
//! It fails fast on a dropped socket (`wi-fi link lost: …`) and on a link
//! reset inside a connection (a rekey, or the board's link giving up on a
//! frame): every request in flight on the old session is lost.

use std::collections::VecDeque;
use std::rc::Rc;

use async_trait::async_trait;
use lpa_client::ClientIo;
use lpc_wire::{ClientMessage, TransportError, WireServerMessage};

use super::ws_wire::{WsWire, is_link_lost};
use crate::device_link::link_port_edge::sleep_ms;
use crate::device_link::wire_reader::WireRead;

/// How often the receive loop re-drains the session (the serial io's and
/// the Bluetooth io's 20 ms).
const RECEIVE_POLL_MS: u32 = 20;

/// Quiet budget for one response: a wedge guard, not a measurement.
const RESPONSE_BUDGET_MS: u32 = 5_000;

/// What the io hands the tap: a whole message as its `M!{json}` line, a
/// journal note from the session's link, or the session reporting the link
/// failed underneath it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WsTapLine {
    Line(String),
    Note(String),
    PortError(String),
}

/// The io. Build one per borrow.
pub struct WsClientIo {
    wire: Rc<WsWire>,
    /// Replies drained but not yet handed out, in order — with a link reset
    /// among them as the error it hands out in its place.
    pending: VecDeque<Result<WireServerMessage, String>>,
    tap: Option<Rc<dyn Fn(WsTapLine)>>,
}

impl WsClientIo {
    pub fn new(wire: Rc<WsWire>, tap: Option<Rc<dyn Fn(WsTapLine)>>) -> Self {
        Self {
            wire,
            pending: VecDeque::new(),
            tap,
        }
    }

    fn tap(&self, line: WsTapLine) {
        if let Some(tap) = &self.tap {
            tap(line);
        }
    }

    /// Surface the session's errors. A lost link fails the conversation now.
    fn check_errors(&self) -> Result<(), TransportError> {
        let errors = self.wire.take_errors().map_err(TransportError::Other)?;
        let mut lost = None;
        for error in errors {
            self.tap(WsTapLine::PortError(error.clone()));
            if is_link_lost(&error) {
                lost = Some(error);
            }
        }
        match lost {
            Some(error) => Err(TransportError::Other(error)),
            None => Ok(()),
        }
    }

    /// Drain the session once: the link's notes to the tap, then every read.
    fn drain(&mut self) {
        for note in self.wire.take_notes() {
            self.tap(WsTapLine::Note(note));
        }
        for read in self.wire.take_reads() {
            match read {
                WireRead::Frame(frame) => {
                    self.tap(WsTapLine::Line(frame.to_line()));
                    if let Ok(message) = frame.message {
                        self.pending.push_back(Ok(message));
                    }
                }
                WireRead::Line(line) => self.tap(WsTapLine::Line(line)),
                WireRead::Error(error) => {
                    self.tap(WsTapLine::Note(format!("undeliverable frame: {error}")));
                }
                WireRead::LinkReset(note) => {
                    self.tap(WsTapLine::Note(note.clone()));
                    self.pending.push_back(Err(note));
                }
                WireRead::Note(_) => {}
            }
        }
    }
}

#[async_trait(?Send)]
impl ClientIo for WsClientIo {
    async fn send(&mut self, msg: ClientMessage) -> Result<(), TransportError> {
        self.check_errors()?;
        let json = lpc_wire::json::to_string(&msg)
            .map_err(|error| TransportError::Other(format!("encode failed: {error}")))?;
        self.wire
            .send_client_json(&json)
            .map_err(TransportError::Other)
    }

    async fn receive(&mut self) -> Result<WireServerMessage, TransportError> {
        let mut waited = 0_u32;
        loop {
            if let Some(next) = self.pending.pop_front() {
                return next.map_err(TransportError::Other);
            }
            self.check_errors()?;
            self.drain();
            if !self.pending.is_empty() {
                continue;
            }
            if waited >= RESPONSE_BUDGET_MS {
                return Err(TransportError::Other(format!(
                    "the device did not respond over Wi-Fi within {:.1}s",
                    f64::from(RESPONSE_BUDGET_MS) / 1_000.0
                )));
            }
            sleep_ms(RECEIVE_POLL_MS).await;
            waited += RECEIVE_POLL_MS;
        }
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        // The session belongs to the model's link.
        Ok(())
    }
}
