use std::cell::RefCell;
use std::rc::Rc;

use async_trait::async_trait;
use lpa_client::ClientIo;
use lpa_link::LinkProvider;
use lpa_link::provider::session::LinkSessionId;
use lpa_link::providers::browser_worker::{
    BrowserInputEnvelope, BrowserOutputEnvelope, BrowserWorkerProvider,
};
use lpc_wire::{ClientMessage, TransportError, WireServerMessage, json};
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::JsFuture;

use crate::{UxLogEntry, UxLogLevel};

const RESPONSE_POLL_LIMIT: usize = 240;

pub struct BrowserWorkerClientIo {
    state: Rc<RefCell<BrowserWorkerClientState>>,
}

impl BrowserWorkerClientIo {
    pub fn new(
        provider: Rc<RefCell<BrowserWorkerProvider>>,
        session_id: LinkSessionId,
        logs: Rc<RefCell<Vec<UxLogEntry>>>,
    ) -> Self {
        Self {
            state: Rc::new(RefCell::new(BrowserWorkerClientState {
                provider,
                session_id,
                logs,
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
                        return Ok(response);
                    }
                    output => self.state.borrow().record_output(output),
                }
            }
        }
        Err(TransportError::Other(
            "timed out waiting for browser worker protocol output".to_string(),
        ))
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        self.state
            .borrow()
            .provider
            .borrow_mut()
            .close(&self.state.borrow().session_id)
            .await
            .map_err(|error| TransportError::Other(error.to_string()))
    }
}

struct BrowserWorkerClientState {
    provider: Rc<RefCell<BrowserWorkerProvider>>,
    session_id: LinkSessionId,
    logs: Rc<RefCell<Vec<UxLogEntry>>>,
}

impl BrowserWorkerClientState {
    fn post(&self, envelope: &BrowserInputEnvelope) -> Result<(), TransportError> {
        self.provider
            .borrow()
            .post(&self.session_id, envelope)
            .map_err(|error| TransportError::Other(error.to_string()))
    }

    fn take_outputs(&self) -> Result<Vec<BrowserOutputEnvelope>, TransportError> {
        self.provider
            .borrow_mut()
            .take_outputs(&self.session_id)
            .map_err(|error| TransportError::Other(error.to_string()))
    }

    fn record_output(&self, output: BrowserOutputEnvelope) {
        if let Some(log) = worker_output_to_log(output) {
            self.logs.borrow_mut().push(log);
        }
    }
}

pub fn worker_output_to_log(output: BrowserOutputEnvelope) -> Option<UxLogEntry> {
    match output {
        BrowserOutputEnvelope::Status {
            status, message, ..
        } => Some(UxLogEntry::new(
            UxLogLevel::Info,
            "fw-browser",
            message.unwrap_or(status),
        )),
        BrowserOutputEnvelope::Log {
            level,
            target,
            message,
            ..
        } => Some(UxLogEntry::new(
            parse_worker_log_level(&level),
            target,
            message,
        )),
        BrowserOutputEnvelope::ProtocolOut { .. } => None,
    }
}

fn parse_worker_log_level(level: &str) -> UxLogLevel {
    match level {
        "trace" | "debug" => UxLogLevel::Debug,
        "warn" => UxLogLevel::Warn,
        "error" => UxLogLevel::Error,
        _ => UxLogLevel::Info,
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
