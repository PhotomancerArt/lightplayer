//! `lpa-client`'s `ClientIo` over a Bluetooth session: what a push, a
//! project removal, a manifest write and the editor lens speak through.
//!
//! The exclusive-borrow io, the same shape as the serial port's
//! (`port_client_io.rs`) and the tab emulator's: the effects layer has paused
//! the link's pump, so this is the only drainer of the session's lp-link end
//! for as long as it lives, and `close` is a no-op because the session
//! belongs to the model's link. A request goes out as one link message, and
//! every message comes back whole — the link already reassembled it.
//!
//! Everything it drains goes to the tap first, so the device fold keeps
//! hearing the board (heartbeats, the link's own notes) while a
//! conversation owns the wire.
//!
//! # Failing fast
//!
//! - **The GATT link dropped** (`bluetooth link lost: …`): both ends lost
//!   the session, and the conversation fails now rather than after its
//!   quiet budget.
//! - **The link reset inside a connection** (the board's link gave up on a
//!   frame): every request in flight is lost; `receive` fails at once with
//!   the reset's note, in order behind any reply that beat it, and the tap
//!   hears the note so the fold's journal says why (the USB cut-over's D9,
//!   the same over Bluetooth).

use std::collections::VecDeque;
use std::rc::Rc;

use async_trait::async_trait;
use lpa_client::ClientIo;
use lpc_wire::{ClientMessage, TransportError, WireServerMessage};

use super::ble_wire::{BleWire, is_link_lost};
use crate::device_link::link_port_edge::sleep_ms;
use crate::device_link::wire_reader::WireRead;

/// How often the receive loop re-drains the session. The same 20 ms the
/// serial io and the model's pump use.
const RECEIVE_POLL_MS: u32 = 20;

/// Quiet budget for one response, as on the serial io. A wedge guard, not a
/// measurement: a knob write's round trip over BLE was ~31–40 ms in Run F.
const RESPONSE_BUDGET_MS: u32 = 5_000;

/// What the io hands the tap: a whole message as its `M!{json}` line, a
/// journal note from the session's link (a reset, a stall), or the session
/// reporting the link failed underneath it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BleTapLine {
    Line(String),
    Note(String),
    PortError(String),
}

/// The io. Build one per borrow.
pub struct BleClientIo {
    wire: Rc<BleWire>,
    /// Replies drained but not yet handed out, in order — with a link reset
    /// among them as the error it hands out in its place.
    pending: VecDeque<Result<WireServerMessage, String>>,
    tap: Option<Rc<dyn Fn(BleTapLine)>>,
}

impl BleClientIo {
    pub fn new(wire: Rc<BleWire>, tap: Option<Rc<dyn Fn(BleTapLine)>>) -> Self {
        Self {
            wire,
            pending: VecDeque::new(),
            tap,
        }
    }

    fn tap(&self, line: BleTapLine) {
        if let Some(tap) = &self.tap {
            tap(line);
        }
    }

    /// Surface the session's errors. A lost link fails the conversation now
    /// rather than after its whole quiet budget.
    fn check_errors(&self) -> Result<(), TransportError> {
        let errors = self.wire.take_errors().map_err(TransportError::Other)?;
        let mut lost = None;
        for error in errors {
            self.tap(BleTapLine::PortError(error.clone()));
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
            self.tap(BleTapLine::Note(note));
        }
        for read in self.wire.take_reads() {
            match read {
                // The fold hears a message as the line it always read, in
                // either form; the reader decoded it once.
                WireRead::Frame(frame) => {
                    self.tap(BleTapLine::Line(frame.to_line()));
                    // A frame whose JSON did not decode is dropped here and
                    // NOT lost: the tap carried it to the fold, whose demux
                    // counts it as an anomaly.
                    if let Ok(message) = frame.message {
                        self.pending.push_back(Ok(message));
                    }
                }
                WireRead::Line(line) => self.tap(BleTapLine::Line(line)),
                // A journal line, not a dead port (a link port never reads
                // one; kept so nothing is ever silence).
                WireRead::Error(error) => {
                    self.tap(BleTapLine::Note(format!("undeliverable frame: {error}")));
                }
                // Everything in flight is lost: fail it now (D9).
                WireRead::LinkReset(note) => {
                    self.tap(BleTapLine::Note(note.clone()));
                    self.pending.push_back(Err(note));
                }
                // Notes come from `take_notes` (`take_reads` hands out none).
                WireRead::Note(_) => {}
            }
        }
    }
}

#[async_trait(?Send)]
impl ClientIo for BleClientIo {
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
                    "the device did not respond over Bluetooth within {:.1}s",
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
