use lpa_link::{LinkConnectionKind, LinkEndpoint, LinkEndpointId, LinkProviderId, LinkSessionId};
use lpc_wire::{LoadedProject, WireProjectHandle, WireProjectInventoryReadResponse};
use serde::{Deserialize, Serialize};

use crate::{ActionId, DeviceCapability, StudioDiagnostic, StudioHeartbeat, StudioLogEntry};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum StudioEvent {
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
    DeviceDisconnected {
        action_id: ActionId,
        session_id: LinkSessionId,
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
