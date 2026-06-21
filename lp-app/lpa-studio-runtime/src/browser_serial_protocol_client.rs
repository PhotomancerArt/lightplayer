use std::cell::RefCell;
use std::rc::Rc;

use lpa_client::project_deploy::{project_load_path, request_label};
use lpa_client::{ClientError, ClientEvent, ClientIo, ClientOutcome, LpClient};
use lpa_link::LinkProvider;
use lpa_link::link_session::LinkSessionId;
use lpa_link::providers::browser_serial_esp32::BrowserSerialEsp32Provider;
use lpc_wire::{ClientMessage, TransportError, WireServerMessage, json};
use wasm_bindgen::{JsValue, prelude::wasm_bindgen};

use lpa_studio_core::{
    ProjectStateResult, StudioDiagnostic, StudioEffect, StudioEvent, StudioLogEntry, StudioLogLevel,
};

use crate::protocol_event::client_event;
use crate::{StudioRuntimeError, demo_project};

const MALFORMED_PROTOCOL_SNIPPET_LIMIT: usize = 4_096;
const DEVICE_LOG_SNIPPET_LIMIT: usize = 1_024;

pub struct BrowserSerialProtocolClient {
    client: LpClient<BrowserSerialClientIo>,
    io_state: Rc<RefCell<BrowserSerialClientState>>,
}

impl BrowserSerialProtocolClient {
    pub fn new(
        provider: Rc<RefCell<BrowserSerialEsp32Provider>>,
        session_id: LinkSessionId,
    ) -> Self {
        let io_state = Rc::new(RefCell::new(BrowserSerialClientState::new(
            provider, session_id,
        )));
        Self {
            client: LpClient::new(BrowserSerialClientIo::new(Rc::clone(&io_state))),
            io_state,
        }
    }

    pub async fn probe_server(&mut self) -> Result<Vec<StudioEvent>, StudioRuntimeError> {
        let outcome = self
            .client
            .project_list_loaded()
            .await
            .map_err(map_client_error)?;
        Ok(self.studio_events(outcome.events))
    }

    pub async fn seed_demo_project(
        &mut self,
        action_id: lpa_studio_core::ActionId,
        project_id: &str,
    ) -> Result<Vec<StudioEvent>, StudioRuntimeError> {
        let mut events = Vec::new();

        let stop = self
            .client
            .stop_all_projects()
            .await
            .map_err(map_client_error)?;
        events.extend(self.studio_events(stop.events));

        let push = self
            .client
            .push_project_files(project_id, demo_project::demo_project_deploy_files())
            .await
            .map_err(map_client_error)?;
        events.extend(self.studio_events(push.events));

        events.push(StudioEvent::DemoProjectSeeded {
            action_id,
            project_id: project_id.to_string(),
        });
        Ok(events)
    }

    pub async fn execute_project_effect(
        &mut self,
        effect: StudioEffect,
    ) -> Result<Vec<StudioEvent>, StudioRuntimeError> {
        match effect {
            StudioEffect::LoadProject {
                action_id,
                project_id,
            } => self.load_project(action_id, &project_id).await,
            StudioEffect::ReadProjectInventory { action_id, handle } => {
                self.read_inventory(action_id, handle).await
            }
            StudioEffect::RefreshStatus { action_id } => {
                self.refresh_loaded_projects(action_id).await
            }
            StudioEffect::ReadProjectState { action_id } => {
                self.read_project_state(action_id).await
            }
            _ => Ok(Vec::new()),
        }
    }

    async fn load_project(
        &mut self,
        action_id: lpa_studio_core::ActionId,
        project_id: &str,
    ) -> Result<Vec<StudioEvent>, StudioRuntimeError> {
        let outcome = self
            .client
            .project_load(&project_load_path(project_id))
            .await
            .map_err(map_client_error)?;
        let (handle, mut events) = self.split_outcome(outcome);
        events.push(StudioEvent::ProjectLoaded {
            action_id,
            project_id: project_id.to_string(),
            handle,
        });
        Ok(events)
    }

    async fn read_inventory(
        &mut self,
        action_id: lpa_studio_core::ActionId,
        handle: lpc_wire::WireProjectHandle,
    ) -> Result<Vec<StudioEvent>, StudioRuntimeError> {
        let outcome = self
            .client
            .project_inventory_read(handle)
            .await
            .map_err(map_client_error)?;
        let (inventory, mut events) = self.split_outcome(outcome);
        events.push(StudioEvent::ProjectInventoryRead {
            action_id,
            inventory,
        });
        Ok(events)
    }

    async fn refresh_loaded_projects(
        &mut self,
        action_id: lpa_studio_core::ActionId,
    ) -> Result<Vec<StudioEvent>, StudioRuntimeError> {
        let outcome = self
            .client
            .project_list_loaded()
            .await
            .map_err(map_client_error)?;
        let (projects, mut events) = self.split_outcome(outcome);
        events.push(StudioEvent::LoadedProjectsRefreshed {
            action_id,
            projects,
        });
        Ok(events)
    }

    async fn read_project_state(
        &mut self,
        action_id: lpa_studio_core::ActionId,
    ) -> Result<Vec<StudioEvent>, StudioRuntimeError> {
        let outcome = self
            .client
            .project_list_loaded()
            .await
            .map_err(map_client_error)?;
        let (projects, mut events) = self.split_outcome(outcome);
        events.push(StudioEvent::ProjectStateRead {
            action_id,
            result: ProjectStateResult::from_loaded_projects(projects),
        });
        Ok(events)
    }

    fn split_outcome<T>(&self, outcome: ClientOutcome<T>) -> (T, Vec<StudioEvent>) {
        (outcome.value, self.studio_events(outcome.events))
    }

