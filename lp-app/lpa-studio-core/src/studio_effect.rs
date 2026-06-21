use crate::ActionId;
use lpa_link::LinkProviderKind;
use lpa_link::provider::endpoint::LinkEndpointId;
use lpa_link::provider::session::LinkSessionId;
use lpc_wire::WireProjectHandle;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum StudioEffect {
    RefreshProviderCatalog {
        action_id: ActionId,
    },
    RequestDeviceAccess {
        action_id: ActionId,
        provider_id: LinkProviderKind,
    },
    DiscoverEndpoints {
        action_id: ActionId,
        provider_id: LinkProviderKind,
    },
    ConnectEndpoint {
        action_id: ActionId,
        endpoint_id: LinkEndpointId,
    },
    ProbeTarget {
        action_id: ActionId,
        endpoint_id: LinkEndpointId,
    },
    DisconnectSession {
        action_id: ActionId,
        session_id: LinkSessionId,
    },
    ResetDevice {
        action_id: ActionId,
        endpoint_id: LinkEndpointId,
    },
    FlashDeviceFirmware {
        action_id: ActionId,
        endpoint_id: LinkEndpointId,
        firmware_id: Option<String>,
    },
    SeedDemoProject {
        action_id: ActionId,
        project_id: String,
    },
    LoadProject {
        action_id: ActionId,
        project_id: String,
    },
    RefreshStatus {
        action_id: ActionId,
    },
    ReadProjectState {
        action_id: ActionId,
    },
    ReadProjectInventory {
        action_id: ActionId,
        handle: WireProjectHandle,
    },
}
