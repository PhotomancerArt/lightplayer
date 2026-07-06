//! Project overlay mutation envelopes.

use lpc_model::{MutationCmdBatch, MutationCmdBatchResult, Revision};

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
    /// Revision at which the overlay last changed, after applying the batch.
    pub overlay_revision: Revision,
}

impl WireOverlayMutationResponse {
    pub fn new(result: MutationCmdBatchResult, overlay_revision: Revision) -> Self {
        Self {
            result,
            overlay_revision,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use lpc_model::{
        ArtifactLocation, AssetBodyOverlay, LpValue, MutationCmd, MutationCmdId, MutationCmdResult,
        MutationEffect, MutationOp, MutationRejection, MutationRejectionReason, SlotEdit, SlotPath,
        StoredSlotEdit,
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
        let response = WireOverlayMutationResponse::new(
            MutationCmdBatchResult::new(vec![MutationCmdResult::accepted(
                MutationCmdId::new(1),
                MutationEffect::OverlayChanged { changed: true },
            )]),
            Revision::new(11),
        );

        let json = serde_json::to_string(&response).unwrap();
        let decoded: WireOverlayMutationResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, response);
        assert_eq!(decoded.overlay_revision, Revision::new(11));
        assert!(json.contains("overlay_changed"));
        assert!(json.contains("overlay_revision"));
    }

    #[test]
    fn normalized_to_removal_effect_round_trips() {
        // Minimal-diff normalization rides the per-command effect: clients
        // mirror the stored removal from the ack, so the variant must survive
        // the wire distinctly from `overlay_changed`.
        let response = WireOverlayMutationResponse::new(
            MutationCmdBatchResult::new(vec![
                MutationCmdResult::accepted(
                    MutationCmdId::new(1),
                    MutationEffect::NormalizedToRemoval { changed: true },
                ),
                MutationCmdResult::accepted(
                    MutationCmdId::new(2),
                    MutationEffect::NormalizedToRemoval { changed: false },
                ),
            ]),
            Revision::new(12),
        );

        let json = serde_json::to_string(&response).unwrap();
        let decoded: WireOverlayMutationResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, response);
        assert!(json.contains("normalized_to_removal"));
    }

    #[test]
    fn move_slot_entry_request_round_trips() {
        // Map keys are path segments, so the move endpoints ride the wire as
        // canonical slot-path strings like every other edit path.
        let request = WireOverlayMutationRequest::new(MutationCmdBatch::new(vec![MutationCmd {
            id: MutationCmdId::new(1),
            mutation: MutationOp::MoveSlotEntry {
                artifact: ArtifactLocation::file("/fixture.json"),
                from: SlotPath::parse("mapping.PathPoints.paths[0]").unwrap(),
                to: SlotPath::parse("mapping.PathPoints.paths[1]").unwrap(),
            },
        }]));

        let json = serde_json::to_string(&request).unwrap();
        let decoded: WireOverlayMutationRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, request);
        assert!(json.contains("move_slot_entry"));
        assert!(json.contains("mapping.PathPoints.paths[0]"));
    }

    #[test]
    fn materialized_effect_round_trips() {
        // A move's ack lists the stored per-path edits so ack-mirroring
        // clients can replay them without a follow-up fetch; both stored and
        // removed forms must survive the wire distinctly.
        let response = WireOverlayMutationResponse::new(
            MutationCmdBatchResult::new(vec![MutationCmdResult::accepted(
                MutationCmdId::new(1),
                MutationEffect::Materialized {
                    edits: vec![
                        StoredSlotEdit::Put {
                            edit: SlotEdit::ensure_present(SlotPath::parse("paths[1]").unwrap()),
                        },
                        StoredSlotEdit::Put {
                            edit: SlotEdit::assign_value(
                                SlotPath::parse("paths[1].PointList.first_channel").unwrap(),
                                LpValue::U32(5),
                            ),
                        },
                        StoredSlotEdit::Removed {
                            path: SlotPath::parse("paths[0]").unwrap(),
                        },
                    ],
                    changed: true,
                },
            )]),
            Revision::new(14),
        );

        let json = serde_json::to_string(&response).unwrap();
        let decoded: WireOverlayMutationResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, response);
        assert!(json.contains("materialized"));
        assert!(json.contains("put"));
        assert!(json.contains("removed"));
    }

    #[test]
    fn target_occupied_rejection_round_trips() {
        // Occupied-target moves reject with a dedicated reason so the key
        // editor can surface "key already in use" on the row.
        let response = WireOverlayMutationResponse::new(
            MutationCmdBatchResult::new(vec![MutationCmdResult::rejected(
                MutationCmdId::new(1),
                MutationRejection::new(
                    MutationRejectionReason::TargetOccupied,
                    "map entry paths[1] already exists in the effective definition".into(),
                ),
            )]),
            Revision::new(15),
        );

        let json = serde_json::to_string(&response).unwrap();
        let decoded: WireOverlayMutationResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, response);
        assert!(json.contains("target_occupied"));
    }

    #[test]
    fn not_a_value_leaf_rejection_round_trips() {
        // Structural `AssignValue` targets reject with a reason distinct from
        // `type_mismatch` (M3 plan, D6); the variant must survive the wire so
        // clients can tell "wrong value" from "wrong kind of target".
        let response = WireOverlayMutationResponse::new(
            MutationCmdBatchResult::new(vec![MutationCmdResult::rejected(
                MutationCmdId::new(1),
                MutationRejection::new(
                    MutationRejectionReason::NotAValueLeaf,
                    "slot mapping is a structural slot, not a value leaf".into(),
                ),
            )]),
            Revision::new(13),
        );

        let json = serde_json::to_string(&response).unwrap();
        let decoded: WireOverlayMutationResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, response);
        assert!(json.contains("not_a_value_leaf"));
    }
}
