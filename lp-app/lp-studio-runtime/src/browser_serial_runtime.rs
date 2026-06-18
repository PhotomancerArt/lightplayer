use lp_studio_core::{
    ActionOrigin, BROWSER_SERIAL_ESP32_PROVIDER_ID, DeviceAccessStatus, DeviceCapability,
    StudioActionKind, StudioApp, StudioEffect, StudioEvent, StudioLogEntry, StudioLogLevel,
};
use lpa_link::providers::browser_serial_esp32::{
    BrowserSerialEsp32Provider, BrowserSerialEsp32Session,
};
use lpa_link::{LinkConnectionKind, LinkEndpointId, LinkProvider, LinkProviderId, LinkSession};
use lpc_model::DEFAULT_SERIAL_BAUD_RATE;

use crate::StudioRuntimeError;
use crate::browser_serial_protocol_client::BrowserSerialProtocolClient;
use crate::browser_serial_shim;
use crate::effect_executor::EffectExecutor;

pub struct BrowserSerialStudioRuntime {
    provider: BrowserSerialEsp32Provider,
    endpoint_ports: Vec<(LinkEndpointId, u32)>,
    session: Option<BrowserSerialEsp32Session>,
    client: Option<BrowserSerialProtocolClient>,
}

impl BrowserSerialStudioRuntime {
    pub fn new() -> Self {
        Self {
            provider: BrowserSerialEsp32Provider::new(BROWSER_SERIAL_ESP32_PROVIDER_ID),
            endpoint_ports: Vec::new(),
            session: None,
            client: None,
        }
    }

