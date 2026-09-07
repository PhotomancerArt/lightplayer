//! An `lpa-client` io over a [`BrowserWorkerLink`]'s protocol channel
//! (wasm only): the exclusive-borrow conversations, on a sim.
//!
//! A push, a project removal and the editor lens all speak the app protocol
//! over a wire the effects layer has borrowed exclusively — the pump paused,
//! this io the only reader. On silicon that io is the serial provider's; on a
//! sim it is this, over the same worker envelopes the link's pump would have
//! drained. Sharing the borrow discipline is the point: two drainers would
//! split the responses between them and both halves would look like a dead
//! board, whatever the transport underneath.
//!
//! Nothing here classifies. Whole `M!` lines are teed to the caller's tap
//! verbatim (so the card's evidence keeps folding while the client owns the
//! channel) and protocol frames are handed on undecoded beyond the wire's
//! own vocabulary — the same shape the browser serial io keeps.

use std::collections::VecDeque;

use async_trait::async_trait;
use lpa_client::ClientIo;
use lpc_wire::{ClientMessage, TransportError, WireServerMessage, json};
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::JsFuture;

use crate::device_link::browser_worker::BrowserWorkerControl;
use crate::providers::browser_worker::{BrowserInputEnvelope, BrowserOutputEnvelope};

/// Overall budget for one `receive()` before it reports a wedged worker.
/// The same ~1 s the studio's other worker io allows.
const RECEIVE_TIMEOUT_MS: i32 = 1_000;

/// How long the io sleeps between drains while waiting for a frame.
///
/// A worker answers in the same event-loop turn it is asked, so the wait is
/// almost always one interval; the budget above is what a silent worker
/// costs, and this is the resolution it is spent at.
const POLL_INTERVAL_MS: i32 = 4;

/// Every whole `M!` line this io drains, verbatim, before it is decoded.
///
/// The studio's lens tap in its own vocabulary; `lpa-link` stays independent
/// of the studio's types, so the join is one closure at the call site.
pub type WorkerLineTap = std::rc::Rc<dyn Fn(String)>;

/// One [`ClientIo`] over a worker link's protocol channel.
pub struct BrowserWorkerLinkIo {
    control: BrowserWorkerControl,
    /// Frames drained but not yet handed out.
    pending: VecDeque<WireServerMessage>,
    tap: Option<WorkerLineTap>,
}

impl BrowserWorkerLinkIo {
    /// An io over the channel `control` speaks for.
    pub fn new(control: BrowserWorkerControl) -> Self {
        Self {
            control,
            pending: VecDeque::new(),
            tap: None,
        }
    }

    /// Tee every whole line this io drains to `tap` (the editor lens's
    /// requirement: the fold never goes deaf while the client owns the
    /// channel).
    pub fn with_tap(mut self, tap: WorkerLineTap) -> Self {
        self.tap = Some(tap);
        self
    }

    /// Drain the worker's buffer: protocol frames into [`Self::pending`],
    /// everything a line-shaped consumer would see into the tap.
    fn drain(&mut self) {
        for output in self.control.take_outputs() {
            let line = match &output {
                BrowserOutputEnvelope::ProtocolOut { frame, .. } => Some(format!("M!{frame}")),
                BrowserOutputEnvelope::Log {
                    level,
                    target,
                    message,
                    ..
                } => Some(format!("{level} {target}: {message}")),
                _ => None,
            };
            if let (Some(tap), Some(line)) = (&self.tap, &line) {
                tap(line.clone());
            }
            if let BrowserOutputEnvelope::ProtocolOut { frame, .. } = output {
                match json::from_str::<WireServerMessage>(&frame) {
                    Ok(message) => self.pending.push_back(message),
                    // A garbled frame is worth saying so: the fold counts
                    // anomalies, and a channel that mangles every frame
                    // must not read as one that is merely quiet.
                    Err(error) => web_sys::console::debug_1(&JsValue::from_str(&format!(
                        "[lpa-link] sim protocol frame unreadable: {error}"
                    ))),
                }
            }
        }
    }
}

#[async_trait(?Send)]
impl ClientIo for BrowserWorkerLinkIo {
    async fn send(&mut self, msg: ClientMessage) -> Result<(), TransportError> {
        let frame = json::to_string(&msg)
            .map_err(|error| TransportError::Serialization(error.to_string()))?;
        self.control
            .post(&BrowserInputEnvelope::ProtocolIn {
                runtime_id: None,
                frame,
            })
            .map_err(TransportError::Other)
    }

    async fn receive(&mut self) -> Result<WireServerMessage, TransportError> {
        let mut waited_ms = 0;
        loop {
            self.drain();
            if let Some(message) = self.pending.pop_front() {
                return Ok(message);
            }
            if waited_ms >= RECEIVE_TIMEOUT_MS {
                return Err(TransportError::Other(
                    "timed out waiting for the sim to answer".to_string(),
                ));
            }
            // A drain-then-sleep poll, not a park on the worker's output
            // signal, and deliberately so: this io runs inside a borrowed
            // wire, so a worker that says NOTHING (a condemned instance, a
            // runtime that never came back from a restart) must reach the
            // deadline rather than wait on a signal that will not fire.
            // Only eviction bounds a hung effect otherwise, which is the
            // shape this whole layer exists to avoid.
            sleep_ms(POLL_INTERVAL_MS).await?;
            waited_ms += POLL_INTERVAL_MS;
        }
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        // The worker belongs to the model's link; the borrow ends, the sim
        // keeps running. Same rule as the serial io.
        Ok(())
    }
}

async fn sleep_ms(ms: i32) -> Result<(), TransportError> {
    let promise =
        js_sys::Promise::new(&mut |resolve: js_sys::Function, reject: js_sys::Function| {
            let Some(window) = web_sys::window() else {
                let _ = reject.call1(&JsValue::NULL, &JsValue::from_str("missing window"));
                return;
            };
            if let Err(error) =
                window.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms)
            {
                let _ = reject.call1(&JsValue::NULL, &error);
            }
        });
    JsFuture::from(promise)
        .await
        .map(|_| ())
        .map_err(|error| TransportError::Other(format!("{error:?}")))
}