    fn studio_events(&self, client_events: Vec<ClientEvent>) -> Vec<StudioEvent> {
        let mut events = self.io_state.borrow_mut().take_events();
        events.extend(client_events.into_iter().map(client_event));
        events
    }
}

struct BrowserSerialClientIo {
    state: Rc<RefCell<BrowserSerialClientState>>,
}

impl BrowserSerialClientIo {
    fn new(state: Rc<RefCell<BrowserSerialClientState>>) -> Self {
        Self { state }
    }

    fn handle_line(&self, line: String) -> Result<Option<WireServerMessage>, TransportError> {
        if line.is_empty() {
            return Ok(None);
        }

        let Some(json_frame) = line.strip_prefix("M!") else {
            echo_device_line(&line);
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

    fn record_malformed_frame(&self, issue: String) {
        let message = format!("malformed M! frame while {}: {issue}", self.wait_context());
        console_warn(&format!("[browser-serial] {message}"));

        let mut state = self.state.borrow_mut();
        state.last_protocol_issue = Some(issue);
        state.push_event(StudioEvent::LogReceived {
            entry: StudioLogEntry::new(StudioLogLevel::Warn, "browser-serial", &message),
        });
        state.push_event(StudioEvent::DiagnosticRaised {
            diagnostic: StudioDiagnostic::error(None, message),
        });
    }

    fn wait_context(&self) -> String {
        self.state.borrow().wait_context()
    }
}

#[async_trait::async_trait(?Send)]
impl ClientIo for BrowserSerialClientIo {
    async fn send(&mut self, msg: ClientMessage) -> Result<(), TransportError> {
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
        let (provider, session_id) = {
            let state = self.state.borrow();
            (Rc::clone(&state.provider), state.session_id.clone())
        };
        provider
            .borrow()
            .write_line(&session_id, &format!("M!{frame}\n"))
            .await
            .map_err(link_error_to_transport)
    }

    async fn receive(&mut self) -> Result<WireServerMessage, TransportError> {
        for _ in 0..600 {
            let (provider, session_id) = {
                let state = self.state.borrow();
                (Rc::clone(&state.provider), state.session_id.clone())
            };

            for error in provider
                .borrow()
                .take_errors(&session_id)
                .map_err(link_error_to_transport)?
            {
                let message = format!(
                    "browser serial error while {}: {error}",
                    self.wait_context()
                );
                console_error(&format!("[browser-serial] {message}"));
                self.state
                    .borrow_mut()
                    .push_event(StudioEvent::LogReceived {
                        entry: StudioLogEntry::new(
                            StudioLogLevel::Error,
                            "browser-serial",
                            &message,
                        ),
                    });
                return Err(TransportError::Other(message));
            }

            for line in provider
                .borrow()
                .take_lines(&session_id)
                .map_err(link_error_to_transport)?
            {
                if let Some(response) = self.handle_line(line)? {
                    return Ok(response);
                }
            }

            sleep_ms(10).await.map_err(studio_error_to_transport)?;
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
        let (provider, session_id) = {
            let state = self.state.borrow();
            (Rc::clone(&state.provider), state.session_id.clone())
        };
        provider
            .borrow_mut()
            .close(&session_id)
            .await
            .map_err(link_error_to_transport)
    }
}

struct BrowserSerialClientState {
    provider: Rc<RefCell<BrowserSerialEsp32Provider>>,
    session_id: LinkSessionId,
    last_request: Option<BrowserSerialRequest>,
    last_protocol_issue: Option<String>,
    pending_events: Vec<StudioEvent>,
}

impl BrowserSerialClientState {
    fn new(provider: Rc<RefCell<BrowserSerialEsp32Provider>>, session_id: LinkSessionId) -> Self {
        Self {
            provider,
            session_id,
            last_request: None,
            last_protocol_issue: None,
            pending_events: Vec::new(),
        }
    }

    fn push_event(&mut self, event: StudioEvent) {
        self.pending_events.push(event);
    }

    fn take_events(&mut self) -> Vec<StudioEvent> {
        core::mem::take(&mut self.pending_events)
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

#[derive(Clone, Copy)]
struct BrowserSerialRequest {
    id: u64,
    label: &'static str,
}

fn map_client_error(error: ClientError) -> StudioRuntimeError {
    match error {
        ClientError::Transport(message) => StudioRuntimeError::Transport(message),
        ClientError::Server(message) | ClientError::Protocol(message) => {
            StudioRuntimeError::Protocol(message)
        }
        error @ ClientError::UnexpectedResponse { .. } => {
            StudioRuntimeError::Protocol(error.to_string())
        }
    }
}

fn studio_error_to_transport(error: StudioRuntimeError) -> TransportError {
    TransportError::Other(error.to_string())
}

fn link_error_to_transport(error: lpa_link::LinkError) -> TransportError {
    TransportError::Other(error.to_string())
}

fn echo_device_line(line: &str) {
    let message = format!(
        "[fw-esp32] {}",
        line_snippet(line, DEVICE_LOG_SNIPPET_LIMIT)
    );
    if line.starts_with("[ERROR]") {
        console_error(&message);
    } else if line.starts_with("[WARN]") {
        console_warn(&message);
    } else if line.starts_with("[DEBUG]") || line.starts_with("[TRACE]") {
        console_debug(&message);
    } else {
        console_log(&message);
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

pub async fn sleep_ms(ms: i32) -> Result<(), StudioRuntimeError> {
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
    wasm_bindgen_futures::JsFuture::from(promise)
        .await
        .map(|_| ())
        .map_err(|error| StudioRuntimeError::Browser(format!("{error:?}")))
}
