use std::cell::RefCell;
use std::rc::Rc;

use async_trait::async_trait;
use lpa_client::ClientIo;
use lpa_client::project_deploy::request_label;
use lpa_link::provider::session::LinkSessionId;
use lpa_link::providers::browser_serial_esp32::BrowserSerialEsp32Provider;
use lpa_link::providers::{LinkProviderInstance, LinkProviderRegistry};
use lpa_link::{LinkProvider, LinkProviderKind};
use lpc_wire::{ClientMessage, TransportError, WireServerMessage, json};
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::JsFuture;

use super::browser_serial_readiness::{
    BrowserSerialReadinessClassifier, BrowserSerialReadinessFailure,
};
use super::device_log_line::parse_device_log_line;
use super::pending_server_messages::{BatchItem, PendingServerMessages};
use crate::core::view::activity_view::{UiActivityStep, UiActivityStepState};
use crate::{
    ControllerId, ServerController, SharedLinkRegistry, UiActivityView, UiLogDraft, UiLogLevel,
    UiLogOrigin, UiLogSource, UiStatus, UxActivityTarget, UxUpdate, UxUpdateSink,
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
    pending: PendingServerMessages<WireServerMessage>,
}

impl BrowserSerialClientIo {
    pub fn new(
        registry: SharedLinkRegistry,
        session_id: LinkSessionId,
        logs: Rc<RefCell<Vec<UiLogDraft>>>,
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
            pending: PendingServerMessages::new(),
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
                // Pure diagnostic (P4): rides the global `log::` sink into the
                // ring (origin Studio, module target as detail).
                log::info!("server protocol stream is ready");
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
        self.state
            .borrow()
            .push_log(UiLogLevel::Warn, message.clone());
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
            // Pure diagnostic (P4, was a direct console_warn): the buffered
            // Warn draft below carries the error itself.
            log::debug!("treating readiness error as no firmware: {message}");
            self.state.borrow().push_log(UiLogLevel::Warn, message);
            self.state
                .borrow_mut()
                .mark_protocol_failed(&no_firmware_message, true);
            return TransportError::Other(no_firmware_message);
        }

        self.state
            .borrow()
            .push_log(UiLogLevel::Error, message.clone());
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
                    // Pure diagnostic (P4, was a direct console_warn).
                    log::warn!(
                        "attempting resync at nested M! frame while {}",
                        self.wait_context()
                    );
                    self.handle_line(next_frame.to_string())
                } else {
                    Ok(None)
                }
            }
        }
    }

    /// Record a non-protocol serial line: parse the firmware logger format
    /// into a structured draft (level, module path as detail, message
    /// remainder) and feed the full raw line to the readiness classifier
    /// (which matches boot-ROM and server-start markers against unstripped
    /// text). JS-console mirroring happens once, structurally, when the draft
    /// enters the controller ring (the web shell's `on_entry` hook, P4) — the
    /// old raw-line `log_device_line` mirror is gone.
    fn record_device_line(&self, line: &str) {
        let parsed = parse_device_log_line(line);
        let raw_line = line_snippet(line, DEVICE_LOG_SNIPPET_LIMIT);
        let draft = UiLogDraft::new(
            parsed.level,
            match parsed.module {
                Some(module) => UiLogSource::with_detail(UiLogOrigin::Device, module),
                None => UiLogSource::new(UiLogOrigin::Device),
            },
            line_snippet(parsed.message, DEVICE_LOG_SNIPPET_LIMIT),
        );
        self.state
            .borrow_mut()
            .record_readiness_device_line(draft, raw_line);
    }

    fn record_malformed_frame(&self, issue: String) {
        let message = format!("malformed M! frame while {}: {issue}", self.wait_context());
        let mut state = self.state.borrow_mut();
        state.last_protocol_issue = Some(issue);
        state.push_log(UiLogLevel::Warn, message);
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

        // Pure diagnostic transport chatter (P4): migrated from a hand-built
        // Debug draft + duplicate console_debug to one `log::` record.
        log::debug!(
            "tx request id={request_id} kind={label} json_bytes={}",
            frame.len()
        );

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
        if let Some(response) = self.pending.pop() {
            return Ok(response);
        }

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
                self.state
                    .borrow()
                    .push_log(UiLogLevel::Error, message.clone());
                return Err(TransportError::Other(message));
            }

            // Decode the whole drained batch: every `M!` frame is queued in
            // order (a single `take_lines()` window can carry several 16 KiB
            // project-read frames back to back), while device/log lines keep
            // their existing handling inside `handle_line`. `pending` is moved
            // out so the classifier can borrow `self` (for `handle_line`)
            // without overlapping the `&mut self.pending` borrow.
            let mut pending = std::mem::take(&mut self.pending);
            let outcome = pending.ingest(lines, |line| {
                self.handle_line(line).map(|decoded| match decoded {
                    Some(response) => BatchItem::Protocol(response),
                    None => BatchItem::Other,
                })
            });
            self.pending = pending;
            outcome?;

            if let Some(response) = self.pending.pop() {
                return Ok(response);
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
        self.state
            .borrow()
            .push_log(UiLogLevel::Warn, message.clone());
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
    logs: Rc<RefCell<Vec<UiLogDraft>>>,
    updates: UxUpdateSink,
    readiness_activity: UiActivityView,
    readiness_classifier: BrowserSerialReadinessClassifier,
    boot_output_seen: bool,
    last_request: Option<BrowserSerialRequest>,
    last_protocol_issue: Option<String>,
    protocol_ready: bool,
}

impl BrowserSerialClientState {
    /// Buffer a transport-level draft (origin `Link`, detail `browser-serial`)
    /// and mirror it as a progressive `UxUpdate::Log`. Drafts are stamped by
    /// the controller when it drains the buffer.
    fn push_log(&self, level: UiLogLevel, message: impl Into<String>) {
        let draft = UiLogDraft::new(
            level,
            UiLogSource::with_detail(UiLogOrigin::Link, "browser-serial"),
            message,
        );
        self.logs.borrow_mut().push(draft.clone());
        self.updates.emit(UxUpdate::Log(draft));
    }

    /// Buffer a parsed device-line draft and advance the readiness activity.
    /// The classifier observes `raw_line` (the unstripped serial line) so its
    /// substring markers keep matching regardless of log-prefix parsing.
    fn record_readiness_device_line(&mut self, draft: UiLogDraft, raw_line: String) {
        self.readiness_classifier.observe_line(raw_line);
        self.logs.borrow_mut().push(draft.clone());
        self.updates.emit(UxUpdate::Log(draft));
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

fn initial_readiness_activity() -> UiActivityView {
    UiActivityView::new("Connecting ESP32 server")
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

fn server_node_id() -> ControllerId {
    ControllerId::new(ServerController::NODE_ID)
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
