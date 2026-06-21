use lpa_link::link_endpoint::LinkEndpointId;
use lpa_link::link_provider::LinkProviderId;
use lpa_studio_core::{
    ActionId, DeviceAccessStatus, DeviceIssue, ProgressState, ProvisioningReason,
    StudioDiagnostic, StudioEffect, StudioEvent, StudioHeartbeat, StudioLogEntry,
    StudioLogLevel, TargetKind, TargetProbeResult, STUDIO_DEMO_PROJECT_ID,
};
use lpc_model::AsLpPathBuf;
use lpc_wire::LoadedProject;

use crate::effect_executor::EffectExecutor;
use crate::scenario::{
    AccessOutcome, ConnectOutcome, ConnectionOutcome, FlashOutcome, ProbeOutcome, ProjectOutcome,
    ProjectStateOutcome, ProvisioningScenario,
};
use crate::StudioRuntimeError;

/// Effect executor that maps a `ProvisioningScenario` into real Studio events.
#[derive(Clone, Debug)]
pub struct ScenarioRuntime {
    scenario: ProvisioningScenario,
}

impl ScenarioRuntime {
    pub fn new(scenario: ProvisioningScenario) -> Self {
        Self { scenario }
    }

    pub fn scenario(&self) -> &ProvisioningScenario {
        &self.scenario
    }

    fn provider_id_for_endpoint(&self, endpoint_id: &LinkEndpointId) -> LinkProviderId {
        self.scenario
            .provider_id_for_endpoint(endpoint_id)
            .or_else(|| self.scenario.primary_provider_id().cloned())
            .unwrap_or_else(|| LinkProviderId::new("scenario"))
    }
}

impl EffectExecutor for ScenarioRuntime {
    async fn execute_effect(
        &mut self,
        effect: StudioEffect,
    ) -> Result<Vec<StudioEvent>, StudioRuntimeError> {
        match effect {
            StudioEffect::RefreshProviderCatalog { action_id } => {
                Ok(vec![StudioEvent::ProviderCatalogUpdated {
                    action_id: Some(action_id),
                    providers: self.scenario.providers.clone(),
                }])
            }
            StudioEffect::RequestDeviceAccess {
                action_id,
                provider_id,
            } => Ok(access_events(
                action_id,
                provider_id,
                &self.scenario.access,
                &self.scenario,
            )),
            StudioEffect::DiscoverEndpoints {
                action_id,
                provider_id,
            } => Ok(vec![StudioEvent::EndpointsDiscovered {
                action_id,
                endpoints: self.scenario.endpoints_for(&provider_id),
                provider_id,
            }]),
            StudioEffect::ConnectEndpoint {
                action_id,
                endpoint_id,
            } => Ok(connect_events(
                action_id,
                endpoint_id,
                &self.scenario.connect,
                self,
            )),
            StudioEffect::ProbeTarget {
                action_id,
                endpoint_id,
            } => Ok(probe_events(action_id, endpoint_id, &self.scenario.probe)),
            StudioEffect::DisconnectSession {
                action_id,
                session_id,
            } => Ok(vec![StudioEvent::DeviceDisconnected {
                action_id,
                session_id,
            }]),
            StudioEffect::ResetDevice {
                action_id,
                endpoint_id,
            } => Ok(vec![StudioEvent::DeviceReset {
                action_id,
                endpoint_id,
            }]),
            StudioEffect::FlashDeviceFirmware {
                action_id,
                endpoint_id,
                firmware_id,
            } => Ok(flash_events(
                action_id,
                endpoint_id,
                firmware_id,
                &self.scenario.flash,
            )),
            StudioEffect::SeedDemoProject {
                action_id,
                project_id,
            } => Ok(seed_project_events(
                action_id,
                project_id,
                &self.scenario.project,
            )),
            StudioEffect::LoadProject {
                action_id,
                project_id,
            } => Ok(load_project_events(
                action_id,
                project_id,
                &self.scenario.project,
            )),
            StudioEffect::RefreshStatus { action_id } => Ok(refresh_status_events(
                action_id,
                &self.scenario.connection,
                &self.scenario.project,
            )),
            StudioEffect::ReadProjectState { action_id } => Ok(project_state_events(
                action_id,
                &self.scenario.project_state,
            )),
            StudioEffect::ReadProjectInventory {
                action_id,
                handle: _,
            } => Ok(vec![StudioEvent::ProjectInventoryRead {
                action_id,
                inventory: self.scenario.project.inventory(),
            }]),
        }
    }
}

