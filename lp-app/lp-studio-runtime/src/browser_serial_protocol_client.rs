use lpc_wire::{
    ClientRequest, FsRequest, WireProjectCommandResponse, WireServerMessage, WireServerMsgBody,
    json, messages::ClientMessage,
};
use wasm_bindgen::{JsValue, prelude::wasm_bindgen};

use lp_studio_core::{StudioDiagnostic, StudioEffect, StudioEvent, StudioLogEntry, StudioLogLevel};

use crate::browser_serial_shim;
use crate::protocol_event::{inventory_request, server_event};
use crate::{StudioRuntimeError, demo_project};

pub struct BrowserSerialProtocolClient {
    port_id: u32,
    next_request_id: u64,
}

impl BrowserSerialProtocolClient {
    pub fn new(port_id: u32) -> Self {
        Self {
            port_id,
            next_request_id: 1,
        }
    }

    pub fn port_id(&self) -> u32 {
        self.port_id
    }

    pub async fn seed_demo_project(
        &mut self,
        action_id: lp_studio_core::ActionId,
        project_id: &str,
    ) -> Result<Vec<StudioEvent>, StudioRuntimeError> {
        let mut events = Vec::new();
        for request in demo_project::demo_write_requests(project_id) {
            let response = self.send_request(request).await?;
            events.extend(response.events);
            demo_project::ensure_write_response(&response.response.msg)
                .map_err(StudioRuntimeError::Protocol)?;
        }
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
            _ => Ok(Vec::new()),
        }
    }

    async fn load_project(
        &mut self,
        action_id: lp_studio_core::ActionId,
        project_id: &str,
    ) -> Result<Vec<StudioEvent>, StudioRuntimeError> {
        let exchange = self
            .send_request(ClientRequest::LoadProject {
                path: project_id.to_string(),
            })
            .await?;
        let mut events = exchange.events;
        match exchange.response.msg {
            WireServerMsgBody::LoadProject { handle } => {
                events.push(StudioEvent::ProjectLoaded {
                    action_id,
                    project_id: project_id.to_string(),
                    handle,
                });
                Ok(events)
            }
            other => Err(StudioRuntimeError::Protocol(format!(
                "unexpected load project response: {other:?}"
            ))),
        }
    }

    async fn read_inventory(
        &mut self,
        action_id: lp_studio_core::ActionId,
        handle: lpc_wire::WireProjectHandle,
    ) -> Result<Vec<StudioEvent>, StudioRuntimeError> {
        let exchange = self.send_request(inventory_request(handle)).await?;
        let mut events = exchange.events;
        match exchange.response.msg {
            WireServerMsgBody::ProjectCommand {
                response:
                    WireProjectCommandResponse::ReadInventory {
                        response: inventory,
                    },
            } => {
                events.push(StudioEvent::ProjectInventoryRead {
                    action_id,
                    inventory,
                });
                Ok(events)
            }
            other => Err(StudioRuntimeError::Protocol(format!(
                "unexpected inventory response: {other:?}"
            ))),
        }
    }

    async fn refresh_loaded_projects(
        &mut self,
        action_id: lp_studio_core::ActionId,
    ) -> Result<Vec<StudioEvent>, StudioRuntimeError> {
        let exchange = self.send_request(ClientRequest::ListLoadedProjects).await?;
        let mut events = exchange.events;
        if let WireServerMsgBody::ListLoadedProjects { projects } = exchange.response.msg {
            events.push(StudioEvent::LoadedProjectsRefreshed {
                action_id,
                projects,
            });
        }
        Ok(events)
    }

    async fn send_request(
        &mut self,
        request: ClientRequest,
    ) -> Result<BrowserSerialExchange, StudioRuntimeError> {
        let request_id = self.next_request_id();
        let request_label = request_label(&request);
        let frame = json::to_string(&ClientMessage {
            id: request_id,
            msg: request,
        })
        .map_err(|error| StudioRuntimeError::Protocol(error.to_string()))?;
        let mut events = vec![StudioEvent::LogReceived {
            entry: StudioLogEntry::new(
                StudioLogLevel::Debug,
                "browser-serial",
                format!(
                    "tx request id={request_id} kind={request_label} json_bytes={}",
                    frame.len()
                ),
            ),
        }];
        browser_serial_shim::write_line(self.port_id, &format!("M!{frame}\n")).await?;

        let mut last_protocol_issue = None;
        for _ in 0..600 {
            events.extend(self.take_link_errors());
            for line in browser_serial_shim::take_lines(self.port_id) {
                if let Some(response) =
                    self.handle_line(line, request_id, &mut events, &mut last_protocol_issue)
                {
                    if let WireServerMsgBody::Error { error } = &response.msg {
                        return Err(StudioRuntimeError::Protocol(error.clone()));
                    }
                    return Ok(BrowserSerialExchange { response, events });
                }
            }
            sleep_ms(10).await?;
        }
        let mut message = format!(
            "timed out waiting for browser serial protocol response id={request_id} kind={request_label}"
        );
        if let Some(issue) = last_protocol_issue {
            message.push_str("; last malformed protocol frame: ");
            message.push_str(&issue);
        }
        Err(StudioRuntimeError::Transport(message))
    }

    fn handle_line(
        &self,
        line: String,
        request_id: u64,
        events: &mut Vec<StudioEvent>,
        last_protocol_issue: &mut Option<String>,
    ) -> Option<WireServerMessage> {
        if line.is_empty() {
            return None;
        }

        let Some(json_frame) = line.strip_prefix("M!") else {
            echo_device_line(&line);
            return None;
        };

        let response = match json::from_str::<WireServerMessage>(json_frame) {
            Ok(response) => response,
            Err(error) => {
                let snippet = line_snippet(json_frame, 240);
                let issue = format!("{error}; json={snippet}");
                *last_protocol_issue = Some(issue.clone());
                let message = format!(
                    "malformed M! frame while waiting for response id={request_id}: {issue}"
                );
                console_warn(&format!("[browser-serial] {message}"));
                events.push(StudioEvent::LogReceived {
                    entry: StudioLogEntry::new(StudioLogLevel::Warn, "browser-serial", &message),
                });
                events.push(StudioEvent::DiagnosticRaised {
                    diagnostic: StudioDiagnostic::error(None, message),
                });
                if let Some(next_frame) = nested_protocol_frame(json_frame) {
                    console_warn(&format!(
                        "[browser-serial] attempting resync at nested M! frame while waiting for response id={request_id}"
                    ));
                    return self.handle_line(
                        next_frame.to_string(),
                        request_id,
                        events,
                        last_protocol_issue,
                    );
                }
                return None;
            }
        };
        if response.id == request_id {
            return Some(response);
        }
        if response.id == 0 {
            if let Some(event) = server_event(response) {
                events.push(event);
            }
        } else {
            events.push(StudioEvent::LogReceived {
                entry: StudioLogEntry::new(
                    StudioLogLevel::Warn,
                    "lp-studio-runtime",
                    format!(
                        "Ignoring uncorrelated serial response id={} while waiting for id={request_id}",
                        response.id
                    ),
                ),
            });
        }
        None
    }

    fn take_link_errors(&self) -> Vec<StudioEvent> {
        browser_serial_shim::take_errors(self.port_id)
            .into_iter()
            .map(|message| StudioEvent::LogReceived {
                entry: StudioLogEntry::new(StudioLogLevel::Error, "browser-serial", message),
            })
            .collect()
    }

    fn next_request_id(&mut self) -> u64 {
        let id = self.next_request_id;
        self.next_request_id += 1;
        id
    }
}

