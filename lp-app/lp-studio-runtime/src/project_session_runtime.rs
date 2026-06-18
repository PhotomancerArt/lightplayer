use lpc_wire::{ClientRequest, WireProjectCommandResponse, WireServerMsgBody};

use lp_studio_core::{StudioEvent, StudioLogEntry, StudioLogLevel};

use crate::client_session_runtime::ClientSessionRuntime;
use crate::protocol_event::inventory_request;
use crate::{StudioRuntimeError, demo_project};

pub struct ProjectSessionRuntime<'a> {
    client: &'a mut ClientSessionRuntime,
}

impl<'a> ProjectSessionRuntime<'a> {
    pub fn new(client: &'a mut ClientSessionRuntime) -> Self {
        Self { client }
    }

    pub async fn seed_demo_project(
        &mut self,
        action_id: lp_studio_core::ActionId,
        project_id: &str,
    ) -> Result<Vec<StudioEvent>, StudioRuntimeError> {
        let mut events = Vec::new();
        for request in demo_project::demo_write_requests(project_id) {
            let exchange = self.client.send_request(request).await?;
            events.extend(exchange.events);
            demo_project::ensure_write_response(&exchange.response.msg)
                .map_err(StudioRuntimeError::Protocol)?;
        }
        events.push(StudioEvent::DemoProjectSeeded {
            action_id,
            project_id: project_id.to_string(),
        });
        Ok(events)
    }

    pub async fn load_project(
        &mut self,
        action_id: lp_studio_core::ActionId,
        project_id: &str,
    ) -> Result<Vec<StudioEvent>, StudioRuntimeError> {
        let exchange = self
            .client
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

    pub async fn read_inventory(
        &mut self,
        action_id: lp_studio_core::ActionId,
        handle: lpc_wire::WireProjectHandle,
    ) -> Result<Vec<StudioEvent>, StudioRuntimeError> {
        let exchange = self.client.send_request(inventory_request(handle)).await?;
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

    pub async fn refresh_loaded_projects(
        &mut self,
        action_id: lp_studio_core::ActionId,
    ) -> Result<Vec<StudioEvent>, StudioRuntimeError> {
        let exchange = self
            .client
            .send_request(ClientRequest::ListLoadedProjects)
            .await?;
        let mut events = exchange.events;
        match exchange.response.msg {
            WireServerMsgBody::ListLoadedProjects { projects } => {
                events.push(StudioEvent::LoadedProjectsRefreshed {
                    action_id,
                    projects,
                });
                Ok(events)
            }
            other => {
                events.push(StudioEvent::LogReceived {
                    entry: StudioLogEntry::new(
                        StudioLogLevel::Warn,
                        "lp-studio-runtime",
                        format!("unexpected status response: {other:?}"),
                    ),
                });
                Ok(events)
            }
        }
    }
}
