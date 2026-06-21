use std::cell::RefCell;
use std::rc::Rc;

use lpa_link::LinkProviderKind;
use lpa_link::provider::endpoint::LinkEndpointId;
use lpa_link::provider::session::LinkSessionId;
use lpa_link::providers::browser_serial_esp32::{
    BrowserEsp32FirmwareManifest, BrowserEsp32FlashProgress, BrowserSerialEsp32Options,
    BrowserSerialEsp32Provider,
};
use lpa_link::{LinkConnectionKind, LinkProvider};
use lpa_studio_core::{
    ActionId, ActionOrigin, BROWSER_SERIAL_ESP32_PROVIDER_ID, DeviceAccessStatus, DeviceCapability,
    DeviceIssue, DeviceIssueKind, LinkActionRequest, ProgressState, ProjectActionRequest,
    ProviderAvailability, ProviderCapability, ProviderCardState, ProviderIntent,
    ProvisioningReason, RecoveryAction, StudioActionKind, StudioApp, StudioDiagnostic,
    StudioEffect, StudioEvent, StudioLogEntry, StudioLogLevel, TargetKind, TargetProbeResult,
};
use lpc_model::DEFAULT_SERIAL_BAUD_RATE;

use crate::StudioRuntimeError;
use crate::browser_serial_protocol_client::BrowserSerialProtocolClient;
use crate::effect_executor::EffectExecutor;

pub struct BrowserSerialStudioRuntime {
    provider: Rc<RefCell<BrowserSerialEsp32Provider>>,
    session_id: Option<LinkSessionId>,
    client: Option<BrowserSerialProtocolClient>,
    flash_manifest: Option<BrowserEsp32FirmwareManifest>,
    flash_available: bool,
}

impl BrowserSerialStudioRuntime {
    pub fn new() -> Self {
        let options = BrowserSerialEsp32Options::default();
        Self::with_options(options)
    }

    pub fn with_options(options: BrowserSerialEsp32Options) -> Self {
        Self {
            provider: Rc::new(RefCell::new(BrowserSerialEsp32Provider::with_options(
                options,
            ))),
            session_id: None,
            client: None,
            flash_manifest: None,
            flash_available: false,
        }
    }

    pub async fn close(&mut self) -> Result<(), StudioRuntimeError> {
        if let Some(session_id) = &self.session_id {
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

    async fn request_device_access(
        &mut self,
        action_id: lpa_studio_core::ActionId,
        provider_id: LinkProviderKind,
    ) -> Result<Vec<StudioEvent>, StudioRuntimeError> {
        if provider_id != BROWSER_SERIAL_ESP32_PROVIDER_ID {
            return Err(StudioRuntimeError::UnsupportedProvider(
                provider_id.as_str().to_string(),
            ));
        }
        if !self.provider.borrow().is_serial_supported() {
            return Ok(vec![StudioEvent::DeviceAccessUpdated {
                action_id: Some(action_id),
                provider_id,
                status: DeviceAccessStatus::Unsupported {
                    reason: "Web Serial is not supported in this browser.".to_string(),
                },
            }]);
        }

        match self.provider.borrow_mut().request_access().await {
            Ok(_) => {}
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

        let endpoints = self
            .provider
            .borrow_mut()
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
        action_id: lpa_studio_core::ActionId,
        provider_id: LinkProviderKind,
    ) -> Result<Vec<StudioEvent>, StudioRuntimeError> {
        if provider_id != BROWSER_SERIAL_ESP32_PROVIDER_ID {
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

    async fn refresh_provider_catalog(
        &mut self,
        action_id: lpa_studio_core::ActionId,
    ) -> Result<Vec<StudioEvent>, StudioRuntimeError> {
        let serial_supported = self.provider.borrow().is_serial_supported();
        let flash_manifest = if serial_supported && self.provider.borrow().is_flash_supported() {
            self.provider.borrow().load_firmware_manifest().await.ok()
        } else {
            None
        };
        self.flash_available = flash_manifest.is_some();
        self.flash_manifest = flash_manifest;

        let availability = if serial_supported {
            ProviderAvailability::AvailableWithPermission
        } else {
            ProviderAvailability::unavailable(
                "Web Serial is not supported in this browser.",
                vec![
                    RecoveryAction::UseCompatibleBrowser,
                    RecoveryAction::ChooseSimulator,
                ],
            )
        };
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
                    BROWSER_SERIAL_ESP32_PROVIDER_ID,
                    "USB ESP32",
                    ProviderIntent::ConnectUsbEsp32,
                )
                .with_availability(availability)
                .with_capabilities(browser_serial_provider_capabilities(self.flash_available))
                .with_endpoints(endpoints),
            ],
        }])
    }

