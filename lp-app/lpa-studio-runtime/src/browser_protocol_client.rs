use js_sys::Promise;
use lpc_wire::{
    ClientRequest, WireProjectCommandResponse, WireServerMessage, WireServerMsgBody, json,
    messages::ClientMessage,
};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

use lpa_studio_core::{ProjectStateResult, StudioEffect, StudioEvent};

use crate::browser_worker_runtime::BrowserWorkerHandle;
use crate::protocol_event::{inventory_request, server_event};
use crate::worker_envelope::{BrowserInputEnvelope, BrowserOutputEnvelope};
use crate::{StudioRuntimeError, demo_project};

pub struct BrowserProtocolClient {
    runtime: BrowserWorkerHandle,
    next_request_id: u64,
}

impl BrowserProtocolClient {
    pub(crate) fn new(runtime: BrowserWorkerHandle) -> Self {
        Self {
            runtime,
            next_request_id: 1,
        }
    }

    pub fn close(&mut self) {
        self.runtime.terminate();
    }

    pub async fn seed_demo_project(
        &mut self,
        action_id: lpa_studio_core::ActionId,
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
        action_id: lpa_studio_core::ActionId,
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
        action_id: lpa_studio_core::ActionId,
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

    async fn read_project_state(
        &mut self,
        action_id: lpa_studio_core::ActionId,
    ) -> Result<Vec<StudioEvent>, StudioRuntimeError> {
        let exchange = self.send_request(ClientRequest::ListLoadedProjects).await?;
        let mut events = exchange.events;
        if let WireServerMsgBody::ListLoadedProjects { projects } = exchange.response.msg {
            events.push(StudioEvent::ProjectStateRead {
                action_id,
                result: ProjectStateResult::from_loaded_projects(projects),
            });
        }
        Ok(events)
    }

    async fn send_request(
        &mut self,
        request: ClientRequest,
    ) -> Result<BrowserExchange, StudioRuntimeError> {
        let request_id = self.next_request_id();
        let frame = json::to_string(&ClientMessage {
            id: request_id,
            msg: request,
        })
        .map_err(|error| StudioRuntimeError::Protocol(error.to_string()))?;
        self.runtime
            .post(&BrowserInputEnvelope::ProtocolIn { frame })?;

        let mut events = Vec::new();
        for _ in 0..240 {
            self.runtime
                .post(&BrowserInputEnvelope::Tick { delta_ms: Some(16) })?;
            sleep_ms(4).await?;
            for output in self.runtime.take_outputs() {
                match output {
                    BrowserOutputEnvelope::ProtocolOut { frame } => {
                        let response: WireServerMessage = json::from_str(&frame)
                            .map_err(|error| StudioRuntimeError::Protocol(error.to_string()))?;
                        if response.id == request_id {
                            return Ok(BrowserExchange { response, events });
                        }
                        if response.id == 0 {
                            if let Some(event) = server_event(response) {
                                events.push(event);
                            }
                        }
                    }
                    output => {
                        if let Some(event) = worker_output_to_event(output) {
                            events.push(event);
                        }
                    }
                }
            }
        }
        Err(StudioRuntimeError::Browser(
            "timed out waiting for worker protocol response".to_string(),
        ))
    }

    fn next_request_id(&mut self) -> u64 {
        let id = self.next_request_id;
        self.next_request_id += 1;
        id
    }
}

pub struct BrowserExchange {
    pub response: WireServerMessage,
    pub events: Vec<StudioEvent>,
}

pub async fn sleep_ms(ms: i32) -> Result<(), StudioRuntimeError> {
    let promise = Promise::new(&mut |resolve: js_sys::Function, reject: js_sys::Function| {
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
        .map_err(|error| StudioRuntimeError::Browser(format!("{error:?}")))
}

fn worker_output_to_event(output: BrowserOutputEnvelope) -> Option<StudioEvent> {
    match output {
        BrowserOutputEnvelope::Status {
            status, message, ..
        } => Some(StudioEvent::LogReceived {
            entry: lpa_studio_core::StudioLogEntry::new(
                lpa_studio_core::StudioLogLevel::Info,
                "fw-browser",
                message.unwrap_or(status),
            ),
        }),
        BrowserOutputEnvelope::Log {
            level,
            target,
            message,
            ..
        } => Some(StudioEvent::LogReceived {
            entry: lpa_studio_core::StudioLogEntry::new(
                parse_worker_log_level(&level),
                target,
                message,
            ),
        }),
        BrowserOutputEnvelope::ProtocolOut { .. } => None,
    }
}

fn parse_worker_log_level(level: &str) -> lpa_studio_core::StudioLogLevel {
    match level {
        "trace" => lpa_studio_core::StudioLogLevel::Trace,
        "debug" => lpa_studio_core::StudioLogLevel::Debug,
        "warn" => lpa_studio_core::StudioLogLevel::Warn,
        "error" => lpa_studio_core::StudioLogLevel::Error,
        _ => lpa_studio_core::StudioLogLevel::Info,
    }
}
