//! Project commands that are not runtime project reads.

use crate::{
    WireOverlayCommitRequest, WireOverlayCommitResponse, WireOverlayMutationRequest,
    WireOverlayMutationResponse, WireOverlayReadRequest, WireOverlayReadResponse,
    WireProjectInventoryReadRequest, WireProjectInventoryReadResponse,
};

/// Project command request.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case", tag = "command")]
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
}

/// Project command response.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case", tag = "command")]
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;
    use lpc_model::{MutationCmdBatch, ProjectInventory, ProjectOverlay};

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
    fn project_command_response_round_trips() {
        let response = WireProjectCommandResponse::ReadInventory {
            response: WireProjectInventoryReadResponse::from_inventory(&ProjectInventory::new()),
        };

        let json = serde_json::to_string(&response).unwrap();
        let decoded: WireProjectCommandResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, response);
        assert!(json.contains("read_inventory"));

        let overlay = WireProjectCommandResponse::ReadOverlay {
            response: WireOverlayReadResponse::new(ProjectOverlay::new()),
        };
        let json = serde_json::to_string(&overlay).unwrap();
        let decoded: WireProjectCommandResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, overlay);
    }
}
