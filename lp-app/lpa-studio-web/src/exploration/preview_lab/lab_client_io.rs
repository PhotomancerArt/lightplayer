//! Minimal `ClientIo` over one preview-lab runtime, used only for deploys.
//!
//! Sends protocol frames tagged with the card's `runtime_id` and, because lab
//! workers run in explicit tick mode, posts a tick per receive poll so the
//! runtime's server loop actually processes queued requests.

use std::cell::RefCell;
use std::rc::Rc;

use async_trait::async_trait;
use lpa_client::ClientIo;
use lpa_link::providers::browser_worker::BrowserInputEnvelope;
use lpc_wire::{ClientMessage, TransportError, WireServerMessage, json};

use super::lab_sleep::LabSleeper;
use super::worker_rig::WorkerRig;

/// ~4 s ceiling (poll every 4 ms) — in-browser shader compiles during project
/// load are the slow step this needs to ride out.
const RECEIVE_POLL_LIMIT: usize = 1_000;
const DEPLOY_TICK_DELTA_MS: u32 = 16;

pub(super) struct LabClientIo {
    rig: Rc<RefCell<WorkerRig>>,
    runtime_id: u32,
    sleeper: Rc<LabSleeper>,
}

impl LabClientIo {
    pub(super) fn new(
        rig: Rc<RefCell<WorkerRig>>,
        runtime_id: u32,
        sleeper: Rc<LabSleeper>,
    ) -> Self {
        Self {
            rig,
            runtime_id,
            sleeper,
        }
    }
}

#[async_trait(?Send)]
impl ClientIo for LabClientIo {
    async fn send(&mut self, msg: ClientMessage) -> Result<(), TransportError> {
        let frame = json::to_string(&msg)
            .map_err(|error| TransportError::Serialization(error.to_string()))?;
        self.rig
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
                let mut rig = self.rig.borrow_mut();
                rig.drain_outputs();
                if let Some(frame) = rig.pop_protocol_frame(self.runtime_id) {
                    return json::from_str(&frame)
                        .map_err(|error| TransportError::Deserialization(error.to_string()));
                }
                // Explicit tick mode: advance the runtime so it services the
                // queued request.
                rig.post(&BrowserInputEnvelope::Tick {
                    runtime_id: Some(self.runtime_id),
                    delta_ms: Some(DEPLOY_TICK_DELTA_MS),
                })
                .map_err(TransportError::Other)?;
            }
            self.sleeper.sleep_ms(4).await;
        }
        Err(TransportError::Other(format!(
            "timed out waiting for runtime {} protocol output",
            self.runtime_id
        )))
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        Ok(())
    }
}
