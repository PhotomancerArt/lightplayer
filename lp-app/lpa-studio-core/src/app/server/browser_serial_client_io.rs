use std::cell::RefCell;
use std::rc::Rc;

use async_trait::async_trait;
use lpa_client::project_deploy::request_label;
use lpa_client::ClientIo;
use lpa_link::provider::session::LinkSessionId;
use lpa_link::providers::browser_serial_esp32::BrowserSerialEsp32Provider;
use lpa_link::providers::{LinkProviderInstance, LinkProviderRegistry};
use lpa_link::{LinkProvider, LinkProviderKind};
use lpc_wire::{json, ClientMessage, TransportError, WireServerMessage};
use wasm_bindgen::{prelude::wasm_bindgen, JsValue};
use wasm_bindgen_futures::JsFuture;

use super::browser_serial_readiness::{
    BrowserSerialReadinessClassifier, BrowserSerialReadinessFailure,
};
use crate::core::activity::{UiActivityStep, UiActivityStepState};
use crate::{
    ServerController, SharedLinkRegistry, UiActivity, UiLogEntry, UiLogLevel, UiStatus,
    UxActivityTarget, UxNodeId, UxUpdate, UxUpdateSink,
};

const RESPONSE_POLL_LIMIT: usize = 500;
const READINESS_POLL_LIMIT: usize = 500;
const RESPONSE_POLL_DELAY_MS: i32 = 10;
const MALFORMED_PROTOCOL_SNIPPET_LIMIT: usize = 4_096;
const DEVICE_LOG_SNIPPET_LIMIT: usize = 1_024;
const STEP_SERIAL_ACCESS: &str = "serial-access";
const STEP_RESET_DEVICE: &str = "reset-device";
const STEP_BOOT_OUTPUT: &str = "boot-output";
const STEP_PROTOCOL: &str = "server-protocol";

pub struct BrowserSerialClientIo {
    state: Rc<RefCell<BrowserSerialClientState>>,
}

impl BrowserSerialClientIo {
    pub fn new(
        registry: SharedLinkRegistry,
        session_id: LinkSessionId,
        logs: Rc<RefCell<Vec<UiLogEntry>>>,
        updates: UxUpdateSink,
    ) -> Self {
        let readiness_activity = initial_readiness_activity();
        updates.emit(UxUpdate::Activity {
            target: UxActivityTarget::pane(server_node_id()),
            status: UiStatus::working("Connecting"),
            activity: readiness_activity.clone(),
        });
        Self {
            state: Rc::new(RefCell::new(BrowserSerialClientState {
                registry,
                session_id,
                logs,
                updates,
                readiness_activity,
                readiness_classifier: BrowserSerialReadinessClassifier::new(),
                boot_output_seen: false,
                last_request: None,
                last_protocol_issue: None,
                protocol_ready: false,
            })),
        }
    }

    async fn ensure_protocol_ready(&self) -> Result<(), TransportError> {
        if self.state.borrow().protocol_ready {
            return Ok(());
        }
        self.state
            .borrow()
            .emit_readiness_activity(UiStatus::working("Connecting"));

        for _ in 0..READINESS_POLL_LIMIT {
            let (registry, session_id) = {
                let state = self.state.borrow();
                (Rc::clone(&state.registry), state.session_id.clone())
            };

            let (errors, lines) = {
                let mut registry = registry.borrow_mut();
                let provider = browser_serial_provider_mut(&mut registry)?;
                let errors = provider
                    .take_errors(&session_id)
                    .map_err(link_error_to_transport)?;
                let lines = provider
                    .take_lines(&session_id)
                    .map_err(link_error_to_transport)?;
                (errors, lines)
            };

            let mut protocol_ready = false;
            for line in lines {
                if self.handle_line(line)?.is_some() {
                    protocol_ready = true;
                }
                if self.server_started() {
                    protocol_ready = true;
                }
                if let Some(message) = self.detect_readiness_failure() {
                    self.state.borrow_mut().mark_protocol_failed(&message, true);
                    return Err(TransportError::Other(message));
                }
            }

            if let Some(error) = errors.into_iter().next() {
                return Err(self.readiness_error(error));
            }

            if protocol_ready {
                let mut state = self.state.borrow_mut();
                state.protocol_ready = true;
                state.mark_protocol_ready();
                state.push_log(
                    UiLogLevel::Info,
                    "browser-serial",
                    "server protocol stream is ready",
                );
                return Ok(());
            }

            sleep_ms(RESPONSE_POLL_DELAY_MS).await?;
        }

        let failure = self.state.borrow().readiness_classifier.classify_timeout();
        let no_firmware = matches!(
            failure,
            BrowserSerialReadinessFailure::NoFirmwareDetected { .. }
        );
        let message = failure.message();
        self.state
            .borrow_mut()
            .mark_protocol_failed(&message, no_firmware);
        Err(TransportError::Other(message))
    }