fn access_events(
    action_id: ActionId,
    provider_id: LinkProviderId,
    access: &AccessOutcome,
    scenario: &ProvisioningScenario,
) -> Vec<StudioEvent> {
    match access {
        AccessOutcome::Granted => vec![
            StudioEvent::DeviceAccessUpdated {
                action_id: Some(action_id),
                provider_id: provider_id.clone(),
                status: DeviceAccessStatus::Granted,
            },
            StudioEvent::EndpointsDiscovered {
                action_id,
                endpoints: scenario.endpoints_for(&provider_id),
                provider_id,
            },
        ],
        AccessOutcome::PermissionCanceled { reason } => vec![StudioEvent::DeviceAccessUpdated {
            action_id: Some(action_id),
            provider_id,
            status: DeviceAccessStatus::PermissionCanceled {
                reason: reason.clone(),
            },
        }],
        AccessOutcome::PermissionDenied { reason } => vec![StudioEvent::DeviceAccessUpdated {
            action_id: Some(action_id),
            provider_id,
            status: DeviceAccessStatus::PermissionDenied {
                reason: reason.clone(),
            },
        }],
        AccessOutcome::Unsupported { reason } => vec![StudioEvent::DeviceAccessUpdated {
            action_id: Some(action_id),
            provider_id,
            status: DeviceAccessStatus::Unsupported {
                reason: reason.clone(),
            },
        }],
    }
}

fn connect_events(
    action_id: ActionId,
    endpoint_id: LinkEndpointId,
    connect: &ConnectOutcome,
    runtime: &ScenarioRuntime,
) -> Vec<StudioEvent> {
    match connect {
        ConnectOutcome::Connected {
            session_id,
            connection_kind,
            capabilities,
        } => vec![StudioEvent::DeviceConnected {
            action_id,
            provider_id: runtime.provider_id_for_endpoint(&endpoint_id),
            endpoint_id,
            session_id: session_id.clone(),
            connection_kind: connection_kind.clone(),
            capabilities: capabilities.clone(),
        }],
        ConnectOutcome::Failed { issue } => vec![StudioEvent::DeviceConnectionFailed {
            action_id,
            endpoint_id: endpoint_id.clone(),
            issue: issue_for_endpoint(issue, endpoint_id),
        }],
    }
}

fn probe_events(
    action_id: ActionId,
    endpoint_id: LinkEndpointId,
    probe: &ProbeOutcome,
) -> Vec<StudioEvent> {
    match probe {
        ProbeOutcome::Server { version } => vec![StudioEvent::TargetProbeCompleted {
            action_id,
            result: TargetProbeResult::server(endpoint_id, version.clone()),
        }],
        ProbeOutcome::Bootloader => vec![StudioEvent::TargetProbeCompleted {
            action_id,
            result: TargetProbeResult {
                endpoint_id,
                kind: TargetKind::Bootloader,
                server_version: None,
                capabilities: Vec::new(),
                provisioning_reason: Some(ProvisioningReason::BootloaderMode),
                issue: None,
            },
        }],
        ProbeOutcome::Blank => vec![StudioEvent::TargetProbeCompleted {
            action_id,
            result: TargetProbeResult {
                endpoint_id,
                kind: TargetKind::BlankDevice,
                server_version: None,
                capabilities: Vec::new(),
                provisioning_reason: Some(ProvisioningReason::DeviceBlank),
                issue: None,
            },
        }],
        ProbeOutcome::Unsupported { issue } => vec![StudioEvent::TargetProbeCompleted {
            action_id,
            result: TargetProbeResult {
                endpoint_id: endpoint_id.clone(),
                kind: TargetKind::UnsupportedDevice,
                server_version: None,
                capabilities: Vec::new(),
                provisioning_reason: None,
                issue: Some(issue_for_endpoint(issue, endpoint_id)),
            },
        }],
        ProbeOutcome::Timeout { issue } => vec![StudioEvent::TargetProbeFailed {
            action_id,
            endpoint_id: endpoint_id.clone(),
            issue: issue_for_endpoint(issue, endpoint_id),
        }],
        ProbeOutcome::IncompatibleFirmware { version, issue } => {
            vec![StudioEvent::TargetProbeCompleted {
                action_id,
                result: TargetProbeResult {
                    endpoint_id: endpoint_id.clone(),
                    kind: TargetKind::LightPlayerServer,
                    server_version: version.clone(),
                    capabilities: Vec::new(),
                    provisioning_reason: Some(ProvisioningReason::FirmwareIncompatible {
                        version: version.clone(),
                    }),
                    issue: Some(issue_for_endpoint(issue, endpoint_id)),
                },
            }]
        }
    }
}