    async fn connect(
        &mut self,
        action_id: ActionId,
        endpoint_id: LinkEndpointId,
    ) -> Result<Vec<StudioEvent>, StudioRuntimeError> {
        let session = match self.provider.borrow_mut().connect(&endpoint_id).await {
            Ok(session) => session,
            Err(error) => {
                return Ok(Self::connection_failed_events(
                    action_id,
                    endpoint_id,
                    format!("Could not create browser serial link session: {error}"),
                ));
            }
        };
        if let Err(error) = self
            .provider
            .borrow_mut()
            .open_protocol(session.id(), DEFAULT_SERIAL_BAUD_RATE)
            .await
        {
            return Ok(Self::connection_failed_events(
                action_id,
                endpoint_id,
                format!("Could not open browser serial port: {error}"),
            ));
        }

        let connection = match self.provider.borrow_mut().connection(session.id()).await {
            Ok(connection) => connection,
            Err(error) => {
                return Ok(Self::connection_failed_events(
                    action_id,
                    endpoint_id,
                    format!("Could not open browser serial link connection: {error}"),
                ));
            }
        };
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
            LinkConnectionKind::BrowserSerialEsp32 { protocol } => {
                LinkConnectionKind::BrowserSerialEsp32 { protocol }
            }
            other => other,
        };
        self.client = Some(BrowserSerialProtocolClient::new(
            Rc::clone(&self.provider),
            session_id.clone(),
        ));
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
            provider_id: BROWSER_SERIAL_ESP32_PROVIDER_ID,
            endpoint_id,
            session_id,
            connection_kind,
            capabilities: browser_serial_capabilities(self.flash_available),
        });
        Ok(events)
    }

    fn connection_failed_events(
        action_id: ActionId,
        endpoint_id: LinkEndpointId,
        message: impl Into<String>,
    ) -> Vec<StudioEvent> {
        let issue = DeviceIssue::error(
            format!("browser-serial-connect-{}", action_id.get()),
            DeviceIssueKind::EndpointOpenFailed,
            message,
        )
        .with_provider(BROWSER_SERIAL_ESP32_PROVIDER_ID)
        .with_endpoint(endpoint_id.clone())
        .with_recovery_actions(vec![
            RecoveryAction::Retry,
            RecoveryAction::Reconnect,
            RecoveryAction::ResetDevice,
            RecoveryAction::ChooseSimulator,
        ]);
        vec![StudioEvent::DeviceConnectionFailed {
            action_id,
            endpoint_id,
            issue,
        }]
    }

    async fn probe_target(
        &mut self,
        action_id: ActionId,
        endpoint_id: LinkEndpointId,
    ) -> Result<Vec<StudioEvent>, StudioRuntimeError> {
        if let Some(client) = &mut self.client {
            match client.probe_server().await {
                Ok(mut events) => {
                    events.push(StudioEvent::TargetProbeCompleted {
                        action_id,
                        result: TargetProbeResult::server(
                            endpoint_id,
                            Some("lp-server".to_string()),
                        ),
                    });
                    return Ok(events);
                }
                Err(error) => {
                    let message = format!("LightPlayer server probe did not respond: {error}");
                    let mut events = vec![StudioEvent::LogReceived {
                        entry: StudioLogEntry::new(
                            StudioLogLevel::Warn,
                            "browser-serial-probe",
                            message.clone(),
                        ),
                    }];
                    events.push(StudioEvent::DiagnosticRaised {
                        diagnostic: StudioDiagnostic::info(message),
                    });
                    events.extend(self.release_protocol_session(action_id).await?);
                    return self
                        .probe_bootloader_target(action_id, endpoint_id, events)
                        .await;
                }
            }
        }

        self.probe_bootloader_target(action_id, endpoint_id, Vec::new())
            .await
    }

    async fn probe_bootloader_target(
        &mut self,
        action_id: ActionId,
        endpoint_id: LinkEndpointId,
        mut events: Vec<StudioEvent>,
    ) -> Result<Vec<StudioEvent>, StudioRuntimeError> {
        if !self.provider.borrow().is_flash_supported() {
            events.push(StudioEvent::TargetProbeFailed {
                action_id,
                endpoint_id: endpoint_id.clone(),
                issue: Self::probe_issue(
                    action_id,
                    &endpoint_id,
                    DeviceIssueKind::RuntimeUnsupported,
                    "Browser ESP32 target probing is not supported in this browser.",
                    vec![
                        RecoveryAction::UseCompatibleBrowser,
                        RecoveryAction::ChooseSimulator,
                    ],
                ),
            });
            return Ok(events);
        }

        match self.provider.borrow_mut().probe_target(&endpoint_id).await {
            Ok(result) => {
                for line in result.logs {
                    events.push(StudioEvent::LogReceived {
                        entry: StudioLogEntry::new(
                            StudioLogLevel::Info,
                            "browser-esp32-probe",
                            line,
                        ),
                    });
                }
                let chip_name = result
                    .chip_name
                    .unwrap_or_else(|| "unknown ESP32".to_string());
                if is_supported_esp32c6_chip(&chip_name) {
                    events.push(StudioEvent::DiagnosticRaised {
                        diagnostic: StudioDiagnostic::info(format!(
                            "Detected provisionable {chip_name} bootloader."
                        )),
                    });
                    events.push(StudioEvent::TargetProbeCompleted {
                        action_id,
                        result: TargetProbeResult {
                            endpoint_id,
                            kind: TargetKind::Bootloader,
                            server_version: None,
                            capabilities: browser_serial_capabilities(self.flash_available),
                            provisioning_reason: Some(ProvisioningReason::BootloaderMode),
                            issue: None,
                        },
                    });
                } else {
                    let issue = Self::probe_issue(
                        action_id,
                        &endpoint_id,
                        DeviceIssueKind::UnsupportedTarget,
                        format!("Detected unsupported ESP32 target: {chip_name}."),
                        vec![
                            RecoveryAction::Reconnect,
                            RecoveryAction::ChooseSimulator,
                            RecoveryAction::OpenHelp {
                                topic: "supported hardware".to_string(),
                            },
                        ],
                    );
                    events.push(StudioEvent::TargetProbeCompleted {
                        action_id,
                        result: TargetProbeResult {
                            endpoint_id,
                            kind: TargetKind::UnsupportedDevice,
                            server_version: None,
                            capabilities: Vec::new(),
                            provisioning_reason: None,
                            issue: Some(issue),
                        },
                    });
                }
                Ok(events)
            }
            Err(error) => {
                events.push(StudioEvent::TargetProbeFailed {
                    action_id,
                    endpoint_id: endpoint_id.clone(),
                    issue: Self::probe_issue(
                        action_id,
                        &endpoint_id,
                        DeviceIssueKind::ServerTimeout,
                        format!("No LightPlayer server or ESP32-C6 bootloader responded: {error}"),
                        vec![
                            RecoveryAction::Retry,
                            RecoveryAction::Reconnect,
                            RecoveryAction::ResetDevice,
                            RecoveryAction::ChooseSimulator,
                        ],
                    ),
                });
                Ok(events)
            }
        }
    }

    async fn release_protocol_session(
        &mut self,
        action_id: ActionId,
    ) -> Result<Vec<StudioEvent>, StudioRuntimeError> {
        let session_id = self.session_id.clone();
        if let Some(session_id) = &session_id {
            self.provider
                .borrow_mut()
                .release_session_for_management(session_id)
                .await
                .map_err(|error| StudioRuntimeError::Link(error.to_string()))?;
        }
        self.client = None;
        self.session_id = None;
        Ok(session_id
            .map(|session_id| StudioEvent::DeviceDisconnected {
                action_id,
                session_id,
            })
            .into_iter()
            .collect())
    }

    fn probe_issue(
        action_id: ActionId,
        endpoint_id: &LinkEndpointId,
        kind: DeviceIssueKind,
        message: impl Into<String>,
        recovery_actions: Vec<RecoveryAction>,
    ) -> DeviceIssue {
        DeviceIssue::error(
            format!("browser-serial-probe-{}", action_id.get()),
            kind,
            message,
        )
        .with_provider(BROWSER_SERIAL_ESP32_PROVIDER_ID)
        .with_endpoint(endpoint_id.clone())
        .with_recovery_actions(recovery_actions)
    }

    async fn flash_firmware(
        &mut self,
        action_id: ActionId,
        endpoint_id: LinkEndpointId,
        requested_firmware_id: Option<String>,
    ) -> Result<Vec<StudioEvent>, StudioRuntimeError> {
        if !self.provider.borrow().is_flash_supported() {
            return Ok(Self::flash_issue_events(
                action_id,
                endpoint_id,
                DeviceIssueKind::RuntimeUnsupported,
                "Browser ESP32 firmware flashing is not supported in this browser.",
                vec![
                    RecoveryAction::UseCompatibleBrowser,
                    RecoveryAction::ChooseSimulator,
                ],
            ));
        }

        let manifest = match self.flash_manifest.clone() {
            Some(manifest) => manifest,
            None => match self.provider.borrow().load_firmware_manifest().await {
                Ok(manifest) => {
                    self.flash_available = true;
                    self.flash_manifest = Some(manifest.clone());
                    manifest
                }
                Err(error) => {
                    return Ok(Self::flash_issue_events(
                        action_id,
                        endpoint_id,
                        DeviceIssueKind::FirmwareArtifactMissing,
                        format!("ESP32-C6 firmware artifact is unavailable: {error}"),
                        vec![
                            RecoveryAction::Retry,
                            RecoveryAction::OpenHelp {
                                topic: "studio firmware packaging".to_string(),
                            },
                        ],
                    ));
                }
            },
        };

        if requested_firmware_id
            .as_ref()
            .is_some_and(|firmware_id| firmware_id != &manifest.firmware_id)
        {
            return Ok(Self::flash_issue_events(
                action_id,
                endpoint_id,
                DeviceIssueKind::FirmwareArtifactMissing,
                format!(
                    "Requested firmware is not packaged for browser flashing: {}",
                    requested_firmware_id.unwrap_or_default()
                ),
                vec![
                    RecoveryAction::Retry,
                    RecoveryAction::OpenHelp {
                        topic: "studio firmware packaging".to_string(),
                    },
                ],
            ));
        }

        let disconnected_events = self.release_protocol_session(action_id).await?;

        let mut events = vec![StudioEvent::ProvisioningProgressUpdated {
            action_id: None,
            progress: ProgressState::new(format!("Flashing {}", manifest.display_name))
                .with_steps(0, 3)
                .with_percent(0),
        }];

        match self
            .provider
            .borrow_mut()
            .flash_firmware(&endpoint_id)
            .await
        {
            Ok(result) => {
                for line in result.logs {
                    events.push(StudioEvent::LogReceived {
                        entry: StudioLogEntry::new(
                            StudioLogLevel::Info,
                            "browser-esp32-flash",
                            line,
                        ),
                    });
                }
                for progress in result.progress {
                    events.push(StudioEvent::ProvisioningProgressUpdated {
                        action_id: None,
                        progress: map_flash_progress(progress),
                    });
                }
                if let Some(chip_name) = result.chip_name {
                    events.push(StudioEvent::DiagnosticRaised {
                        diagnostic: StudioDiagnostic::info(format!(
                            "Flashed firmware to {chip_name}."
                        )),
                    });
                }
                events.push(StudioEvent::ProvisioningProgressUpdated {
                    action_id: None,
                    progress: ProgressState::new("Firmware flash complete")
                        .with_steps(3, 3)
                        .with_percent(100),
                });
                events.extend(disconnected_events);
                events.push(StudioEvent::FirmwareFlashCompleted {
                    action_id,
                    endpoint_id,
                    firmware_id: Some(result.manifest.firmware_id),
                });
                Ok(events)
            }
            Err(error) => {
                events.extend(disconnected_events);
                events.extend(Self::flash_issue_events(
                    action_id,
                    endpoint_id,
                    DeviceIssueKind::FlashFailed,
                    format!("ESP32 firmware flash failed: {error}"),
                    vec![
                        RecoveryAction::Retry,
                        RecoveryAction::Reconnect,
                        RecoveryAction::ChooseSimulator,
                    ],
                ));
                Ok(events)
            }
        }
    }

    fn flash_issue_events(
        action_id: ActionId,
        endpoint_id: LinkEndpointId,
        kind: DeviceIssueKind,
        message: impl Into<String>,
        recovery_actions: Vec<RecoveryAction>,
    ) -> Vec<StudioEvent> {
        let message = message.into();
        let issue = DeviceIssue::error(
            format!("browser-esp32-flash-{}", action_id.get()),
            kind,
            message.clone(),
        )
        .with_provider(BROWSER_SERIAL_ESP32_PROVIDER_ID)
        .with_endpoint(endpoint_id);
        let issue = issue.with_recovery_actions(recovery_actions);
        vec![
            StudioEvent::DiagnosticRaised {
                diagnostic: StudioDiagnostic::error(Some(action_id), message.clone()),
            },
            StudioEvent::LogReceived {
                entry: StudioLogEntry::new(StudioLogLevel::Error, "browser-esp32-flash", message),
            },
            StudioEvent::ProvisioningIssueRaised {
                action_id: Some(action_id),
                issue,
            },
        ]
    }

    fn project_client(&mut self) -> Result<&mut BrowserSerialProtocolClient, StudioRuntimeError> {
        self.client
            .as_mut()
            .ok_or(StudioRuntimeError::MissingClient)
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
            } => self.probe_target(action_id, endpoint_id).await,
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
                endpoint_id,
                firmware_id,
            } => {
                self.flash_firmware(action_id, endpoint_id, firmware_id)
                    .await
            }
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
        StudioActionKind::from(LinkActionRequest::SelectProvider {
            provider_id: BROWSER_SERIAL_ESP32_PROVIDER_ID,
        }),
        ActionOrigin::System,
    );
    let mut runtime = BrowserSerialStudioRuntime::new();
    let effects = app.dispatch_kind(
        StudioActionKind::from(LinkActionRequest::RequestDeviceAccess),
        ActionOrigin::User,
    );
    drain_effects(&mut app, &mut runtime, effects).await?;
    let endpoint_id = app
        .state()
        .device_manager
        .providers
        .first_selected_endpoint()
        .ok_or_else(|| {
            StudioRuntimeError::Link(
                "browser serial permission did not yield an endpoint".to_string(),
            )
        })?
        .id
        .clone();
    let effects = app.dispatch_kind(
        StudioActionKind::from(LinkActionRequest::ConnectEndpoint { endpoint_id }),
        ActionOrigin::User,
    );
    drain_effects(&mut app, &mut runtime, effects).await?;
    let effects = app.dispatch_kind(
        StudioActionKind::from(ProjectActionRequest::UploadDemoProject),
        ActionOrigin::User,
    );
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

