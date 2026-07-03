use std::cell::RefCell;
use std::collections::VecDeque;
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

use crate::{SharedLinkRegistry, UiLogEntry, UiLogLevel};

const RESPONSE_POLL_LIMIT: usize = 240;

pub struct BrowserWorkerClientIo {
    state: Rc<RefCell<BrowserWorkerClientState>>,
}

impl BrowserWorkerClientIo {
    pub fn new(
        registry: SharedLinkRegistry,
        session_id: LinkSessionId,
        logs: Rc<RefCell<Vec<UiLogEntry>>>,
    ) -> Self {
        Self {
            state: Rc::new(RefCell::new(BrowserWorkerClientState {
                registry,
                session_id,
                logs,
                pending_protocol_out: VecDeque::new(),
            })),
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
        for _ in 0..RESPONSE_POLL_LIMIT {
            if let Some(response) = self.state.borrow_mut().pending_protocol_out.pop_front() {
                return Ok(response);
            }

            self.state
                .borrow()
                .post(&BrowserInputEnvelope::Tick { delta_ms: Some(16) })?;
            sleep_ms(4).await?;

            let outputs = self.state.borrow().take_outputs()?;
            for output in outputs {
                match output {
                    BrowserOutputEnvelope::ProtocolOut { frame } => {
                        let response = json::from_str(&frame)
                            .map_err(|error| TransportError::Deserialization(error.to_string()))?;
                        self.state
                            .borrow_mut()
                            .pending_protocol_out
                            .push_back(response);
                    }
                    output => self.state.borrow().record_output(output),
                }
            }

            if let Some(response) = self.state.borrow_mut().pending_protocol_out.pop_front() {
                return Ok(response);
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
    logs: Rc<RefCell<Vec<UiLogEntry>>>,
    pending_protocol_out: VecDeque<WireServerMessage>,
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

fn worker_output_to_log(output: BrowserOutputEnvelope) -> Option<UiLogEntry> {
    match output {
        BrowserOutputEnvelope::Status {
            status, message, ..
        } => Some(UiLogEntry::new(
            UiLogLevel::Info,
            "fw-browser",
            message.unwrap_or(status),
        )),
        BrowserOutputEnvelope::Log {
            level,
            target,
            message,
            ..
        } => Some(UiLogEntry::new(
            parse_worker_log_level(&level),
            target,
            message,
        )),
        BrowserOutputEnvelope::ProtocolOut { .. } => None,
    }
}

fn parse_worker_log_level(level: &str) -> UiLogLevel {
    match level {
        "trace" | "debug" => UiLogLevel::Debug,
        "warn" => UiLogLevel::Warn,
        "error" => UiLogLevel::Error,
        _ => UiLogLevel::Info,
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
