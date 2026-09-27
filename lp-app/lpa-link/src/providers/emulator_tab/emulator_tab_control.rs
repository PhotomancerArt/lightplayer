//! The effect-side handle on one tab-hosted board: what a card's verb does
//! to it, as opposed to what the model does to its link.
//!
//! Shared with the link, not a replacement for it. The same port backs both,
//! which is what makes the exclusive borrow mean something: the effects
//! layer pauses the link's pump for the duration of a coarse effect, so the
//! io built here is the only drainer of the board's bytes while the
//! conversation runs.
//!
//! # The io is the exclusive-borrow one, not the shared one
//!
//! `SharedLinkClientIo` exists for conversations that ride a link the pump
//! is still draining (the card's frame feed), and it works because the
//! transport's demux classifies app-range replies BEFORE the mirror. This io
//! drains the board's lp-link end directly (`emulator_tab_link_port`), the
//! same queue of decoded messages the pump drains — so, as on the serial
//! port (`port_client_io.rs`), two drainers would split the board's answers
//! between them and both halves would look like a dead board. This is that
//! shape: one drainer, pump paused (plan D2), `close` a no-op because the
//! port belongs to the model's link. What changed with lp-link is only that
//! the drainer takes whole messages, so nothing is ever handed back.
//!
//! A link reset (the board rebooted under the conversation) fails the
//! request in flight at once (plan D9) and is teed as a journal note.

use std::collections::VecDeque;
use std::rc::Rc;

use async_trait::async_trait;
use lpa_client::ClientIo;
use lpc_wire::{ClientMessage, TransportError, WireServerMessage};
use wasm_bindgen::JsValue;

use super::emulator_tab_bridge::EmulatorTabPort;
use super::emulator_tab_link_port::{send_client_json, take_notes, take_reads};
use crate::LinkError;
use crate::device_link::link_port_edge::sleep_ms;
use crate::device_link::wire_reader::WireRead;

/// How often the receive loop re-drains the board's link. The same 20 ms
/// the serial io and the model's pump use; faster would only find an empty
/// queue.
const RECEIVE_POLL_MS: u32 = 20;

/// Quiet budget for one response, as on the serial io.
///
/// Wall time on the HOST's side of the wall, and a wedge guard rather than
/// a measurement: the guest's own clock is the machine's, and a board that
/// runs at half speed answers in half-speed guest time, not in twice the
/// wall seconds this bounds.
const RESPONSE_BUDGET_MS: u32 = 5_000;

/// What the conversation io tees while it holds the wire: a console line or
/// a message (as its `M!` line, which the fold's demux reads), or a journal
/// note from the board's link (a reset).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EmuTapLine {
    Line(String),
    Note(String),
}

/// One running tab-hosted board, for the effects layer.
#[derive(Clone, Debug)]
pub struct EmulatorTabControl {
    port: EmulatorTabPort,
}

impl EmulatorTabControl {
    pub fn new(port: EmulatorTabPort) -> Self {
        Self { port }
    }

    /// Write a packaged build into the chip and reboot into it (D5/D24).
    ///
    /// Mode A does not go through the ROM downloader: there is no serial
    /// bootloader protocol to speak when the flash chip is a byte array the
    /// page can address. The fetch and the write live in the bridge; this
    /// answers the build's display name so the card's summary can name what
    /// was written.
    pub async fn flash_package(&self, manifest_url: &str) -> Result<String, LinkError> {
        self.port.flash_package(manifest_url).await
    }

    /// Erase the whole chip (the card's Factory reset).
    pub async fn erase(&self) -> Result<(), LinkError> {
        self.port.erase_flash().await
    }

    /// Reset the chip. Not a replug: the cable stays in and the port stays
    /// as it is, so the link does not re-enumerate underneath the model.
    pub async fn reset(&self) -> Result<(), LinkError> {
        self.port.reset().await
    }

    /// How fast the board runs against wall time, as the worker last
    /// reported it (D8/D25). `None` until the first stats tick.
    pub fn dilation(&self) -> Option<f64> {
        self.port.dilation()
    }

    /// Whether the worker, the module fetch and the cold ROM boot are still
    /// in flight. The studio asks before it decides a board has given up —
    /// a boot takes seconds and the fold cannot see the difference between
    /// "still coming up" and "nothing there".
    pub fn is_starting(&self) -> bool {
        self.port.is_starting()
    }