fn flash_events(
    action_id: ActionId,
    endpoint_id: LinkEndpointId,
    firmware_id: Option<String>,
    flash: &FlashOutcome,
) -> Vec<StudioEvent> {
    match flash {
        FlashOutcome::Succeeds => vec![
            StudioEvent::ProvisioningProgressUpdated {
                action_id: Some(action_id),
                progress: ProgressState::new("Flashing firmware")
                    .with_steps(1, 2)
                    .with_percent(50),
            },
            StudioEvent::FirmwareFlashCompleted {
                action_id,
                endpoint_id,
                firmware_id,
            },
        ],
        FlashOutcome::Unavailable { issue }
        | FlashOutcome::ArtifactMissing { issue }
        | FlashOutcome::Fails { issue } => {
            vec![StudioEvent::ProvisioningIssueRaised {
                action_id: Some(action_id),
                issue: issue_for_endpoint(issue, endpoint_id),
            }]
        }
        FlashOutcome::ReconnectFails { issue } => vec![
            StudioEvent::ProvisioningProgressUpdated {
                action_id: Some(action_id),
                progress: ProgressState::new("Flashing firmware")
                    .with_steps(2, 2)
                    .with_percent(100),
            },
            StudioEvent::FirmwareFlashCompleted {
                action_id,
                endpoint_id: endpoint_id.clone(),
                firmware_id,
            },
            StudioEvent::ProvisioningIssueRaised {
                action_id: Some(action_id),
                issue: issue_for_endpoint(issue, endpoint_id),
            },
        ],
    }
}

fn project_state_events(action_id: ActionId, outcome: &ProjectStateOutcome) -> Vec<StudioEvent> {
    match outcome {
        ProjectStateOutcome::Succeeds(result) => vec![StudioEvent::ProjectStateRead {
            action_id,
            result: result.clone(),
        }],
        ProjectStateOutcome::Fails { issue } => vec![StudioEvent::ProvisioningIssueRaised {
            action_id: Some(action_id),
            issue: issue.clone(),
        }],
    }
}

fn seed_project_events(
    action_id: ActionId,
    project_id: String,
    project: &ProjectOutcome,
) -> Vec<StudioEvent> {
    match project {
        ProjectOutcome::DeployFails { issue } => vec![StudioEvent::ProvisioningIssueRaised {
            action_id: Some(action_id),
            issue: issue.clone(),
        }],
        ProjectOutcome::Succeeds { .. } | ProjectOutcome::LoadFails { .. } => {
            vec![StudioEvent::DemoProjectSeeded {
                action_id,
                project_id,
            }]
        }
    }
}

fn load_project_events(
    action_id: ActionId,
    project_id: String,
    project: &ProjectOutcome,
) -> Vec<StudioEvent> {
    match project {
        ProjectOutcome::LoadFails { issue } | ProjectOutcome::DeployFails { issue } => {
            vec![StudioEvent::ProvisioningIssueRaised {
                action_id: Some(action_id),
                issue: issue.clone(),
            }]
        }
        ProjectOutcome::Succeeds { handle, .. } => vec![StudioEvent::ProjectLoaded {
            action_id,
            project_id,
            handle: *handle,
        }],
    }
}

