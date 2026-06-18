use lpa_link::{LinkEndpointId, LinkProviderId, LinkSessionId};
use lpc_wire::WireProjectHandle;
use serde::{Deserialize, Serialize};

use crate::ActionId;

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum StudioEffect {
    DiscoverEndpoints {
        action_id: ActionId,
        provider_id: LinkProviderId,
    },
    ConnectEndpoint {
        action_id: ActionId,
        endpoint_id: LinkEndpointId,
    },
    DisconnectSession {
        action_id: ActionId,
        session_id: LinkSessionId,
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
    ReadProjectInventory {
        action_id: ActionId,
        handle: WireProjectHandle,
    },
}