    fn detect_readiness_failure(&self) -> Option<String> {
        let state = self.state.borrow();
        if state.readiness_classifier.no_firmware_detected() {
            Some(state.readiness_classifier.classify_timeout().message())
        } else {
            None
        }
    }

    fn server_started(&self) -> bool {
        self.state.borrow().readiness_classifier.server_started()
    }

    fn readiness_error(&self, error: String) -> TransportError {
        let message = format!("browser serial error while waiting for server readiness: {error}");
        if let Some(no_firmware_message) = self.detect_readiness_failure() {
            console_warn(&format!(
                "[browser-serial] {message}; treating as no firmware"
            ));
            self.state
                .borrow()
                .push_log(UiLogLevel::Warn, "browser-serial", message);
            self.state
                .borrow_mut()
                .mark_protocol_failed(&no_firmware_message, true);
            return TransportError::Other(no_firmware_message);
        }

        console_error(&format!("[browser-serial] {message}"));
        self.state
            .borrow()
            .push_log(UiLogLevel::Error, "browser-serial", message.clone());
        self.state
            .borrow_mut()
            .mark_protocol_failed(&message, false);
        TransportError::Other(message)
    }

    fn handle_line(&self, line: String) -> Result<Option<WireServerMessage>, TransportError> {
        if line.is_empty() {
            return Ok(None);
        }

        let Some(json_frame) = line.strip_prefix("M!") else {
            self.record_device_line(&line);
            return Ok(None);
        };

        match json::from_str::<WireServerMessage>(json_frame) {
            Ok(response) => Ok(Some(response)),
            Err(error) => {
                let snippet = line_snippet(json_frame, MALFORMED_PROTOCOL_SNIPPET_LIMIT);
                let issue = format!("{error}; json={snippet}");
                self.record_malformed_frame(issue.clone());
                if let Some(next_frame) = nested_protocol_frame(json_frame) {
                    console_warn(&format!(
                        "[browser-serial] attempting resync at nested M! frame while {}",
                        self.wait_context()
                    ));
                    self.handle_line(next_frame.to_string())
                } else {
                    Ok(None)
                }
            }
        }
    }

    fn record_device_line(&self, line: &str) {
        let level = device_line_level(line);
        let message = line_snippet(line, DEVICE_LOG_SNIPPET_LIMIT);
        log_device_line(level, &message);
        self.state
            .borrow_mut()
            .record_readiness_device_line(level, message);
    }

    fn record_malformed_frame(&self, issue: String) {
        let message = format!("malformed M! frame while {}: {issue}", self.wait_context());
        console_warn(&format!("[browser-serial] {message}"));
        let mut state = self.state.borrow_mut();
        state.last_protocol_issue = Some(issue);
        state.push_log(UiLogLevel::Warn, "browser-serial", message);
    }

    fn wait_context(&self) -> String {
        self.state.borrow().wait_context()
    }
}

#[async_trait(?Send)]
impl ClientIo for BrowserSerialClientIo {
    async fn send(&mut self, msg: ClientMessage) -> Result<(), TransportError> {
        self.ensure_protocol_ready().await?;

        let request_id = msg.id;
        let label = request_label(&msg.msg);
        let frame = json::to_string(&msg)
            .map_err(|error| TransportError::Serialization(error.to_string()))?;

        {
            let mut state = self.state.borrow_mut();
            state.last_request = Some(BrowserSerialRequest {
                id: request_id,
                label,
            });
            state.last_protocol_issue = None;
        }

        console_debug(&format!(
            "[browser-serial] tx request id={request_id} kind={label} json_bytes={}",
            frame.len()
        ));

        let (registry, session_id) = {
            let state = self.state.borrow();
            (Rc::clone(&state.registry), state.session_id.clone())
        };
        let mut registry = registry.borrow_mut();
        let provider = browser_serial_provider_mut(&mut registry)?;
        provider
            .write_line(&session_id, &format!("M!{frame}\n"))
            .await
            .map_err(link_error_to_transport)
    }

