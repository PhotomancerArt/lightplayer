//! Project commands that are not runtime project reads.

use crate::{
    WireCreateNodeRequest, WireCreateNodeResponse, WireNodeCommand, WireNodeCommandResponse,
    WireOverlayCommitRequest, WireOverlayCommitResponse, WireOverlayMutationRequest,
    WireOverlayMutationResponse, WireOverlayReadRequest, WireOverlayReadResponse,
    WireProjectInventoryReadRequest, WireProjectInventoryReadResponse, WireRemoveNodeRequest,
    WireRemoveNodeResponse,
};
use lpc_model::NodeId;

/// Project command request.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WireProjectCommand {
    ReadOverlay {
        request: WireOverlayReadRequest,
    },
    MutateOverlay {
        request: WireOverlayMutationRequest,
    },
    CommitOverlay {
        request: WireOverlayCommitRequest,
    },
    ReadInventory {
        request: WireProjectInventoryReadRequest,
    },
    CreateNode {
        request: WireCreateNodeRequest,
    },
    RemoveNode {
        request: WireRemoveNodeRequest,
    },
    /// Runtime command channel: dispatch `command` to the live runtime of
    /// the node with runtime id `node` (the id project reads/tree deltas
    /// carry). Non-overlay, non-persistent — see
    /// [`crate::WireNodeCommand`].
    NodeCommand {
        node: NodeId,
        command: WireNodeCommand,
    },
    /// Engage/update a panel writer at `(scope, channel)` — runtime state,
    /// no overlay, no dirty (see [`crate::WirePanelWriteRequest`]).
    PanelWrite {
        request: crate::WirePanelWriteRequest,
    },
    /// Clear engaged panel writers (see [`crate::WirePanelClearRequest`]).
    PanelClear {
        request: crate::WirePanelClearRequest,
    },
}

/// Project command response.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WireProjectCommandResponse {
    ReadOverlay {
        response: WireOverlayReadResponse,
    },
    MutateOverlay {
        response: WireOverlayMutationResponse,
    },
    CommitOverlay {
        response: WireOverlayCommitResponse,
    },
    ReadInventory {
        response: WireProjectInventoryReadResponse,
    },
    CreateNode {
        response: WireCreateNodeResponse,
    },
    RemoveNode {
        response: WireRemoveNodeResponse,
    },
    NodeCommand {
        response: WireNodeCommandResponse,
    },
    PanelWrite {
        response: crate::WirePanelCommandResponse,
    },
    PanelClear {
        response: crate::WirePanelCommandResponse,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;
    use lpc_model::{MutationCmdBatch, ProjectInventory, ProjectOverlay, Revision};

    #[test]
    fn project_command_round_trips() {
        let request = WireProjectCommand::MutateOverlay {
            request: WireOverlayMutationRequest::new(MutationCmdBatch::new(Vec::new())),
        };

        let json = serde_json::to_string(&request).unwrap();
        let decoded: WireProjectCommand = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, request);
        assert!(json.contains("mutate_overlay"));
    }

    #[test]
    fn node_command_request_round_trips() {
        let request = WireProjectCommand::NodeCommand {
            node: lpc_model::NodeId::new(7),
            command: WireNodeCommand::PlaylistActivateEntry { entry: 2 },
        };

        let json = serde_json::to_string(&request).unwrap();
        let decoded: WireProjectCommand = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, request);
        assert!(json.contains("node_command"));
        assert!(json.contains("playlist_activate_entry"));
    }

    #[test]
    fn node_command_response_round_trips() {
        let response = WireProjectCommandResponse::NodeCommand {
            response: WireNodeCommandResponse::Accepted,
        };

        let json = serde_json::to_string(&response).unwrap();
        let decoded: WireProjectCommandResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, response);
        assert!(json.contains("node_command"));
    }

    #[test]
    fn project_command_response_round_trips() {
        let response = WireProjectCommandResponse::ReadInventory {
            response: WireProjectInventoryReadResponse::from_inventory(&ProjectInventory::new()),
        };

        let json = serde_json::to_string(&response).unwrap();
        let decoded: WireProjectCommandResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, response);
        assert!(json.contains("read_inventory"));

        let overlay = WireProjectCommandResponse::ReadOverlay {
            response: WireOverlayReadResponse::new(ProjectOverlay::new(), Revision::default()),
        };
        let json = serde_json::to_string(&overlay).unwrap();
        let decoded: WireProjectCommandResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, overlay);
    }

    #[test]
    fn create_node_command_round_trips() {
        use lpc_model::{LpPathBuf, NodeAttachSite};

        let request = WireProjectCommand::CreateNode {
            request: crate::WireCreateNodeRequest::new(
                LpPathBuf::from("./clock-2.json"),
                b"{\n  \"kind\": \"Clock\"\n}\n".to_vec(),
                Vec::new(),
                NodeAttachSite::ProjectNodes {
                    key: "clock-2".into(),
                },
            ),
        };

        let json = serde_json::to_string(&request).unwrap();
        let decoded: WireProjectCommand = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, request);
        assert!(json.contains("create_node"));
    }

    #[test]
    fn remove_node_command_round_trips() {
        use lpc_model::NodeAttachSite;

        let request = WireProjectCommand::RemoveNode {
            request: crate::WireRemoveNodeRequest::new(NodeAttachSite::ProjectNodes {
                key: "clock-2".into(),
            }),
        };

        let json = serde_json::to_string(&request).unwrap();
        let decoded: WireProjectCommand = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, request);
        assert!(json.contains("remove_node"));
    }
}
