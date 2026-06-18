use std::cell::RefCell;
use std::rc::Rc;

use lp_studio_core::{
    ActionOrigin, BROWSER_WORKER_PROVIDER_ID, DeviceCapability, StudioActionKind, StudioApp,
    StudioEvent, StudioLogEntry, StudioLogLevel,
};
use lpa_link::providers::browser_worker::BrowserWorkerProvider;
use lpa_link::{LinkConnectionKind, LinkEndpointId, LinkProvider, LinkProviderId, LinkSession};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{MessageEvent, Worker, WorkerOptions, WorkerType};

use crate::browser_protocol_client::BrowserProtocolClient;
use crate::worker_envelope::{BrowserInputEnvelope, BrowserOutputEnvelope};
use crate::{StudioRuntimeError, demo_project};

pub struct BrowserWorkerStudioRuntime {
    worker: Worker,
    outputs: Rc<RefCell<Vec<BrowserOutputEnvelope>>>,
}

impl BrowserWorkerStudioRuntime {
    pub fn new(worker_url: &str) -> Result<Self, StudioRuntimeError> {
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
                    target: "lp-studio-runtime".to_string(),
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

    pub fn take_studio_events(&mut self) -> Vec<StudioEvent> {
        self.take_outputs()
            .into_iter()
            .filter_map(output_to_event)
            .collect()
    }
}

pub async fn run_browser_worker_demo(worker_url: &str) -> Result<StudioApp, StudioRuntimeError> {
    let mut app = StudioApp::new();
    app.dispatch_kind(
        StudioActionKind::SelectLinkProvider {
            provider_id: LinkProviderId::new(BROWSER_WORKER_PROVIDER_ID),
        },
        ActionOrigin::System,
    );

    let mut provider = BrowserWorkerProvider::new(BROWSER_WORKER_PROVIDER_ID);
    let endpoint_id = provider.create_worker_endpoint("Browser firmware runtime");
    let endpoints = provider
        .discover()
        .await
        .map_err(|error| StudioRuntimeError::Link(error.to_string()))?;
    let discover_effects =
        app.dispatch_kind(StudioActionKind::DiscoverDevices, ActionOrigin::Harness);
    let action_id = discover_effects
        .first()
        .and_then(|effect| match effect {
            lp_studio_core::StudioEffect::DiscoverEndpoints { action_id, .. } => Some(*action_id),
            _ => None,
        })
        .unwrap_or_default();
    app.apply_event(StudioEvent::EndpointsDiscovered {
        action_id,
        provider_id: LinkProviderId::new(BROWSER_WORKER_PROVIDER_ID),
        endpoints,
    });

    let mut session = provider
        .connect(&endpoint_id)
        .await
        .map_err(|error| StudioRuntimeError::Link(error.to_string()))?;
    let connection = session
        .connection()
        .await
        .map_err(|error| StudioRuntimeError::Link(error.to_string()))?;
    let mut runtime = BrowserWorkerStudioRuntime::new(worker_url)?;
    for event in runtime.boot("Studio browser runtime").await? {
        app.apply_event(event);
    }
    let connect_effects = app.dispatch_kind(
        StudioActionKind::ConnectDevice {
            endpoint_id: LinkEndpointId::new(endpoint_id.as_str()),
        },
        ActionOrigin::Harness,
    );
    let connect_action_id = connect_effects
        .first()
        .and_then(|effect| match effect {
            lp_studio_core::StudioEffect::ConnectEndpoint { action_id, .. } => Some(*action_id),
            _ => None,
        })
        .unwrap_or_default();
    app.apply_event(StudioEvent::DeviceConnected {
        action_id: connect_action_id,
        provider_id: LinkProviderId::new(BROWSER_WORKER_PROVIDER_ID),
        endpoint_id,
        session_id: session.id().clone(),
        connection_kind: match connection.kind {
            LinkConnectionKind::BrowserWorker { protocol } => {
                LinkConnectionKind::BrowserWorker { protocol }
            }
            other => other,
        },
        capabilities: browser_worker_capabilities(),
    });

    let mut client = BrowserProtocolClient::new(runtime);
    let load_effects = app.dispatch_kind(StudioActionKind::LoadDemoProject, ActionOrigin::Harness);
    let load_action_id = load_effects
        .first()
        .and_then(|effect| match effect {
            lp_studio_core::StudioEffect::SeedDemoProject { action_id, .. } => Some(*action_id),
            _ => None,
        })
        .unwrap_or_default();
    for event in client
        .seed_demo_project(load_action_id, demo_project::DEMO_PROJECT_ID)
        .await?
    {
        let effects = app.apply_event(event);
        for effect in effects {
            for event in client.execute_project_effect(effect).await? {
                let follow_up = app.apply_event(event);
                for effect in follow_up {
                    for event in client.execute_project_effect(effect).await? {
                        app.apply_event(event);
                    }
                }
            }
        }
    }
    Ok(app)
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
