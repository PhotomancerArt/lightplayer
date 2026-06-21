use std::cell::RefCell;
use std::rc::Rc;

use crate::StudioRuntimeError;
use crate::browser_protocol_client::BrowserProtocolClient;
use crate::effect_executor::EffectExecutor;
use crate::harness::RuntimeHarness;
use lpa_link::LinkProviderKind;
use lpa_link::provider::endpoint::LinkEndpointId;
use lpa_link::provider::session::LinkSessionId;
use lpa_link::providers::browser_worker::{
    BrowserOutputEnvelope, BrowserWorkerOptions, BrowserWorkerProvider,
};
use lpa_link::{LinkConnectionKind, LinkProvider};
use lpa_studio_core::{
    ActionOrigin, BROWSER_WORKER_PROVIDER_ID, DeviceAccessStatus, DeviceCapability,
    LinkActionRequest, ProjectActionRequest, ProviderAvailability, ProviderCapability,
    ProviderCardState, ProviderIntent, StudioActionKind, StudioApp, StudioEffect, StudioEvent,
    StudioLogEntry, StudioLogLevel, TargetProbeResult,
};

/// Browser Worker-backed Studio runtime used by the simulator provider.
pub struct BrowserWorkerStudioRuntime {
    provider: Rc<RefCell<BrowserWorkerProvider>>,
    session_id: Option<LinkSessionId>,
    client: Option<BrowserProtocolClient>,
}

impl BrowserWorkerStudioRuntime {
    pub fn new() -> Self {
        Self::with_options(BrowserWorkerOptions::default())
    }

    pub fn with_options(options: BrowserWorkerOptions) -> Self {
        let mut provider = BrowserWorkerProvider::with_options(options);
        provider.create_worker_endpoint("Browser firmware runtime");
        Self {
            provider: Rc::new(RefCell::new(provider)),
            session_id: None,
            client: None,
        }
    }

    pub async fn close(&mut self) -> Result<(), StudioRuntimeError> {
        if let Some(client) = &mut self.client {
            client.close().await?;
        } else if let Some(session_id) = &self.session_id {
            self.provider
                .borrow_mut()
                .close(session_id)
                .await
                .map_err(|error| StudioRuntimeError::Link(error.to_string()))?;
        }
        self.client = None;
        self.session_id = None;
        Ok(())
    }

    async fn refresh_provider_catalog(
        &mut self,
        action_id: lpa_studio_core::ActionId,
    ) -> Result<Vec<StudioEvent>, StudioRuntimeError> {
        let endpoints = self
            .provider
            .borrow_mut()
            .discover()
            .await
            .map_err(|error| StudioRuntimeError::Link(error.to_string()))?;
        Ok(vec![StudioEvent::ProviderCatalogUpdated {
            action_id: Some(action_id),
            providers: vec![
                ProviderCardState::new(
                    BROWSER_WORKER_PROVIDER_ID,
                    "Simulator",
                    ProviderIntent::SimulateInBrowser,
                )
                .with_availability(ProviderAvailability::Available)
                .with_capabilities(vec![
                    ProviderCapability::DiscoverEndpoints,
                    ProviderCapability::Connect,
                    ProviderCapability::Simulate,
                    ProviderCapability::ReadLogs,
                    ProviderCapability::ReadDiagnostics,
                    ProviderCapability::ReadHeartbeat,
                    ProviderCapability::DeployProject,
                    ProviderCapability::ReadProjectInventory,
                ])
                .with_endpoints(endpoints),
            ],
        }])
    }

