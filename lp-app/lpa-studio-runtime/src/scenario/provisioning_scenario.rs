use crate::scenario::{
    AccessOutcome, ConnectOutcome, ConnectionOutcome, FlashOutcome, ProbeOutcome, ProjectOutcome,
    ProjectStateOutcome,
};
use lpa_link::link_endpoint::LinkEndpointId;
use lpa_link::link_provider::LinkProviderId;
use lpa_link::{LinkConnectionKind, LinkEndpoint};
use lpa_studio_core::{
    BROWSER_SERIAL_ESP32_PROVIDER_ID, DeviceCapability, DeviceIssue, DeviceIssueKind,
    HOST_PROCESS_PROVIDER_ID, ProjectChoice, ProjectStateResult, ProviderAvailability,
    ProviderCapability, ProviderCardState, ProviderIntent, RecoveryAction, RecoveryReason,
    STUDIO_DEMO_PROJECT_ID,
};
use serde::{Deserialize, Serialize};

const HOST_ENDPOINT_ID: &str = "scenario-host-runtime";
const BROWSER_SERIAL_ENDPOINT_ID: &str = "scenario-browser-serial-esp32";

/// A deterministic product-level link journey for Studio runtime tests.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct ProvisioningScenario {
    pub name: String,
    pub providers: Vec<ProviderCardState>,
    pub access: AccessOutcome,
    pub connect: ConnectOutcome,
    pub probe: ProbeOutcome,
    pub flash: FlashOutcome,
    pub project_state: ProjectStateOutcome,
    pub project: ProjectOutcome,
    pub connection: ConnectionOutcome,
}

impl ProvisioningScenario {
    pub fn host_runtime_ready() -> Self {
        Self::host("host-runtime-ready")
    }

    pub fn web_serial_unsupported() -> Self {
        Self::browser_serial("web-serial-unsupported").with_access(AccessOutcome::unsupported(
            "Web Serial is not supported in this browser.",
        ))
    }

    pub fn permission_denied() -> Self {
        Self::browser_serial("permission-denied").with_access(AccessOutcome::permission_denied(
            "The browser denied access to the selected device.",
        ))
    }

    pub fn permission_canceled() -> Self {
        Self::browser_serial("permission-canceled").with_access(AccessOutcome::permission_canceled(
            "The device chooser was canceled.",
        ))
    }

    pub fn endpoint_open_failure() -> Self {
        Self::host("endpoint-open-failure").with_connect(ConnectOutcome::failed(issue(
            "endpoint-open-failed",
            DeviceIssueKind::EndpointOpenFailed,
            "The selected endpoint could not be opened.",
            vec![RecoveryAction::Retry, RecoveryAction::ChooseSimulator],
        )))
    }

    pub fn server_timeout() -> Self {
        Self::host("server-timeout").with_probe(ProbeOutcome::timeout(issue(
            "server-timeout",
            DeviceIssueKind::ServerTimeout,
            "A LightPlayer server did not respond before the timeout.",
            vec![RecoveryAction::Retry, RecoveryAction::ResetDevice],
        )))
    }

    pub fn incompatible_firmware() -> Self {
        Self::browser_serial("incompatible-firmware").with_probe(
            ProbeOutcome::incompatible_firmware(
                Some("0.0.1".to_string()),
                issue(
                    "incompatible-firmware",
                    DeviceIssueKind::IncompatibleFirmware,
                    "The device is running incompatible LightPlayer firmware.",
                    vec![RecoveryAction::FlashFirmware { firmware_id: None }],
                ),
            ),
        )
    }

    pub fn blank_device_flash_available() -> Self {
        Self::browser_serial("blank-device-flash-available").with_probe(ProbeOutcome::Blank)
    }

    pub fn flash_succeeds() -> Self {
        Self::blank_device_flash_available().with_name("flash-succeeds")
    }

    pub fn flash_unavailable() -> Self {
        Self::blank_device_flash_available()
            .with_name("flash-unavailable")
            .with_flash(FlashOutcome::unavailable(issue(
                "flash-unavailable",
                DeviceIssueKind::FlashFailed,
                "Firmware flashing is unavailable in this runtime.",
                vec![RecoveryAction::ChooseSimulator],
            )))
    }

    pub fn flash_artifact_missing() -> Self {
        Self::blank_device_flash_available()
            .with_name("flash-artifact-missing")
            .with_flash(FlashOutcome::artifact_missing(issue(
                "flash-artifact-missing",
                DeviceIssueKind::FirmwareArtifactMissing,
                "The LightPlayer firmware artifact is missing from this Studio build.",
                vec![
                    RecoveryAction::Retry,
                    RecoveryAction::OpenHelp {
                        topic: "firmware packaging".to_string(),
                    },
                ],
            )))
    }