    pub async fn close(&mut self) -> Result<(), StudioRuntimeError> {
        if let Some(client) = &self.client {
            browser_serial_shim::close(client.port_id()).await?;
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

    async fn request_device_access(
        &mut self,
        action_id: lp_studio_core::ActionId,
        provider_id: LinkProviderId,
    ) -> Result<Vec<StudioEvent>, StudioRuntimeError> {
        if provider_id.as_str() != BROWSER_SERIAL_ESP32_PROVIDER_ID {
            return Err(StudioRuntimeError::UnsupportedProvider(
                provider_id.as_str().to_string(),
            ));
        }
        if !browser_serial_shim::is_supported() {
            return Ok(vec![StudioEvent::DeviceAccessUpdated {
                action_id: Some(action_id),
                provider_id,
                status: DeviceAccessStatus::Unsupported {
                    reason: "Web Serial is not supported in this browser.".to_string(),
                },
            }]);
        }

        let port = match browser_serial_shim::request_port().await {
            Ok(port) => port,
            Err(error) => {
                return Ok(vec![StudioEvent::DeviceAccessUpdated {
                    action_id: Some(action_id),
                    provider_id,
                    status: DeviceAccessStatus::PermissionDenied {
                        reason: error.to_string(),
                    },
                }]);
            }
        };

        let endpoint_id = self.provider.create_granted_endpoint(port.label);
        self.endpoint_ports.push((endpoint_id.clone(), port.id));
        let endpoints = self
            .provider
            .discover()
            .await
            .map_err(|error| StudioRuntimeError::Link(error.to_string()))?;

        Ok(vec![
            StudioEvent::DeviceAccessUpdated {
                action_id: Some(action_id),
                provider_id: provider_id.clone(),
                status: DeviceAccessStatus::Granted,
            },
            StudioEvent::EndpointsDiscovered {
                action_id,
                provider_id,
                endpoints,
            },
        ])
    }

    async fn discover(
        &mut self,
        action_id: lp_studio_core::ActionId,
        provider_id: LinkProviderId,
    ) -> Result<Vec<StudioEvent>, StudioRuntimeError> {
        if provider_id.as_str() != BROWSER_SERIAL_ESP32_PROVIDER_ID {
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
        action_id: lp_studio_core::ActionId,
        endpoint_id: LinkEndpointId,
    ) -> Result<Vec<StudioEvent>, StudioRuntimeError> {
        let port_id = self.port_id_for_endpoint(&endpoint_id)?;
        browser_serial_shim::open(port_id, DEFAULT_SERIAL_BAUD_RATE).await?;

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
            LinkConnectionKind::BrowserSerialEsp32 { protocol } => {
                LinkConnectionKind::BrowserSerialEsp32 { protocol }
            }
            other => other,
        };
        self.client = Some(BrowserSerialProtocolClient::new(port_id));
        self.session = Some(session);

        let mut events = Vec::new();
        for log in logs {
            events.push(StudioEvent::LogReceived {
                entry: StudioLogEntry::new(map_log_level(log.level), "lpa-link", log.message),
            });
        }
        for diagnostic in diagnostics {
            events.push(StudioEvent::DiagnosticRaised {
                diagnostic: lp_studio_core::StudioDiagnostic::info(diagnostic.message),
            });
        }
        events.push(StudioEvent::DeviceConnected {
            action_id,
            provider_id: LinkProviderId::new(BROWSER_SERIAL_ESP32_PROVIDER_ID),
            endpoint_id,
            session_id,
            connection_kind,
            capabilities: browser_serial_capabilities(),
        });
        Ok(events)
    }

    fn project_client(&mut self) -> Result<&mut BrowserSerialProtocolClient, StudioRuntimeError> {
        self.client
            .as_mut()
            .ok_or(StudioRuntimeError::MissingClient)
    }

    fn port_id_for_endpoint(
        &self,
        endpoint_id: &LinkEndpointId,
    ) -> Result<u32, StudioRuntimeError> {
        self.endpoint_ports
            .iter()
            .find(|(entry_endpoint_id, _)| entry_endpoint_id == endpoint_id)
            .map(|(_, port_id)| *port_id)
            .ok_or_else(|| {
                StudioRuntimeError::Link(format!(
                    "no browser serial port handle for endpoint {}",
                    endpoint_id.as_str()
                ))
            })
    }
}

impl Default for BrowserSerialStudioRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl EffectExecutor for BrowserSerialStudioRuntime {
    async fn execute_effect(
        &mut self,
        effect: StudioEffect,
    ) -> Result<Vec<StudioEvent>, StudioRuntimeError> {
        match effect {
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
                message: "browser serial reset is not implemented yet".to_string(),
            }]),
            StudioEffect::FlashDeviceFirmware {
                action_id,
                endpoint_id: _,
                firmware_id: _,
            } => Ok(vec![StudioEvent::ActionFailed {
                action_id,
                message: "browser firmware flashing is planned for the next phase".to_string(),
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

pub async fn run_browser_serial_demo() -> Result<StudioApp, StudioRuntimeError> {
    let mut app = StudioApp::new();
    app.dispatch_kind(
        StudioActionKind::SelectLinkProvider {
            provider_id: LinkProviderId::new(BROWSER_SERIAL_ESP32_PROVIDER_ID),
        },
        ActionOrigin::System,
    );
    let mut runtime = BrowserSerialStudioRuntime::new();
    let effects = app.dispatch_kind(StudioActionKind::RequestDeviceAccess, ActionOrigin::User);
    drain_effects(&mut app, &mut runtime, effects).await?;
    let endpoint_id = app
        .state()
        .link_selection
        .endpoints
        .first()
        .ok_or_else(|| {
            StudioRuntimeError::Link(
                "browser serial permission did not yield an endpoint".to_string(),
            )
        })?
        .id
        .clone();
    let effects = app.dispatch_kind(
        StudioActionKind::ConnectDevice { endpoint_id },
        ActionOrigin::User,
    );
    drain_effects(&mut app, &mut runtime, effects).await?;
    let effects = app.dispatch_kind(StudioActionKind::UploadDemoProject, ActionOrigin::User);
    drain_effects(&mut app, &mut runtime, effects).await?;
    Ok(app)
}

async fn drain_effects(
    app: &mut StudioApp,
    runtime: &mut BrowserSerialStudioRuntime,
    mut effects: Vec<StudioEffect>,
) -> Result<(), StudioRuntimeError> {
    while let Some(effect) = effects.pop() {
        let events = runtime.execute_effect(effect).await?;
        for event in events {
            effects.extend(app.apply_event(event));
        }
    }
    Ok(())
}

fn browser_serial_capabilities() -> Vec<DeviceCapability> {
    vec![
        DeviceCapability::RequestDeviceAccess,
        DeviceCapability::Connect,
        DeviceCapability::UseBrowserSerialEsp32,
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
