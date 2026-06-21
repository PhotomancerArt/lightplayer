use std::cell::RefCell;
use std::rc::Rc;

use crate::browser_protocol_client::BrowserProtocolClient;
use crate::effect_executor::EffectExecutor;
use crate::harness::RuntimeHarness;
use crate::worker_envelope::{BrowserInputEnvelope, BrowserOutputEnvelope};
use crate::StudioRuntimeError;
use lpa_link::link_endpoint::LinkEndpointId;
use lpa_link::link_provider::LinkProviderId;
use lpa_link::providers::browser_worker::{BrowserWorkerProvider, BrowserWorkerSession};
use lpa_link::{LinkConnectionKind, LinkProvider, LinkSession};
use lpa_studio_core::{
    ActionOrigin, DeviceAccessStatus, DeviceCapability, LinkActionRequest,
    ProjectActionRequest, ProviderAvailability, ProviderCapability, ProviderCardState,
    ProviderIntent, StudioActionKind, StudioApp, StudioEffect, StudioEvent, StudioLogEntry,
    StudioLogLevel, TargetProbeResult, BROWSER_WORKER_PROVIDER_ID,
};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{MessageEvent, Worker, WorkerOptions, WorkerType};

/// Browser Worker-backed Studio runtime used by the simulator provider.
pub struct BrowserWorkerStudioRuntime {
    worker_url: String,
    provider: BrowserWorkerProvider,
    session: Option<BrowserWorkerSession>,
    client: Option<BrowserProtocolClient>,
}

impl BrowserWorkerStudioRuntime {
    pub fn new(worker_url: &str) -> Self {
        let mut provider = BrowserWorkerProvider::new(BROWSER_WORKER_PROVIDER_ID);
        provider.create_worker_endpoint("Browser firmware runtime");
        Self {
            worker_url: worker_url.to_string(),
            provider,
            session: None,
            client: None,
        }
    }

    pub async fn close(&mut self) -> Result<(), StudioRuntimeError> {
        if let Some(client) = &mut self.client {
            client.close();
        }
        if let Some(session) = &mut self.session {
            session
                .close()
                .await
                .map_err(|error| StudioRuntimeError::Link(error.to_string()))?;
        }
        self.client = None;
        self.session = None;
        Ok(())
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
        provider_id: LinkProviderId,
    ) -> Result<Vec<StudioEvent>, StudioRuntimeError> {
        if provider_id.as_str() != BROWSER_WORKER_PROVIDER_ID {
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
        provider_id: LinkProviderId,
    ) -> Result<Vec<StudioEvent>, StudioRuntimeError> {
        if provider_id.as_str() != BROWSER_WORKER_PROVIDER_ID {
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

    async fn connect(
        &mut self,
        action_id: lpa_studio_core::ActionId,
        endpoint_id: LinkEndpointId,
    ) -> Result<Vec<StudioEvent>, StudioRuntimeError> {
        let mut session = self
            .provider
            .connect(&endpoint_id)
            .await
            .map_err(|error| StudioRuntimeError::Link(error.to_string()))?;
        let connection = session
            .connection()
            .await
            .map_err(|error| StudioRuntimeError::Link(error.to_string()))?;
        let session_id = session.id().clone();
        let logs = session.logs();
        let diagnostics = session.diagnostics();
        let connection_kind = match connection.kind {
            LinkConnectionKind::BrowserWorker { protocol } => {
                LinkConnectionKind::BrowserWorker { protocol }
            }
            other => other,
        };
        let mut worker = BrowserWorkerHandle::new(&self.worker_url)?;

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
        events.extend(worker.boot("Studio browser runtime").await?);

        self.client = Some(BrowserProtocolClient::new(worker));
        self.session = Some(session);
        events.push(StudioEvent::DeviceConnected {
            action_id,
            provider_id: LinkProviderId::new(BROWSER_WORKER_PROVIDER_ID),
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

pub(crate) struct BrowserWorkerHandle {
    worker: Worker,
    outputs: Rc<RefCell<Vec<BrowserOutputEnvelope>>>,
}

impl BrowserWorkerHandle {
    fn new(worker_url: &str) -> Result<Self, StudioRuntimeError> {
        let options = WorkerOptions::new();
        options.set_type(WorkerType::Module);
        let worker = Worker::new_with_options(worker_url, &options)
            .map_err(|error| StudioRuntimeError::Browser(format!("{error:?}")))?;
        let outputs = Rc::new(RefCell::new(Vec::new()));
        let output_ref = Rc::clone(&outputs);
        let on_message = Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
            match serde_wasm_bindgen::from_value::<BrowserOutputEnvelope>(event.data()) {
                Ok(envelope) => output_ref.borrow_mut().push(envelope),
                Err(error) => output_ref.borrow_mut().push(BrowserOutputEnvelope::Log {
                    runtime_id: 0,
                    level: "error".to_string(),
                    target: "lpa-studio-runtime".to_string(),
                    message: format!("failed to parse worker message: {error}"),
                }),
            }
        });
        worker.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
        on_message.forget();
        Ok(Self { worker, outputs })
    }

    pub async fn boot(&mut self, label: &str) -> Result<Vec<StudioEvent>, StudioRuntimeError> {
        self.post(&BrowserInputEnvelope::Boot {
            label: label.to_string(),
        })?;
        let mut events = Vec::new();
        for _ in 0..200 {
            crate::browser_protocol_client::sleep_ms(25).await?;
            for output in self.take_outputs() {
                let ready = matches!(
                    &output,
                    BrowserOutputEnvelope::Status { status, .. } if status == "ready"
                );
                if let Some(event) = output_to_event(output) {
                    events.push(event);
                }
                if ready {
                    return Ok(events);
                }
            }
        }
        Err(StudioRuntimeError::Browser(
            "timed out waiting for browser worker boot".to_string(),
        ))
    }

    pub fn post(&self, envelope: &BrowserInputEnvelope) -> Result<(), StudioRuntimeError> {
        let value = serde_wasm_bindgen::to_value(envelope)
            .map_err(|error| StudioRuntimeError::Browser(error.to_string()))?;
        self.worker
            .post_message(&value)
            .map_err(|error| StudioRuntimeError::Browser(format!("{error:?}")))
    }

    pub fn take_outputs(&mut self) -> Vec<BrowserOutputEnvelope> {
        core::mem::take(&mut *self.outputs.borrow_mut())
    }

    pub fn terminate(&self) {
        self.worker.terminate();
    }
}

pub async fn run_browser_worker_demo(worker_url: &str) -> Result<StudioApp, StudioRuntimeError> {
    let mut harness = RuntimeHarness::with_runtime(BrowserWorkerStudioRuntime::new(worker_url));
    harness
        .dispatch(
            StudioActionKind::from(LinkActionRequest::RefreshProviderCatalog),
            ActionOrigin::Harness,
        )
        .await?;
    harness
        .dispatch(
            StudioActionKind::from(LinkActionRequest::StartProvisioning {
                provider_id: LinkProviderId::new(BROWSER_WORKER_PROVIDER_ID),
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