    pub fn flash_fails() -> Self {
        Self::blank_device_flash_available()
            .with_name("flash-fails")
            .with_flash(FlashOutcome::fails(issue(
                "flash-failed",
                DeviceIssueKind::FlashFailed,
                "Firmware flashing failed.",
                vec![RecoveryAction::Retry, RecoveryAction::ResetDevice],
            )))
    }

    pub fn flash_reconnect_fails() -> Self {
        Self::blank_device_flash_available()
            .with_name("flash-reconnect-fails")
            .with_flash(FlashOutcome::reconnect_fails(issue(
                "post-flash-reconnect-failed",
                DeviceIssueKind::ConnectionLost,
                "Firmware was flashed, but Studio could not reconnect to the device.",
                vec![RecoveryAction::Reconnect, RecoveryAction::ResetDevice],
            )))
    }

    pub fn project_deploy_fails() -> Self {
        Self::host("project-deploy-fails").with_project(ProjectOutcome::deploy_fails(issue(
            "project-deploy-failed",
            DeviceIssueKind::ProjectDeployFailed,
            "The demo project could not be written to the target.",
            vec![RecoveryAction::Retry],
        )))
    }

    pub fn no_loaded_project() -> Self {
        Self::host("no-loaded-project").with_project_state(ProjectStateResult::NoLoadedProject)
    }

    pub fn multiple_loaded_projects() -> Self {
        Self::host("multiple-loaded-projects").with_project_state(
            ProjectStateResult::MultipleProjects {
                projects: vec![
                    ProjectChoice::new(
                        STUDIO_DEMO_PROJECT_ID,
                        project_path(),
                        lpc_wire::WireProjectHandle::new(1),
                    ),
                    ProjectChoice::new(
                        "gallery",
                        "/projects/gallery",
                        lpc_wire::WireProjectHandle::new(2),
                    ),
                ],
            },
        )
    }

    pub fn recovery_required() -> Self {
        Self::host("recovery-required").with_project_state(ProjectStateResult::RecoveryRequired {
            reason: RecoveryReason::ProjectCrash {
                project_id: Some(STUDIO_DEMO_PROJECT_ID.to_string()),
                message: Some("The previously loaded project failed during boot.".to_string()),
            },
        })
    }

    pub fn post_flash_project_state_fails() -> Self {
        Self::flash_succeeds()
            .with_name("post-flash-project-state-fails")
            .with_project_state_failure(issue(
                "post-flash-project-state-failed",
                DeviceIssueKind::ServerTimeout,
                "Firmware was flashed, but Studio could not read project state from the server.",
                vec![RecoveryAction::Retry, RecoveryAction::Reconnect],
            ))
    }

    pub fn project_load_fails() -> Self {
        Self::host("project-load-fails").with_project(ProjectOutcome::load_fails(issue(
            "project-load-failed",
            DeviceIssueKind::ProjectLoadFailed,
            "The demo project was written but could not be loaded.",
            vec![RecoveryAction::Retry],
        )))
    }

    pub fn connection_lost() -> Self {
        Self::host("connection-lost").with_connection(ConnectionOutcome::lost(issue(
            "connection-lost",
            DeviceIssueKind::ConnectionLost,
            "The device connection was lost.",
            vec![RecoveryAction::Reconnect],
        )))
    }

    pub fn primary_provider_id(&self) -> Option<&LinkProviderId> {
        self.providers.first().map(|provider| &provider.provider_id)
    }

    pub fn first_endpoint_id(&self) -> Option<LinkEndpointId> {
        self.providers
            .iter()
            .flat_map(|provider| provider.endpoints.iter())
            .map(|endpoint| endpoint.id.clone())
            .next()
    }

    pub fn endpoints_for(&self, provider_id: &LinkProviderId) -> Vec<LinkEndpoint> {
        self.providers
            .iter()
            .find(|provider| provider.provider_id == *provider_id)
            .map(|provider| provider.endpoints.clone())
            .unwrap_or_default()
    }

    pub fn provider_id_for_endpoint(&self, endpoint_id: &LinkEndpointId) -> Option<LinkProviderId> {
        self.providers
            .iter()
            .find(|provider| {
                provider
                    .endpoints
                    .iter()
                    .any(|endpoint| endpoint.id == *endpoint_id)
            })
            .map(|provider| provider.provider_id.clone())
    }

    fn host(name: impl Into<String>) -> Self {
        let provider = ProviderCardState::new(
            HOST_PROCESS_PROVIDER_ID,
            "Host runtime",
            ProviderIntent::RunHostRuntime,
        )
        .with_availability(ProviderAvailability::Available)
        .with_capabilities(common_provider_capabilities())
        .with_endpoints(vec![LinkEndpoint::new(
            HOST_ENDPOINT_ID,
            HOST_PROCESS_PROVIDER_ID,
            "Scenario host runtime",
        )]);
        Self::new(name, provider, host_connection())
    }