    async fn receive(&mut self) -> Result<WireServerMessage, TransportError> {
        for _ in 0..RESPONSE_POLL_LIMIT {
            let (registry, session_id) = {
                let state = self.state.borrow();
                (Rc::clone(&state.registry), state.session_id.clone())
            };

            let (errors, lines) = {
                let mut registry = registry.borrow_mut();
                let provider = browser_serial_provider_mut(&mut registry)?;
                let errors = provider
                    .take_errors(&session_id)
                    .map_err(link_error_to_transport)?;
                let lines = provider
                    .take_lines(&session_id)
                    .map_err(link_error_to_transport)?;
                (errors, lines)
            };

            for error in errors {
                let message = format!(
                    "browser serial error while {}: {error}",
                    self.wait_context()
                );
                console_error(&format!("[browser-serial] {message}"));
                self.state
                    .borrow()
                    .push_log(UiLogLevel::Error, "browser-serial", message.clone());
                return Err(TransportError::Other(message));
            }

            for line in lines {
                if let Some(response) = self.handle_line(line)? {
                    return Ok(response);
                }
            }

            sleep_ms(RESPONSE_POLL_DELAY_MS).await?;
        }

        let mut message = format!(
            "timed out waiting for browser serial protocol response while {}",
            self.wait_context()
        );
        if let Some(issue) = self.state.borrow().last_protocol_issue.clone() {
            message.push_str("; last malformed protocol frame: ");
            message.push_str(&issue);
        }
        Err(TransportError::Other(message))
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        let (registry, session_id) = {
            let state = self.state.borrow();
            (Rc::clone(&state.registry), state.session_id.clone())
        };
        let mut registry = registry.borrow_mut();
        browser_serial_provider_mut(&mut registry)?
            .close(&session_id)
            .await
            .map_err(link_error_to_transport)
    }
}

struct BrowserSerialClientState {
    registry: SharedLinkRegistry,
    session_id: LinkSessionId,
    logs: Rc<RefCell<Vec<UiLogEntry>>>,
    updates: UxUpdateSink,
    readiness_activity: UiActivity,
    readiness_classifier: BrowserSerialReadinessClassifier,
    boot_output_seen: bool,
    last_request: Option<BrowserSerialRequest>,
    last_protocol_issue: Option<String>,
    protocol_ready: bool,
}

impl BrowserSerialClientState {
    fn push_log(&self, level: UiLogLevel, source: impl Into<String>, message: impl Into<String>) {
        self.logs
            .borrow_mut()
            .push(UiLogEntry::new(level, source, message));
    }

    fn record_readiness_device_line(&mut self, level: UiLogLevel, message: String) {
        self.readiness_classifier.observe_line(message.clone());
        let entry = UiLogEntry::new(level, "fw-esp32", message.clone());
        self.logs.borrow_mut().push(entry.clone());
        self.updates.emit(UxUpdate::Log(entry));
        if !self.boot_output_seen {
            self.boot_output_seen = true;
            self.readiness_activity
                .set_step_state(STEP_BOOT_OUTPUT, UiActivityStepState::Complete);
        }
        self.readiness_activity
            .set_step_state(STEP_PROTOCOL, UiActivityStepState::Active);
        self.emit_readiness_activity(UiStatus::working("Connecting"));
    }

    fn mark_protocol_ready(&mut self) {
        self.readiness_activity
            .set_step_state(STEP_BOOT_OUTPUT, UiActivityStepState::Complete);
        self.readiness_activity
            .set_step_state(STEP_PROTOCOL, UiActivityStepState::Complete);
        self.readiness_activity.progress = None;
        self.emit_readiness_activity(UiStatus::good("Connected"));
    }

    fn mark_protocol_failed(&mut self, message: &str, no_firmware: bool) {
        if self.readiness_classifier.recent_lines().is_empty() {
            self.readiness_activity
                .set_step_state(STEP_BOOT_OUTPUT, UiActivityStepState::Failed);
        } else {
            self.readiness_activity
                .set_step_state(STEP_BOOT_OUTPUT, UiActivityStepState::Complete);
        }
        self.readiness_activity
            .set_step_state(STEP_PROTOCOL, UiActivityStepState::Failed);
        self.readiness_activity.detail = Some(message.to_string());
        self.readiness_activity.progress = None;
        let status = if no_firmware {
            UiStatus::warning("Flash ready")
        } else {
            UiStatus::error("Timeout")
        };
        self.emit_readiness_activity(status);
    }

