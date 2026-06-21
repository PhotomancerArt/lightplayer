use lpa_link::LinkProvider;
use lpa_link::LinkProviderKind;
use lpa_link::provider::endpoint::LinkEndpointId;
use lpa_link::provider::session::LinkSessionId;
use lpa_link::providers::host_process::HostProcessProvider;
use lpa_studio_core::{
    DeviceAccessStatus, DeviceCapability, HOST_PROCESS_PROVIDER_ID, ProviderAvailability,
    ProviderCapability, ProviderCardState, ProviderIntent, StudioEffect, StudioEvent,
    StudioLogEntry, StudioLogLevel, TargetProbeResult,
};

use crate::StudioRuntimeError;
use crate::client_session_runtime::ClientSessionRuntime;
use crate::effect_executor::EffectExecutor;
use crate::project_session_runtime::ProjectSessionRuntime;

pub struct HostProcessStudioRuntime {
    provider: HostProcessProvider,
    session_id: Option<LinkSessionId>,
    client: Option<ClientSessionRuntime>,
}

impl HostProcessStudioRuntime {
    pub fn new() -> Self {
        let mut provider = HostProcessProvider::new();
        provider.create_memory_endpoint("Studio host runtime");
        Self {
            provider,
            session_id: None,
            client: None,
        }
    }

    pub async fn close(&mut self) -> Result<(), StudioRuntimeError> {
        if let Some(session_id) = &self.session_id {
            self.provider
                .close(session_id)
                .await
                .map_err(|error| StudioRuntimeError::Link(error.to_string()))?;
        }
        self.session_id = None;
        self.client = None;
        Ok(())
    }

    async fn discover(
        &mut self,
        action_id: lpa_studio_core::ActionId,
        provider_id: LinkProviderKind,
    ) -> Result<Vec<StudioEvent>, StudioRuntimeError> {
        if provider_id != HOST_PROCESS_PROVIDER_ID {
            return Err(StudioRuntimeError::UnsupportedProvider(
                provider_id.as_str().to_string(),
            ));
        }
        let endpoints = self
            .provider
            .discover()
            .await
            .map_err(|error| StudioRuntimeError::Link(error.to_string()))?;
        Ok(vec![StudioEvent::EndpointsDiscovered {
            action_id,
            provider_id,
            endpoints,
        }])
    }

    async fn refresh_provider_catalog(
        &mut self,
        action_id: lpa_studio_core::ActionId,
    ) -> Result<Vec<StudioEvent>, StudioRuntimeError> {
        let endpoints = self
            .provider
            .discover()
            .await
            .map_err(|error| StudioRuntimeError::Link(error.to_string()))?;
        Ok(vec![StudioEvent::ProviderCatalogUpdated {
            action_id: Some(action_id),
            providers: vec![
                ProviderCardState::new(
                    HOST_PROCESS_PROVIDER_ID,
                    "Host runtime",
                    ProviderIntent::RunHostRuntime,
                )
                .with_availability(ProviderAvailability::Available)
                .with_capabilities(vec![
                    ProviderCapability::DiscoverEndpoints,
                    ProviderCapability::Connect,
                    ProviderCapability::Simulate,
                    ProviderCapability::ReadLogs,
                    ProviderCapability::ReadDiagnostics,
                    ProviderCapability::DeployProject,
                    ProviderCapability::ReadProjectInventory,
                ])
                .with_endpoints(endpoints),
            ],
        }])
    }

    async fn connect(
        &mut self,
        action_id: lpa_studio_core::ActionId,
        endpoint_id: LinkEndpointId,
    ) -> Result<Vec<StudioEvent>, StudioRuntimeError> {
        let session = self
            .provider
            .connect(&endpoint_id)
            .await
            .map_err(|error| StudioRuntimeError::Link(error.to_string()))?;
        let connection = self
            .provider
            .connection(session.id())
            .await
            .map_err(|error| StudioRuntimeError::Link(error.to_string()))?;
        let transport = connection
            .server_connection()
            .ok_or(StudioRuntimeError::MissingClient)?;
        let session_id = session.id().clone();
        let logs = self
            .provider
            .logs(&session_id)
            .map_err(|error| StudioRuntimeError::Link(error.to_string()))?;
        let diagnostics = self
            .provider
            .diagnostics(&session_id)
            .map_err(|error| StudioRuntimeError::Link(error.to_string()))?;
        let connection_kind = connection.kind.clone();
        self.client = Some(ClientSessionRuntime::new(transport));
        self.session_id = Some(session_id.clone());

        let mut events = Vec::new();
        for log in logs {
            events.push(StudioEvent::LogReceived {
                entry: StudioLogEntry::new(map_log_level(log.level), "lpa-link", log.message),
            });
        }
        for diagnostic in diagnostics {
            events.push(StudioEvent::DiagnosticRaised {
                diagnostic: lpa_studio_core::StudioDiagnostic::info(diagnostic.message),
            });
        }
        events.push(StudioEvent::DeviceConnected {
            action_id,
            provider_id: HOST_PROCESS_PROVIDER_ID,
            endpoint_id,
            session_id,
            connection_kind,
            capabilities: host_process_capabilities(),
        });
        Ok(events)
    }

    fn project_runtime(&mut self) -> Result<ProjectSessionRuntime<'_>, StudioRuntimeError> {
        let client = self
            .client
            .as_mut()
            .ok_or(StudioRuntimeError::MissingClient)?;
        Ok(ProjectSessionRuntime::new(client))
    }
}

