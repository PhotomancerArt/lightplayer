//! `lpa-client`'s `ClientIo` over a Bluetooth session: what a push, a
//! project removal, a manifest write and the editor lens speak through.
//!
//! The exclusive-borrow io, the same shape as the serial port's and the tab
//! emulator's: the effects layer has paused the link's pump, so this is the
//! only reader of the session for as long as it lives, and `close` is a
//! no-op because the session belongs to the model's link. Every line it
//! drains goes to the tap first, so the device fold keeps hearing the board
//! (heartbeats, logs) while a conversation owns the wire.

use std::rc::Rc;

use async_trait::async_trait;
use js_sys::{Function, Promise, Reflect};
use lpa_client::ClientIo;
use lpc_wire::{ClientMessage, TransportError, WireServerMessage};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

use super::ble_wire::{BleWire, is_link_lost};

/// How often the receive loop re-drains the session. The same 20 ms the
/// serial io and the model's pump use.
const RECEIVE_POLL_MS: u32 = 20;

/// Quiet budget for one response, as on the serial io. A wedge guard, not a
/// measurement: a knob write's round trip over BLE was ~31–40 ms in Run F.
const RESPONSE_BUDGET_MS: u32 = 5_000;

/// What the io hands the tap: a whole line off the wire, or the session
/// reporting the link failed underneath it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BleTapLine {
    Line(String),
    PortError(String),
}

/// The io. Build one per borrow.
pub struct BleClientIo {
    wire: Rc<BleWire>,
    pending: Vec<WireServerMessage>,
    tap: Option<Rc<dyn Fn(BleTapLine)>>,
}

impl BleClientIo {
    pub fn new(wire: Rc<BleWire>, tap: Option<Rc<dyn Fn(BleTapLine)>>) -> Self {
        Self {
            wire,
            pending: Vec::new(),
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
}

#[async_trait(?Send)]
impl ClientIo for BleClientIo {
    async fn send(&mut self, msg: ClientMessage) -> Result<(), TransportError> {
        self.check_errors()?;
        let json = lpc_wire::json::to_string(&msg)
            .map_err(|error| TransportError::Other(format!("encode failed: {error}")))?;
        match self.wire.write(format!("M!{json}\n").as_bytes()) {
            Ok(true) => Ok(()),
            Ok(false) => Err(TransportError::Other(
                "the bluetooth link is not connected".to_string(),
            )),
            Err(error) => Err(TransportError::Other(error)),
        }
    }

    async fn receive(&mut self) -> Result<WireServerMessage, TransportError> {
        let mut waited = 0_u32;
        loop {
            if !self.pending.is_empty() {
                return Ok(self.pending.remove(0));
            }
            self.check_errors()?;
            let lines = self.wire.take_lines().map_err(TransportError::Other)?;
            for line in lines {
                self.tap(BleTapLine::Line(line.clone()));
                // A non-`M!` line is the board talking (a log), not an
                // answer; the tap above already carried it.
                // A malformed frame is dropped here and NOT lost: the tap
                // carried the raw line to the fold, whose demux counts it as
                // an anomaly the way it counts every garbled frame.
                if let Some(json) = line.strip_prefix("M!")
                    && let Ok(message) = lpc_wire::json::from_str::<WireServerMessage>(json)
                {
                    self.pending.push(message);
                }
            }
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

/// One `setTimeout` tick, with no `web-sys` dependency.
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
            None => {
                let _ = resolve.call0(&JsValue::NULL);
            }
        }
    });
    let _ = JsFuture::from(promise).await;
}