    fn emit_readiness_activity(&self, status: UiStatus) {
        self.updates.emit(UxUpdate::Activity {
            target: UxActivityTarget::pane(server_node_id()),
            status,
            activity: self.readiness_activity.clone(),
        });
    }

    fn wait_context(&self) -> String {
        match self.last_request {
            Some(request) => {
                format!(
                    "waiting for response id={} kind={}",
                    request.id, request.label
                )
            }
            None => "waiting for a protocol response".to_string(),
        }
    }
}

fn initial_readiness_activity() -> UiActivity {
    UiActivity::new("Connecting ESP32 server")
        .with_detail("Waiting for LightPlayer boot output and protocol frames.")
        .with_steps(vec![
            UiActivityStep::new(STEP_SERIAL_ACCESS, "Serial access")
                .with_state(UiActivityStepState::Complete)
                .with_detail("Browser serial port is open."),
            UiActivityStep::new(STEP_RESET_DEVICE, "Reset device")
                .with_state(UiActivityStepState::Complete)
                .with_detail("Device reset was requested while serial output was being read."),
            UiActivityStep::new(STEP_BOOT_OUTPUT, "Boot output")
                .with_state(UiActivityStepState::Active),
            UiActivityStep::new(STEP_PROTOCOL, "LightPlayer protocol")
                .with_state(UiActivityStepState::Active),
        ])
}

fn server_node_id() -> UxNodeId {
    UxNodeId::new(ServerController::NODE_ID)
}

#[derive(Clone, Copy)]
struct BrowserSerialRequest {
    id: u64,
    label: &'static str,
}

fn browser_serial_provider_mut(
    registry: &mut LinkProviderRegistry,
) -> Result<&mut BrowserSerialEsp32Provider, TransportError> {
    match registry.provider_mut(LinkProviderKind::BrowserSerialEsp32) {
        Some(LinkProviderInstance::BrowserSerialEsp32(provider)) => Ok(provider),
        Some(_) => Err(TransportError::Other(
            "browser-serial-esp32 registry entry has the wrong provider type".to_string(),
        )),
        None => Err(TransportError::Other(
            "browser-serial-esp32 provider is not available".to_string(),
        )),
    }
}

fn link_error_to_transport(error: lpa_link::LinkError) -> TransportError {
    TransportError::Other(error.to_string())
}

fn device_line_level(line: &str) -> UiLogLevel {
    if line.starts_with("[ERROR]") {
        UiLogLevel::Error
    } else if line.starts_with("[WARN]") {
        UiLogLevel::Warn
    } else if line.starts_with("[DEBUG]") || line.starts_with("[TRACE]") {
        UiLogLevel::Debug
    } else {
        UiLogLevel::Info
    }
}

fn log_device_line(level: UiLogLevel, message: &str) {
    let message = format!("[fw-esp32] {message}");
    match level {
        UiLogLevel::Error => console_error(&message),
        UiLogLevel::Warn => console_warn(&message),
        UiLogLevel::Debug => console_debug(&message),
        UiLogLevel::Info => console_log(&message),
    }
}

fn nested_protocol_frame(json_frame: &str) -> Option<&str> {
    json_frame
        .find("M!")
        .filter(|offset| *offset > 0)
        .map(|offset| &json_frame[offset..])
}

fn line_snippet(line: &str, max_len: usize) -> String {
    let mut output = String::new();
    for c in line.chars() {
        if output.len() >= max_len {
            output.push_str("...");
            break;
        }
        for escaped in c.escape_default() {
            output.push(escaped);
        }
    }
    output
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

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console, js_name = log)]
    fn console_log(message: &str);

    #[wasm_bindgen(js_namespace = console, js_name = debug)]
    fn console_debug(message: &str);

    #[wasm_bindgen(js_namespace = console, js_name = warn)]
    fn console_warn(message: &str);

    #[wasm_bindgen(js_namespace = console, js_name = error)]
    fn console_error(message: &str);
}