impl Default for HostProcessStudioRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl EffectExecutor for HostProcessStudioRuntime {
    async fn execute_effect(
        &mut self,
        effect: StudioEffect,
    ) -> Result<Vec<StudioEvent>, StudioRuntimeError> {
        match effect {
            StudioEffect::RefreshProviderCatalog { action_id } => {
                self.refresh_provider_catalog(action_id).await
            }
            StudioEffect::RequestDeviceAccess {
                action_id,
                provider_id,
            } => {
                if provider_id != HOST_PROCESS_PROVIDER_ID {
                    return Err(StudioRuntimeError::UnsupportedProvider(
                        provider_id.as_str().to_string(),
                    ));
                }
                Ok(vec![StudioEvent::DeviceAccessUpdated {
                    action_id: Some(action_id),
                    provider_id,
                    status: DeviceAccessStatus::Granted,
                }])
            }
            StudioEffect::DiscoverEndpoints {
                action_id,
                provider_id,
            } => self.discover(action_id, provider_id).await,
            StudioEffect::ConnectEndpoint {
                action_id,
                endpoint_id,
            } => self.connect(action_id, endpoint_id).await,
            StudioEffect::ProbeTarget {
                action_id,
                endpoint_id,
            } => Ok(vec![StudioEvent::TargetProbeCompleted {
                action_id,
                result: TargetProbeResult::server(endpoint_id, Some("host-process".to_string())),
            }]),
            StudioEffect::DisconnectSession {
                action_id,
                session_id,
            } => {
                self.close().await?;
                Ok(vec![StudioEvent::DeviceDisconnected {
                    action_id,
                    session_id,
                }])
            }
            StudioEffect::ResetDevice {
                action_id,
                endpoint_id: _,
            } => Ok(vec![StudioEvent::ActionFailed {
                action_id,
                message: "host-process reset is not implemented".to_string(),
            }]),
            StudioEffect::FlashDeviceFirmware {
                action_id,
                endpoint_id: _,
                firmware_id: _,
            } => Ok(vec![StudioEvent::ActionFailed {
                action_id,
                message: "host-process firmware flashing is not supported".to_string(),
            }]),
            StudioEffect::SeedDemoProject {
                action_id,
                project_id,
            } => {
                self.project_runtime()?
                    .seed_demo_project(action_id, &project_id)
                    .await
            }
            StudioEffect::LoadProject {
                action_id,
                project_id,
            } => {
                self.project_runtime()?
                    .load_project(action_id, &project_id)
                    .await
            }
            StudioEffect::RefreshStatus { action_id } => {
                self.project_runtime()?
                    .refresh_loaded_projects(action_id)
                    .await
            }
            StudioEffect::ReadProjectState { action_id } => {
                self.project_runtime()?.read_project_state(action_id).await
            }
            StudioEffect::ReadProjectInventory { action_id, handle } => {
                self.project_runtime()?
                    .read_inventory(action_id, handle)
                    .await
            }
        }
    }
}

fn host_process_capabilities() -> Vec<DeviceCapability> {
    vec![
        DeviceCapability::Connect,
        DeviceCapability::UseHostProcess,
        DeviceCapability::WriteProjectFiles,
        DeviceCapability::ReadHeartbeat,
        DeviceCapability::ListProjects,
        DeviceCapability::LoadProject,
        DeviceCapability::ReadProjectInventory,
        DeviceCapability::ReadLogs,
        DeviceCapability::ReadDiagnostics,
    ]
}

fn map_log_level(level: lpa_link::LinkLogLevel) -> StudioLogLevel {
    match level {
        lpa_link::LinkLogLevel::Trace => StudioLogLevel::Trace,
        lpa_link::LinkLogLevel::Debug => StudioLogLevel::Debug,
        lpa_link::LinkLogLevel::Info => StudioLogLevel::Info,
        lpa_link::LinkLogLevel::Warn => StudioLogLevel::Warn,
        lpa_link::LinkLogLevel::Error => StudioLogLevel::Error,
    }
}

#[cfg(test)]
mod tests {
    use lpa_studio_core::{
        ActionOrigin, LinkActionRequest, ProjectActionRequest, StudioActionKind,
    };

    use crate::demo_project;
    use crate::harness::RuntimeHarness;

    #[tokio::test]
    async fn host_process_harness_loads_demo_project() {
        let mut harness = RuntimeHarness::host_process();
        harness
            .dispatch(
                StudioActionKind::from(LinkActionRequest::DiscoverDevices),
                ActionOrigin::Harness,
            )
            .await
            .unwrap();
        let endpoint_id = harness
            .app()
            .state()
            .device_manager
            .providers
            .first_selected_endpoint()
            .expect("discovered endpoint")
            .id
            .clone();
        harness
            .dispatch(
                StudioActionKind::from(LinkActionRequest::ConnectEndpoint { endpoint_id }),
                ActionOrigin::Harness,
            )
            .await
            .unwrap();
        harness
            .dispatch(
                StudioActionKind::from(ProjectActionRequest::LoadDemoProject),
                ActionOrigin::Harness,
            )
            .await
            .unwrap();

        let project = harness
            .app()
            .state()
            .project_session
            .as_ref()
            .expect("project session");
        let inventory = project.inventory.as_ref().expect("project inventory");
        assert!(!inventory.nodes.is_empty());
        assert_eq!(project.project_id, demo_project::DEMO_PROJECT_ID);
        harness.runtime_mut().close().await.unwrap();
    }
}
