use std::cell::RefCell;
use std::rc::Rc;

use async_trait::async_trait;
use lpa_client::ClientIo;
use lpa_link::provider::session::LinkSessionId;
use lpa_link::providers::browser_worker::{
    BrowserInputEnvelope, BrowserOutputEnvelope, BrowserWorkerProvider,
};
use lpa_link::providers::{LinkProviderInstance, LinkProviderRegistry};
use lpa_link::{LinkProvider, LinkProviderKind};
use lpc_wire::{ClientMessage, TransportError, WireServerMessage, json};
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::JsFuture;

use super::browser_worker_log::{worker_log_draft, worker_status_draft};
use super::pending_server_messages::{BatchItem, PendingServerMessages};
use crate::{SharedLinkRegistry, UiLogDraft};

const RESPONSE_POLL_LIMIT: usize = 240;

pub struct BrowserWorkerClientIo {
    state: Rc<RefCell<BrowserWorkerClientState>>,
    pending: PendingServerMessages<WireServerMessage>,
}

impl BrowserWorkerClientIo {
    pub fn new(
        registry: SharedLinkRegistry,
        session_id: LinkSessionId,
        logs: Rc<RefCell<Vec<UiLogDraft>>>,
    ) -> Self {
        Self {
            state: Rc::new(RefCell::new(BrowserWorkerClientState {
                registry,
                session_id,
                logs,
            })),
            pending: PendingServerMessages::new(),
        }
    }
}

#[async_trait(?Send)]
impl ClientIo for BrowserWorkerClientIo {
    async fn send(&mut self, msg: ClientMessage) -> Result<(), TransportError> {
        let frame = json::to_string(&msg)
            .map_err(|error| TransportError::Serialization(error.to_string()))?;
        self.state
            .borrow()
            .post(&BrowserInputEnvelope::ProtocolIn { frame })
    }

    async fn receive(&mut self) -> Result<WireServerMessage, TransportError> {
        if let Some(message) = self.pending.pop() {
            return Ok(message);
        }

        // The worker owns its own clock (self-ticking with real deltas), so this
        // loop is a pure consumer: it polls for worker outputs and never advances
        // simulation time. Event-driven receive is future work (M7).
        for _ in 0..RESPONSE_POLL_LIMIT {
            sleep_ms(4).await?;

            let outputs = self.state.borrow().take_outputs()?;
            let state = &self.state;
            self.pending.ingest(outputs, |output| match output {
                BrowserOutputEnvelope::ProtocolOut { frame } => json::from_str(&frame)
                    .map(BatchItem::Protocol)
                    .map_err(|error| TransportError::Deserialization(error.to_string())),
                output => {
                    state.borrow().record_output(output);
                    Ok(BatchItem::Other)
                }
            })?;

            if let Some(message) = self.pending.pop() {
                return Ok(message);
            }
        }
        Err(TransportError::Other(
            "timed out waiting for browser worker protocol output".to_string(),
        ))
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        let (registry, session_id) = {
            let state = self.state.borrow();
            (Rc::clone(&state.registry), state.session_id.clone())
        };
        let mut registry = registry.borrow_mut();
        let provider = browser_worker_provider_mut(&mut registry)?;
        provider
            .close(&session_id)
            .await
            .map_err(|error| TransportError::Other(error.to_string()))
    }
}

struct BrowserWorkerClientState {
    registry: SharedLinkRegistry,
    session_id: LinkSessionId,
    logs: Rc<RefCell<Vec<UiLogDraft>>>,
}

impl BrowserWorkerClientState {
    fn post(&self, envelope: &BrowserInputEnvelope) -> Result<(), TransportError> {
        let mut registry = self.registry.borrow_mut();
        browser_worker_provider_mut(&mut registry)?
            .post(&self.session_id, envelope)
            .map_err(|error| TransportError::Other(error.to_string()))
    }

    fn take_outputs(&self) -> Result<Vec<BrowserOutputEnvelope>, TransportError> {
        let mut registry = self.registry.borrow_mut();
        browser_worker_provider_mut(&mut registry)?
            .take_outputs(&self.session_id)
            .map_err(|error| TransportError::Other(error.to_string()))
    }

    fn record_output(&self, output: BrowserOutputEnvelope) {
        if let Some(log) = worker_output_to_log(output) {
            self.logs.borrow_mut().push(log);
        }
    }
}

fn browser_worker_provider_mut(
    registry: &mut LinkProviderRegistry,
) -> Result<&mut BrowserWorkerProvider, TransportError> {
    match registry.provider_mut(LinkProviderKind::BrowserWorker) {
        Some(LinkProviderInstance::BrowserWorker(provider)) => Ok(provider),
        Some(_) => Err(TransportError::Other(
            "browser-worker registry entry has the wrong provider type".to_string(),
        )),
        None => Err(TransportError::Other(
            "browser-worker provider is not available".to_string(),
        )),
    }
}

/// Map a worker output envelope to a console draft. The logging policy —
/// level parsing (trace preserved), target-as-detail, status labeling —
/// lives in the host-testable [`super::browser_worker_log`] module.
fn worker_output_to_log(output: BrowserOutputEnvelope) -> Option<UiLogDraft> {
    match output {
        BrowserOutputEnvelope::Status {
            status, message, ..
        } => Some(worker_status_draft(status, message)),
        BrowserOutputEnvelope::Log {
            level,
            target,
            message,
            ..
        } => Some(worker_log_draft(&level, target, message)),
        BrowserOutputEnvelope::ProtocolOut { .. } => None,
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