    /// `state` + `pins` + the tab's own counters, as JSON.
    pub async fn probes(&self) -> Result<String, LinkError> {
        self.port.probes().await
    }

    /// End the board and its worker.
    pub async fn dispose(&self) -> Result<(), LinkError> {
        self.port.dispose().await
    }

    /// An `lpa-client` io on this board's link, for the exclusive-borrow
    /// conversations. `tap` receives every whole line (and link note) the io
    /// drains, so a caller that wants the fold to keep hearing the board
    /// while it owns the wire can have it.
    pub fn client_io(&self, tap: Option<Rc<dyn Fn(EmuTapLine)>>) -> Box<dyn ClientIo> {
        Box::new(EmuLineIo {
            port: self.port,
            pending: VecDeque::new(),
            tap,
        })
    }
}

/// `ClientIo` over one tab board's link.
struct EmuLineIo {
    port: EmulatorTabPort,
    /// Replies drained but not yet handed out, in order — a link reset among
    /// them as the error it hands out in its place.
    pending: VecDeque<Result<WireServerMessage, String>>,
    tap: Option<Rc<dyn Fn(EmuTapLine)>>,
}

#[async_trait(?Send)]
impl ClientIo for EmuLineIo {
    async fn send(&mut self, msg: ClientMessage) -> Result<(), TransportError> {
        let json = lpc_wire::json::to_string(&msg)
            .map_err(|error| TransportError::Other(format!("encode failed: {error}")))?;
        send_client_json(self.port, &json).map_err(TransportError::Other)
    }

    async fn receive(&mut self) -> Result<WireServerMessage, TransportError> {
        let mut waited = 0_u32;
        loop {
            if let Some(next) = self.pending.pop_front() {
                return next.map_err(TransportError::Other);
            }
            // A failure on the page's work queue means the write never
            // reached the board. The conversation fails NOW rather than
            // waiting out its budget for an answer to something that was
            // never asked.
            if let Some(error) = self.port.take_error() {
                return Err(TransportError::Other(error));
            }
            // The link's own notes (up, stalled, answering again) wait for
            // the model's pump, which is paused while this io holds the
            // wire; the lens tap stands in for it (D13).
            if let Some(tap) = &self.tap {
                for note in take_notes(self.port) {
                    tap(EmuTapLine::Note(note));
                }
            }
            let Some(reads) = take_reads(self.port) else {
                return Err(TransportError::Other(
                    "the emulated board's port is not open".to_string(),
                ));
            };
            for read in reads {
                match read {
                    // The board talking — boot output, a log — rather than an
                    // answer.
                    WireRead::Line(line) => {
                        if let Some(tap) = self.tap.as_ref().filter(|_| !line.is_empty()) {
                            tap(EmuTapLine::Line(line));
                        }
                    }
                    WireRead::Frame(frame) => {
                        if let Some(tap) = &self.tap {
                            tap(EmuTapLine::Line(frame.to_line()));
                        }
                        match frame.message {
                            Ok(message) => self.pending.push_back(Ok(message)),
                            Err(error) => web_sys::console::warn_1(&JsValue::from_str(&format!(
                                "[emu-link] malformed frame: {error}"
                            ))),
                        }
                    }
                    WireRead::Error(error) => web_sys::console::warn_1(&JsValue::from_str(
                        &format!("[emu-link] undeliverable frame: {error}"),
                    )),
                    // Everything in flight is lost: fail it now (D9).
                    WireRead::LinkReset(note) => {
                        if let Some(tap) = &self.tap {
                            tap(EmuTapLine::Note(note.clone()));
                        }
                        self.pending.push_back(Err(note));
                    }
                    WireRead::Note(_) => {}
                }
            }
            if !self.pending.is_empty() {
                continue;
            }
            if waited >= RESPONSE_BUDGET_MS {
                return Err(TransportError::Other(format!(
                    "the emulated board did not respond within {:.1}s",
                    f64::from(RESPONSE_BUDGET_MS) / 1_000.0
                )));
            }
            sleep_ms(RECEIVE_POLL_MS).await;
            waited += RECEIVE_POLL_MS;
        }
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        // The port belongs to the model's link; the borrow ends, the port
        // stays open.
        Ok(())
    }
}
