//! A minimal `lpa-client` conversation over a granted Web Serial port.
//!
//! The round-2 coarse-effect seam needs one thing the dumb `Link` contract
//! cannot give it: a REAL protocol exchange (request out, decoded response
//! body back). The model's frame mirror is deliberately lossy — a response
//! body surfaces there as a label — so anything that wants the body speaks
//! `lpa-client` below the mirror, on the raw `M!` line framing the JS
//! controller already does.
//!
//! # Exclusive borrow, or two readers fight
//!
//! `takeLines` drains a shared buffer. While this io runs, the effects layer
//! MUST have paused the model's link pump for the same port (that is the
//! coarse-effect discipline: borrow the wire exclusively, run, give it
//! back). Two drainers would each get half the frames, and the halves would
//! both look like a dead device.
//!
//! Lines that are not `M!` frames (boot output, logs) are forwarded to the
//! management event sink so the conversation's journal stays honest.

use async_trait::async_trait;
use js_sys::{Function, Promise, Reflect};
use lpa_client::{ClientIo, LpClient};
use lpc_wire::{ClientMessage, TransportError, WireServerMessage};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

use crate::provider::management_event::{LinkManagementEvent, LinkManagementEventSink};
use crate::providers::browser_serial_esp32::browser_serial;
use crate::{LinkError, LinkManagementProgress};

/// How often the receive loop re-drains the JS line buffer.
const RECEIVE_POLL_MS: u32 = 20;

/// Quiet budget for one response. Heartbeats restart nothing here — the
/// conversation is short and the caller's activity deadline is the outer
/// bound — so this is a plain per-request ceiling.
const RESPONSE_BUDGET_MS: u32 = 5_000;

/// Write `bytes` to `path` on the device over the app protocol.
///
/// Round 2's first consumer is the flash activity's board-manifest stamp
/// (`/hardware.json`, board-selection D4). M3's push conversation reuses
/// this seam's shape.
pub async fn write_device_file(
    port_id: u32,
    path: &str,
    bytes: &[u8],
    events: LinkManagementEventSink,
) -> Result<(), LinkError> {
    use lpc_model::AsLpPath;
    events.emit(LinkManagementEvent::progress(LinkManagementProgress::new(
        format!("Writing {path}"),
    )));
    let io = PortLineIo {
        port_id,
        pending: Vec::new(),
        events: events.clone(),
    };
    let mut client = LpClient::new(io);
    let outcome = client
        .fs_write(path.as_path(), bytes.to_vec())
        .await
        .map_err(|error| LinkError::other(format!("device file write failed: {error}")))?;
    for event in outcome.events {
        events.emit(LinkManagementEvent::log(format!("{event:?}")));
    }
    Ok(())
}

/// Push a project onto the device over the app protocol.
///
/// The conversation itself is `lpa-client`'s — the same stop → clear →
/// chunked write → load → hash-verify the studio runs against the sim — so
/// this function is only the port half: build the io, hand the progress
/// through, translate the error.
pub async fn push_device_project(
    port_id: u32,
    files: &[(String, Vec<u8>)],
    expected_hash: &str,
    fallback_storage_id: &str,
    events: LinkManagementEventSink,
) -> Result<lpa_client::PushReport, LinkError> {
    let io = PortLineIo {
        port_id,
        pending: Vec::new(),
        events: events.clone(),
    };
    let mut client = LpClient::new(io);
    let mut progress = |label: String, percent: Option<u8>| {
        let update = LinkManagementProgress::new(label);
        // A label-only step stays label-only: reporting 0% would make the
        // card's bar jump backwards between named steps.
        let update = match percent {
            Some(percent) => update.with_percent(u32::from(percent)),
            None => update,
        };
        events.emit(LinkManagementEvent::progress(update));
    };
    lpa_client::push_project(
        &mut client,
        files,
        expected_hash,
        fallback_storage_id,
        &mut progress,
    )
    .await
    .map_err(|error| LinkError::other(format!("push failed: {error}")))
}

/// `ClientIo` over one port's raw line framing.
struct PortLineIo {
    port_id: u32,
    /// Frames drained but not yet handed out (one `takeLines` batch can
    /// carry several).
    pending: Vec<WireServerMessage>,
    events: LinkManagementEventSink,
}

#[async_trait(?Send)]
impl ClientIo for PortLineIo {
    async fn send(&mut self, msg: ClientMessage) -> Result<(), TransportError> {
        let json = lpc_wire::json::to_string(&msg)
            .map_err(|error| TransportError::Other(format!("encode failed: {error}")))?;
        browser_serial::write_line(self.port_id, &format!("M!{json}\n"))
            .await
            .map_err(|error| TransportError::Other(error.to_string()))
    }

    async fn receive(&mut self) -> Result<WireServerMessage, TransportError> {
        let mut waited = 0_u32;
        loop {
            if !self.pending.is_empty() {
                return Ok(self.pending.remove(0));
            }
            for line in browser_serial::take_lines(self.port_id) {
                match line.strip_prefix("M!") {
                    Some(json) => match lpc_wire::json::from_str::<WireServerMessage>(json) {
                        Ok(message) => self.pending.push(message),
                        Err(error) => self.events.emit(LinkManagementEvent::log(format!(
                            "malformed frame: {error}"
                        ))),
                    },
                    None => self.events.emit(LinkManagementEvent::log(line)),
                }
            }
            if !self.pending.is_empty() {
                continue;
            }
            if waited >= RESPONSE_BUDGET_MS {
                return Err(TransportError::Other(format!(
                    "device did not respond within {:.1}s",
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

/// One `setTimeout` tick, with no `web-sys` dependency: the global's
/// `setTimeout` looked up reflectively works in both window and worker
/// scopes.
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
            // No setTimeout in this scope: resolve immediately rather than
            // hang. The receive loop degrades to a hot poll, which is still
            // bounded by its budget.
            None => {
                let _ = resolve.call0(&JsValue::NULL);
            }
        }
    });
    let _ = JsFuture::from(promise).await;
}
