use lpc_wire::{
    ClientRequest, WireProjectCommandResponse, WireServerMessage, WireServerMsgBody, json,
    messages::ClientMessage,
};
use wasm_bindgen::JsValue;

use lp_studio_core::{StudioEffect, StudioEvent, StudioLogEntry, StudioLogLevel};

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
        let frame = json::to_string(&ClientMessage {
            id: request_id,
            msg: request,
        })
        .map_err(|error| StudioRuntimeError::Protocol(error.to_string()))?;
        browser_serial_shim::write_line(self.port_id, &format!("M!{frame}\n")).await?;

        let mut events = Vec::new();
        for _ in 0..600 {
            events.extend(self.take_link_errors());
            for line in browser_serial_shim::take_lines(self.port_id) {
                if let Some(response) = self.handle_line(line, request_id, &mut events)? {
                    if let WireServerMsgBody::Error { error } = &response.msg {
                        return Err(StudioRuntimeError::Protocol(error.clone()));
                    }
                    return Ok(BrowserSerialExchange { response, events });
                }
            }
            sleep_ms(10).await?;
        }
        Err(StudioRuntimeError::Transport(
            "timed out waiting for browser serial protocol response".to_string(),
        ))
    }

    fn handle_line(
        &self,
        line: String,
        request_id: u64,
        events: &mut Vec<StudioEvent>,
    ) -> Result<Option<WireServerMessage>, StudioRuntimeError> {
        let Some(json_frame) = line.strip_prefix("M!") else {
            events.push(StudioEvent::LogReceived {
                entry: StudioLogEntry::new(StudioLogLevel::Info, "fw-esp32", line),
            });
            return Ok(None);
        };

        let response = json::from_str::<WireServerMessage>(json_frame)
            .map_err(|error| StudioRuntimeError::Protocol(error.to_string()))?;
        if response.id == request_id {
            return Ok(Some(response));
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
        Ok(None)
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
