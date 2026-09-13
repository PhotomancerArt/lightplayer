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
//! transport's demux classifies app-range replies BEFORE the mirror. There
//! is no demux on this side of the seam: the page buffers raw bytes, and
//! whoever drains gets them. Two drainers would split the board's answers
//! between them and both halves would look like a dead board — which is
//! exactly what `port_client_io.rs` says about the serial port, and it is
//! the io `browser_transport.rs` runs push, remove and the manifest write
//! through. So this is that shape: one drainer, pump paused, `close` a
//! no-op because the port belongs to the model's link.

use std::rc::Rc;

use async_trait::async_trait;
use js_sys::{Function, Promise, Reflect};
use lpa_client::ClientIo;
use lpc_wire::{ClientMessage, TransportError, WireServerMessage};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

use super::emulator_tab_bridge::EmulatorTabPort;
use crate::LinkError;

/// How often the receive loop re-drains the page's line buffer. The same
/// 20 ms the serial io and the model's pump use; faster would only find an
/// empty queue.
const RECEIVE_POLL_MS: u32 = 20;

/// Quiet budget for one response, as on the serial io.
///
/// Wall time on the HOST's side of the wall, and a wedge guard rather than
/// a measurement: the guest's own clock is the machine's, and a board that
/// runs at half speed answers in half-speed guest time, not in twice the
/// wall seconds this bounds.
const RESPONSE_BUDGET_MS: u32 = 5_000;

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

    /// An `lpa-client` io on this board's wire, for the exclusive-borrow
    /// conversations. `tap` receives every whole line the io drains, so a
    /// caller that wants the fold to keep hearing the board while it owns
    /// the wire can have it.
    pub fn client_io(&self, tap: Option<Rc<dyn Fn(String)>>) -> Box<dyn ClientIo> {
        Box::new(EmuLineIo {
            port: self.port,
            pending: Vec::new(),
            tap,
        })
    }
}

/// `ClientIo` over one tab board's raw line framing.
struct EmuLineIo {
    port: EmulatorTabPort,
    /// Frames drained but not yet handed out (one drain can carry several).
    pending: Vec<WireServerMessage>,
    tap: Option<Rc<dyn Fn(String)>>,
}

#[async_trait(?Send)]
impl ClientIo for EmuLineIo {
    async fn send(&mut self, msg: ClientMessage) -> Result<(), TransportError> {
        let json = lpc_wire::json::to_string(&msg)
            .map_err(|error| TransportError::Other(format!("encode failed: {error}")))?;
        self.port
            .write(format!("M!{json}\n").as_bytes())
            .map_err(|error| TransportError::Other(error.to_string()))
    }

    async fn receive(&mut self) -> Result<WireServerMessage, TransportError> {
        let mut waited = 0_u32;
        loop {
            if !self.pending.is_empty() {
                return Ok(self.pending.remove(0));
            }
            // A failure on the page's work queue means the write never
            // reached the board. The conversation fails NOW rather than
            // waiting out its budget for an answer to something that was
            // never asked.
            if let Some(error) = self.port.take_error() {
                return Err(TransportError::Other(error));
            }
            let lines = self
                .port
                .take_lines()
                .map_err(|error| TransportError::Other(error.to_string()))?;
            for line in lines {
                if let Some(tap) = &self.tap {
                    tap(line.clone());
                }
                // A non-`M!` line is the board talking — boot output, a
                // log — rather than an answer; the tap above already
                // carried it, and this io has no journal of its own to
                // put it in (the serial twin has a management sink).
                if let Some(json) = line.strip_prefix("M!") {
                    match lpc_wire::json::from_str::<WireServerMessage>(json) {
                        Ok(message) => self.pending.push(message),
                        Err(error) => web_sys::console::warn_1(&JsValue::from_str(&format!(
                            "[emu-link] malformed frame: {error}"
                        ))),
                    }
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

/// One `setTimeout` tick, with no `web-sys` dependency — the twin of
/// `port_client_io.rs`'s, which is private to the serial provider's own
/// feature.
async fn sleep_ms(ms: u32) {
    let promise = Promise::new(&mut |resolve, _reject| {
        let global = js_sys::global();
        let set_timeout = Reflect::get(&global, &JsValue::from_str("setTimeout"))
            .ok()
            .and_then(|value| value.dyn_into::<Function>().ok());
        match set_timeout {
            Some(set_timeout) => {
                let _ = set_timeout.call2(&global, &resolve, &JsValue::from_f64(f64::from(ms)));
            }
            // No `setTimeout` in this scope: resolve immediately rather
            // than hang. The loop degrades to a hot poll, still bounded.
            None => {
                let _ = resolve.call0(&JsValue::NULL);
            }
        }
    });
    let _ = JsFuture::from(promise).await;
}