fn browser_serial_provider_capabilities(flash_available: bool) -> Vec<ProviderCapability> {
    let mut capabilities = vec![
        ProviderCapability::RequestAccess,
        ProviderCapability::DiscoverEndpoints,
        ProviderCapability::Connect,
        ProviderCapability::ResetDevice,
        ProviderCapability::ReadLogs,
        ProviderCapability::ReadDiagnostics,
        ProviderCapability::ReadHeartbeat,
        ProviderCapability::DeployProject,
        ProviderCapability::ReadProjectInventory,
    ];
    if flash_available {
        capabilities.push(ProviderCapability::FlashFirmware);
    }
    capabilities
}

fn browser_serial_capabilities(flash_available: bool) -> Vec<DeviceCapability> {
    let mut capabilities = vec![
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
    ];
    if flash_available {
        capabilities.push(DeviceCapability::FlashFirmware);
    }
    capabilities
}

fn map_flash_progress(progress: BrowserEsp32FlashProgress) -> ProgressState {
    let mut state = ProgressState::new(progress.label);
    if let Some(total_steps) = progress.total_steps {
        state = state.with_steps(progress.completed_steps, total_steps);
    } else {
        state.completed_steps = progress.completed_steps;
    }
    if let Some(percent) = progress.percent {
        state = state.with_percent(percent as u8);
    }
    state
}

fn is_supported_esp32c6_chip(chip_name: &str) -> bool {
    let normalized = chip_name.to_ascii_lowercase().replace(['-', '_', ' '], "");
    normalized.contains("esp32c6")
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