fn refresh_status_events(
    action_id: ActionId,
    connection: &ConnectionOutcome,
    project: &ProjectOutcome,
) -> Vec<StudioEvent> {
    let mut events = vec![StudioEvent::LoadedProjectsRefreshed {
        action_id,
        projects: vec![LoadedProject {
            handle: project.handle(),
            path: crate::scenario::provisioning_scenario::project_path().as_path_buf(),
        }],
    }];
    match connection {
        ConnectionOutcome::Healthy => {
            events.push(StudioEvent::HeartbeatReceived {
                heartbeat: StudioHeartbeat {
                    fps_avg: 60.0,
                    frame_count: 120,
                    loaded_project_count: 1,
                    uptime_ms: 5_000,
                    free_memory_bytes: Some(128 * 1024),
                },
            });
            events.push(StudioEvent::LogReceived {
                entry: StudioLogEntry::new(
                    StudioLogLevel::Info,
                    "scenario",
                    format!("{STUDIO_DEMO_PROJECT_ID} status refreshed"),
                ),
            });
        }
        ConnectionOutcome::Degraded { issue } | ConnectionOutcome::Lost { issue } => {
            events.push(StudioEvent::ProvisioningIssueRaised {
                action_id: None,
                issue: issue.clone(),
            });
            events.push(StudioEvent::DiagnosticRaised {
                diagnostic: StudioDiagnostic::error(None, issue.message.clone()),
            });
        }
    }
    events
}

fn issue_for_endpoint(issue: &DeviceIssue, endpoint_id: LinkEndpointId) -> DeviceIssue {
    let mut issue = issue.clone();
    if issue.endpoint_id.is_none() {
        issue.endpoint_id = Some(endpoint_id);
    }
    issue
}

#[cfg(test)]
mod tests {
    use lpa_link::link_provider::LinkProviderId;
    use lpa_studio_core::{
        ActionId, DeviceAccessStatus, DeviceIssueKind, StudioEffect,
        StudioEvent, BROWSER_SERIAL_ESP32_PROVIDER_ID,
    };

    use super::*;

    #[tokio::test]
    async fn refresh_catalog_emits_scenario_providers() {
        let mut runtime = ScenarioRuntime::new(ProvisioningScenario::host_runtime_ready());

        let events = runtime
            .execute_effect(StudioEffect::RefreshProviderCatalog {
                action_id: ActionId::new(1),
            })
            .await
            .unwrap();

        assert!(matches!(
            &events[0],
            StudioEvent::ProviderCatalogUpdated { providers, .. }
                if providers.len() == 1
        ));
    }

    #[tokio::test]
    async fn access_denied_maps_to_access_status() {
        let mut runtime = ScenarioRuntime::new(ProvisioningScenario::permission_denied());

        let events = runtime
            .execute_effect(StudioEffect::RequestDeviceAccess {
                action_id: ActionId::new(1),
                provider_id: LinkProviderId::new(BROWSER_SERIAL_ESP32_PROVIDER_ID),
            })
            .await
            .unwrap();

        assert!(matches!(
            &events[0],
            StudioEvent::DeviceAccessUpdated {
                status: DeviceAccessStatus::PermissionDenied { .. },
                ..
            }
        ));
    }

    #[tokio::test]
    async fn flash_success_emits_progress_then_completion() {
        let mut runtime = ScenarioRuntime::new(ProvisioningScenario::flash_succeeds());

        let events = runtime
            .execute_effect(StudioEffect::FlashDeviceFirmware {
                action_id: ActionId::new(1),
                endpoint_id: LinkEndpointId::new("scenario-browser-serial-esp32"),
                firmware_id: None,
            })
            .await
            .unwrap();

        assert!(matches!(
            events.as_slice(),
            [
                StudioEvent::ProvisioningProgressUpdated { .. },
                StudioEvent::FirmwareFlashCompleted { .. }
            ]
        ));
    }

    #[test]
    fn issue_for_endpoint_preserves_existing_endpoint() {
        let issue = DeviceIssue::error("issue", DeviceIssueKind::ActionFailed, "already scoped")
            .with_endpoint("existing");

        let scoped = issue_for_endpoint(&issue, LinkEndpointId::new("new"));

        assert_eq!(scoped.endpoint_id.as_ref().unwrap().as_str(), "existing");
    }
}
