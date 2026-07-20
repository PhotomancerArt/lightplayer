//! Minimal `ClientIo` over one preview slot runtime, used only for deploys.
//!
//! Sends protocol frames tagged with the slot's `runtime_id` and, because
//! preview workers run in explicit tick mode, posts a tick per receive
//! poll so the runtime's server loop actually processes queued requests
//! (the preview-lab client-io pattern, productized).

use std::cell::RefCell;
use std::rc::Rc;

use async_trait::async_trait;
use lpa_client::ClientIo;
use lpa_link::providers::browser_worker::BrowserInputEnvelope;
use lpc_wire::{ClientMessage, TransportError, WireServerMessage, json};

use super::preview_sleep::PreviewSleeper;
use super::preview_worker::PreviewWorker;

/// ~4 s ceiling (poll every 4 ms) — in-browser shader compiles during
/// project load are the slow step this needs to ride out.
const RECEIVE_POLL_LIMIT: usize = 1_000;
const DEPLOY_TICK_DELTA_MS: u32 = 16;

pub(super) struct PreviewClientIo {
    worker: Rc<RefCell<PreviewWorker>>,
    runtime_id: u32,
    sleeper: Rc<PreviewSleeper>,
}

impl PreviewClientIo {
    pub(super) fn new(
        worker: Rc<RefCell<PreviewWorker>>,
        runtime_id: u32,
        sleeper: Rc<PreviewSleeper>,
    ) -> Self {
        Self {
            worker,
            runtime_id,
            sleeper,
        }
    }
}

#[async_trait(?Send)]
impl ClientIo for PreviewClientIo {
    async fn send(&mut self, msg: ClientMessage) -> Result<(), TransportError> {
        let frame = json::to_string(&msg)
            .map_err(|error| TransportError::Serialization(error.to_string()))?;
        self.worker
            .borrow()
            .post(&BrowserInputEnvelope::ProtocolIn {
                runtime_id: Some(self.runtime_id),
                frame,
            })
            .map_err(TransportError::Other)
    }

    async fn receive(&mut self) -> Result<WireServerMessage, TransportError> {
        for _ in 0..RECEIVE_POLL_LIMIT {
            {
                let mut worker = self.worker.borrow_mut();
                worker.drain_outputs();
                if let Some(frame) = worker.pop_protocol_frame(self.runtime_id) {
                    return json::from_str(&frame)
                        .map_err(|error| TransportError::Deserialization(error.to_string()));
                }
                // Explicit tick mode: advance the runtime so it services
                // the queued request.
                worker
                    .post(&BrowserInputEnvelope::Tick {
                        runtime_id: Some(self.runtime_id),
                        delta_ms: Some(DEPLOY_TICK_DELTA_MS),
                    })
                    .map_err(TransportError::Other)?;
            }
            self.sleeper.sleep_ms(4).await;
        }
        Err(TransportError::Other(format!(
            "timed out waiting for preview runtime {} protocol output",
            self.runtime_id
        )))
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        Ok(())
    }
}
