use crate::{
    ActionId, DeviceAccessStatus, DeviceCapability, DeviceIssue, ProgressState, ProjectStateResult,
    ProviderAvailability, ProviderCardState, StudioDiagnostic, StudioHeartbeat, StudioLogEntry,
    TargetProbeResult,
};
use lpa_link::link_endpoint::LinkEndpointId;
use lpa_link::link_provider::LinkProviderId;
use lpa_link::link_session::LinkSessionId;
use lpa_link::{LinkConnectionKind, LinkEndpoint};
use lpc_wire::{LoadedProject, WireProjectHandle, WireProjectInventoryReadResponse};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum StudioEvent {
    ProviderCatalogUpdated {
        action_id: Option<ActionId>,
        providers: Vec<ProviderCardState>,
    },
    ProviderAvailabilityUpdated {
        action_id: Option<ActionId>,
        provider_id: LinkProviderId,
        availability: ProviderAvailability,
    },
    DeviceAccessUpdated {
        action_id: Option<ActionId>,
        provider_id: LinkProviderId,
        status: DeviceAccessStatus,
    },
    EndpointsDiscovered {
        action_id: ActionId,
        provider_id: LinkProviderId,
        endpoints: Vec<LinkEndpoint>,
    },
    DeviceConnected {
        action_id: ActionId,
        provider_id: LinkProviderId,
        endpoint_id: LinkEndpointId,
        session_id: LinkSessionId,
        connection_kind: LinkConnectionKind,
        capabilities: Vec<DeviceCapability>,
    },
    DeviceConnectionFailed {
        action_id: ActionId,
        endpoint_id: LinkEndpointId,
        issue: DeviceIssue,
    },
    DeviceDisconnected {
        action_id: ActionId,
        session_id: LinkSessionId,
    },
    DeviceReset {
        action_id: ActionId,
        endpoint_id: LinkEndpointId,
    },
    FirmwareFlashCompleted {
        action_id: ActionId,
        endpoint_id: LinkEndpointId,
        firmware_id: Option<String>,
    },
    TargetProbeCompleted {
        action_id: ActionId,
        result: TargetProbeResult,
    },
    TargetProbeFailed {
        action_id: ActionId,
        endpoint_id: LinkEndpointId,
        issue: DeviceIssue,
    },
    ProvisioningIssueRaised {
        action_id: Option<ActionId>,
        issue: DeviceIssue,
    },
    ProvisioningProgressUpdated {
        action_id: Option<ActionId>,
        progress: ProgressState,
    },
    ProvisioningFlowCanceled {
        action_id: ActionId,
    },
    DemoProjectSeeded {
        action_id: ActionId,
        project_id: String,
    },
    ProjectLoaded {
        action_id: ActionId,
        project_id: String,
        handle: WireProjectHandle,
    },
    ProjectInventoryRead {
        action_id: ActionId,
        inventory: WireProjectInventoryReadResponse,
    },
    LoadedProjectsRefreshed {
        action_id: ActionId,
        projects: Vec<LoadedProject>,
    },
    ProjectStateRead {
        action_id: ActionId,
        result: ProjectStateResult,
    },
    HeartbeatReceived {
        heartbeat: StudioHeartbeat,
    },
    LogReceived {
        entry: StudioLogEntry,
    },
    DiagnosticRaised {
        diagnostic: StudioDiagnostic,
    },
    ActionFailed {
        action_id: ActionId,
        message: String,
    },
}