    fn browser_serial(name: impl Into<String>) -> Self {
        let provider = ProviderCardState::new(
            BROWSER_SERIAL_ESP32_PROVIDER_ID,
            "USB ESP32",
            ProviderIntent::ConnectUsbEsp32,
        )
        .with_availability(ProviderAvailability::AvailableWithPermission)
        .with_capabilities({
            let mut capabilities = common_provider_capabilities();
            capabilities.push(ProviderCapability::RequestAccess);
            capabilities.push(ProviderCapability::FlashFirmware);
            capabilities
        })
        .with_endpoints(vec![LinkEndpoint::new(
            BROWSER_SERIAL_ENDPOINT_ID,
            BROWSER_SERIAL_ESP32_PROVIDER_ID,
            "Scenario USB ESP32",
        )]);
        Self::new(name, provider, browser_serial_connection())
    }

    fn new(name: impl Into<String>, provider: ProviderCardState, connect: ConnectOutcome) -> Self {
        Self {
            name: name.into(),
            providers: vec![provider],
            access: AccessOutcome::Granted,
            connect,
            probe: ProbeOutcome::server(Some("scenario-server".to_string())),
            flash: FlashOutcome::Succeeds,
            project_state: ProjectStateOutcome::succeeds(ProjectStateResult::loaded_project(
                STUDIO_DEMO_PROJECT_ID,
                project_path(),
                lpc_wire::WireProjectHandle::new(1),
            )),
            project: ProjectOutcome::succeeds(),
            connection: ConnectionOutcome::Healthy,
        }
    }

    fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    fn with_access(mut self, access: AccessOutcome) -> Self {
        self.access = access;
        self
    }

    fn with_connect(mut self, connect: ConnectOutcome) -> Self {
        self.connect = connect;
        self
    }

    fn with_probe(mut self, probe: ProbeOutcome) -> Self {
        self.probe = probe;
        self
    }

    fn with_flash(mut self, flash: FlashOutcome) -> Self {
        self.flash = flash;
        self
    }

    fn with_project_state(mut self, project_state: ProjectStateResult) -> Self {
        self.project_state = ProjectStateOutcome::succeeds(project_state);
        self
    }

    fn with_project_state_failure(mut self, issue: DeviceIssue) -> Self {
        self.project_state = ProjectStateOutcome::fails(issue);
        self
    }

    fn with_project(mut self, project: ProjectOutcome) -> Self {
        self.project = project;
        self
    }

    fn with_connection(mut self, connection: ConnectionOutcome) -> Self {
        self.connection = connection;
        self
    }
}

fn host_connection() -> ConnectOutcome {
    let mut capabilities = common_device_capabilities();
    capabilities.push(DeviceCapability::UseHostProcess);
    ConnectOutcome::connected(
        "scenario-host-session-1",
        LinkConnectionKind::Fake,
        capabilities,
    )
}

fn browser_serial_connection() -> ConnectOutcome {
    let mut capabilities = common_device_capabilities();
    capabilities.push(DeviceCapability::RequestDeviceAccess);
    capabilities.push(DeviceCapability::UseBrowserSerialEsp32);
    capabilities.push(DeviceCapability::FlashFirmware);
    ConnectOutcome::connected(
        "scenario-browser-serial-session-1",
        LinkConnectionKind::BrowserSerialEsp32 {
            protocol: "scenario-serial-json-lines-v1".to_string(),
        },
        capabilities,
    )
}

fn common_provider_capabilities() -> Vec<ProviderCapability> {
    vec![
        ProviderCapability::DiscoverEndpoints,
        ProviderCapability::Connect,
        ProviderCapability::ReadLogs,
        ProviderCapability::ReadDiagnostics,
        ProviderCapability::ReadHeartbeat,
        ProviderCapability::DeployProject,
        ProviderCapability::ReadProjectInventory,
    ]
}

fn common_device_capabilities() -> Vec<DeviceCapability> {
    vec![
        DeviceCapability::Connect,
        DeviceCapability::WriteProjectFiles,
        DeviceCapability::ReadHeartbeat,
        DeviceCapability::ListProjects,
        DeviceCapability::LoadProject,
        DeviceCapability::ReadProjectInventory,
        DeviceCapability::ReadLogs,
        DeviceCapability::ReadDiagnostics,
    ]
}

fn issue(
    id: impl Into<String>,
    kind: DeviceIssueKind,
    message: impl Into<String>,
    recovery_actions: Vec<RecoveryAction>,
) -> DeviceIssue {
    DeviceIssue::error(id, kind, message).with_recovery_actions(recovery_actions)
}

pub(crate) fn project_path() -> String {
    format!("/projects/{STUDIO_DEMO_PROJECT_ID}")
}