fn request_label(request: &ClientRequest) -> &'static str {
    match request {
        ClientRequest::Filesystem(FsRequest::Read { .. }) => "fs.read",
        ClientRequest::Filesystem(FsRequest::Write { .. }) => "fs.write",
        ClientRequest::Filesystem(FsRequest::DeleteFile { .. }) => "fs.delete_file",
        ClientRequest::Filesystem(FsRequest::DeleteDir { .. }) => "fs.delete_dir",
        ClientRequest::Filesystem(FsRequest::ListDir { .. }) => "fs.list_dir",
        ClientRequest::LoadProject { .. } => "project.load",
        ClientRequest::UnloadProject { .. } => "project.unload",
        ClientRequest::ProjectRequest { .. } => "project.read",
        ClientRequest::ProjectCommand { .. } => "project.command",
        ClientRequest::ListAvailableProjects => "project.list_available",
        ClientRequest::ListLoadedProjects => "project.list_loaded",
        ClientRequest::StopAllProjects => "project.stop_all",
    }
}

fn echo_device_line(line: &str) {
    let message = format!("[fw-esp32] {}", line_snippet(line, 1_024));
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
    json_frame.find("M!").map(|offset| &json_frame[offset..])
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

pub struct BrowserSerialExchange {
    pub response: WireServerMessage,
    pub events: Vec<StudioEvent>,
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