    async fn request_device_access(
        &mut self,
        action_id: lpa_studio_core::ActionId,
        provider_id: LinkProviderKind,
    ) -> Result<Vec<StudioEvent>, StudioRuntimeError> {
        if provider_id != BROWSER_WORKER_PROVIDER_ID {
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

    async fn discover(
        &mut self,
        action_id: lpa_studio_core::ActionId,
        provider_id: LinkProviderKind,
    ) -> Result<Vec<StudioEvent>, StudioRuntimeError> {
        if provider_id != BROWSER_WORKER_PROVIDER_ID {
            return Err(StudioRuntimeError::UnsupportedProvider(
                provider_id.as_str().to_string(),
            ));
        }
        let endpoints = self
            .provider
            .borrow_mut()
            .discover()
            .await
            .map_err(|error| StudioRuntimeError::Link(error.to_string()))?;
        Ok(vec![StudioEvent::EndpointsDiscovered {
            action_id,
            provider_id,
            endpoints,
        }])
    }

    async fn connect(
        &mut self,
        action_id: lpa_studio_core::ActionId,
        endpoint_id: LinkEndpointId,
    ) -> Result<Vec<StudioEvent>, StudioRuntimeError> {
        let session = self
            .provider
            .borrow_mut()
            .connect(&endpoint_id)
            .await
            .map_err(|error| StudioRuntimeError::Link(error.to_string()))?;
        let connection = self
            .provider
            .borrow_mut()
            .connection(session.id())
            .await
            .map_err(|error| StudioRuntimeError::Link(error.to_string()))?;
        let session_id = session.id().clone();
        let logs = self
            .provider
            .borrow()
            .logs(&session_id)
            .map_err(|error| StudioRuntimeError::Link(error.to_string()))?;
        let diagnostics = self
            .provider
            .borrow()
            .diagnostics(&session_id)
            .map_err(|error| StudioRuntimeError::Link(error.to_string()))?;
        let connection_kind = match connection.kind {
            LinkConnectionKind::BrowserWorker { protocol } => {
                LinkConnectionKind::BrowserWorker { protocol }
            }
            other => other,
        };
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
        events.extend(
            self.provider
                .borrow_mut()
                .take_outputs(&session_id)
                .map_err(|error| StudioRuntimeError::Link(error.to_string()))?
                .into_iter()
                .filter_map(output_to_event),
        );

        self.client = Some(BrowserProtocolClient::new(
            Rc::clone(&self.provider),
            session_id.clone(),
        ));
        self.session_id = Some(session_id.clone());
        events.push(StudioEvent::DeviceConnected {
            action_id,
            provider_id: BROWSER_WORKER_PROVIDER_ID,
            endpoint_id,
            session_id,
            connection_kind,
            capabilities: browser_worker_capabilities(),
        });
        Ok(events)
    }

    fn project_client(&mut self) -> Result<&mut BrowserProtocolClient, StudioRuntimeError> {
        self.client
            .as_mut()
            .ok_or(StudioRuntimeError::MissingClient)
    }
}

impl EffectExecutor for BrowserWorkerStudioRuntime {
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
            } => self.request_device_access(action_id, provider_id).await,
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
                result: TargetProbeResult::server(endpoint_id, Some("browser-worker".to_string())),
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
                message: "browser worker reset is not implemented".to_string(),
            }]),
            StudioEffect::FlashDeviceFirmware {
                action_id,
                endpoint_id: _,
                firmware_id: _,
            } => Ok(vec![StudioEvent::ActionFailed {
                action_id,
                message: "browser worker firmware flashing is not supported".to_string(),
            }]),
            StudioEffect::SeedDemoProject {
                action_id,
                project_id,
            } => {
                self.project_client()?
                    .seed_demo_project(action_id, &project_id)
                    .await
            }
            effect => self.project_client()?.execute_project_effect(effect).await,
        }
    }
}

pub async fn run_browser_worker_demo() -> Result<StudioApp, StudioRuntimeError> {
    let mut harness = RuntimeHarness::with_runtime(BrowserWorkerStudioRuntime::new());
    harness
        .dispatch(
            StudioActionKind::from(LinkActionRequest::RefreshProviderCatalog),
            ActionOrigin::Harness,
        )
        .await?;
    harness
        .dispatch(
            StudioActionKind::from(LinkActionRequest::StartProvisioning {
                provider_id: BROWSER_WORKER_PROVIDER_ID,
            }),
            ActionOrigin::Harness,
        )
        .await?;
    let endpoint_id = harness
        .app()
        .state()
        .device_manager
        .providers
        .first_selected_endpoint()
        .ok_or_else(|| {
            StudioRuntimeError::Link("browser worker discovery returned no endpoints".to_string())
        })?
        .id
        .clone();
    harness
        .dispatch(
            StudioActionKind::from(LinkActionRequest::ConnectEndpoint { endpoint_id }),
            ActionOrigin::Harness,
        )
        .await?;
    harness
        .dispatch(
            StudioActionKind::from(ProjectActionRequest::LoadDemoProject),
            ActionOrigin::Harness,
        )
        .await?;
    Ok(harness.into_app())
}

fn browser_worker_capabilities() -> Vec<DeviceCapability> {
    vec![
        DeviceCapability::Connect,
        DeviceCapability::UseBrowserWorker,
        DeviceCapability::WriteProjectFiles,
        DeviceCapability::ReadHeartbeat,
        DeviceCapability::ListProjects,
        DeviceCapability::LoadProject,
        DeviceCapability::ReadProjectInventory,
        DeviceCapability::ReadLogs,
        DeviceCapability::ReadDiagnostics,
    ]
}

fn output_to_event(output: BrowserOutputEnvelope) -> Option<StudioEvent> {
    match output {
        BrowserOutputEnvelope::Status {
            status, message, ..
        } => Some(StudioEvent::LogReceived {
            entry: StudioLogEntry::new(
                StudioLogLevel::Info,
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
            entry: StudioLogEntry::new(parse_worker_log_level(&level), target, message),
        }),
        BrowserOutputEnvelope::ProtocolOut { .. } => None,
    }
}

fn parse_worker_log_level(level: &str) -> StudioLogLevel {
    match level {
        "trace" => StudioLogLevel::Trace,
        "debug" => StudioLogLevel::Debug,
        "warn" => StudioLogLevel::Warn,
        "error" => StudioLogLevel::Error,
        _ => StudioLogLevel::Info,
    }
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
