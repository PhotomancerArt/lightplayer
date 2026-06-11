//! Project overlay mutation envelopes.

use lpc_model::{OverlayMutationBatch, OverlayMutationBatchResult};

/// Wire request for an ordered overlay mutation batch.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WireOverlayMutationRequest {
    pub batch: OverlayMutationBatch,
}

impl WireOverlayMutationRequest {
    pub fn new(batch: OverlayMutationBatch) -> Self {
        Self { batch }
    }
}

/// Wire response for an ordered overlay mutation batch.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WireOverlayMutationResponse {
    pub result: OverlayMutationBatchResult,
}

impl WireOverlayMutationResponse {
    pub fn new(result: OverlayMutationBatchResult) -> Self {
        Self { result }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use lpc_model::{
        ArtifactBodyEdit, ArtifactLocation, OverlayMutation, OverlayMutationCommand,
        OverlayMutationCommandId, OverlayMutationCommandResult, OverlayMutationEffect, SlotEdit,
        SlotPath,
    };

    #[test]
    fn overlay_mutation_request_round_trips() {
        let request = WireOverlayMutationRequest::new(OverlayMutationBatch::new(vec![
            OverlayMutationCommand {
                id: OverlayMutationCommandId::new(1),
                mutation: OverlayMutation::PutSlotEdit {
                    artifact: ArtifactLocation::file("/project.toml"),
                    edit: SlotEdit::ensure_present(SlotPath::parse("nodes[clock]").unwrap()),
                },
            },
            OverlayMutationCommand {
                id: OverlayMutationCommandId::new(2),
                mutation: OverlayMutation::SetArtifactBody {
                    artifact: ArtifactLocation::file("/shader.glsl"),
                    edit: ArtifactBodyEdit::ReplaceBody(b"void main() {}".to_vec()),
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
        let response = WireOverlayMutationResponse::new(OverlayMutationBatchResult::new(vec![
            OverlayMutationCommandResult::accepted(
                OverlayMutationCommandId::new(1),
                OverlayMutationEffect::OverlayChanged { changed: true },
            ),
        ]));

        let json = serde_json::to_string(&response).unwrap();
        let decoded: WireOverlayMutationResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, response);
        assert!(json.contains("overlay_changed"));
    }
}
