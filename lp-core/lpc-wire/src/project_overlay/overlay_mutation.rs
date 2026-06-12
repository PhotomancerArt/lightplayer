//! Project overlay mutation envelopes.

use lpc_model::{MutationCmdBatch, MutationCmdBatchResult};

/// Wire request for an ordered overlay mutation batch.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WireOverlayMutationRequest {
    pub batch: MutationCmdBatch,
}

impl WireOverlayMutationRequest {
    pub fn new(batch: MutationCmdBatch) -> Self {
        Self { batch }
    }
}

/// Wire response for an ordered overlay mutation batch.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WireOverlayMutationResponse {
    pub result: MutationCmdBatchResult,
}

impl WireOverlayMutationResponse {
    pub fn new(result: MutationCmdBatchResult) -> Self {
        Self { result }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use lpc_model::{
        ArtifactLocation, AssetBodyOverlay, MutationCmd, MutationCmdId, MutationCmdResult,
        MutationEffect, MutationOp, SlotEdit, SlotPath,
    };

    #[test]
    fn overlay_mutation_request_round_trips() {
        let request = WireOverlayMutationRequest::new(MutationCmdBatch::new(vec![
            MutationCmd {
                id: MutationCmdId::new(1),
                mutation: MutationOp::PutSlotEdit {
                    artifact: ArtifactLocation::file("/project.toml"),
                    edit: SlotEdit::ensure_present(SlotPath::parse("nodes[clock]").unwrap()),
                },
            },
            MutationCmd {
                id: MutationCmdId::new(2),
                mutation: MutationOp::SetArtifactBody {
                    artifact: ArtifactLocation::file("/shader.glsl"),
                    edit: AssetBodyOverlay::ReplaceBody(b"void main() {}".to_vec()),
                },
            },
        ]));

        let json = serde_json::to_string(&request).unwrap();
        let decoded: WireOverlayMutationRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, request);
        assert!(json.contains("put_slot_edit"));
        assert!(json.contains("set_artifact_body"));
    }

    #[test]
    fn overlay_mutation_response_round_trips() {
        let response = WireOverlayMutationResponse::new(MutationCmdBatchResult::new(vec![
            MutationCmdResult::accepted(
                MutationCmdId::new(1),
                MutationEffect::OverlayChanged { changed: true },
            ),
        ]));

        let json = serde_json::to_string(&response).unwrap();
        let decoded: WireOverlayMutationResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, response);
        assert!(json.contains("overlay_changed"));
    }
}
