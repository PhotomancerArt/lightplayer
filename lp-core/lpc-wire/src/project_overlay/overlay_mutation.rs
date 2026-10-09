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

    /// The PLAYFUL choker's shader (1,971 B), the edit Yona's board refused
    /// on 2026-10-08.
    const CHOKER_SHADER: &str =
        include_str!("../../../../catalog/projects/playful-choker/shader.glsl");
    /// `projects/test/basic`'s shader (4,365 B).
    const BASIC_SHADER: &str = include_str!("../../../../projects/test/basic/shader.glsl");

    /// Studio's edit of a known shader is the shader's size plus its
    /// envelope and a byte a line (wire 41), not ~3.5 characters a byte as
    /// the array of numbers it was. Pinned, so a change to the encoding or
    /// the envelope shows up here first.
    #[test]
    fn a_shader_edit_request_is_about_the_size_of_its_source() {
        for (name, shader, before, after) in [
            ("playful-choker", CHOKER_SHADER, 7_134, 2_239),
            ("test/basic", BASIC_SHADER, 15_183, 4_709),
        ] {
            let source = shader.len();
            let text = shader_edit_request(shader.as_bytes()).len();
            let array = shader_edit_request_as_byte_array(shader.as_bytes()).len();
            assert_eq!((array, text), (before, after), "{name}: {source} B source");
            // The envelope (~200 B) and the newlines' escapes are all it adds.
            let lines = shader.matches('\n').count();
            assert!(
                text <= source + lines + 300,
                "{name}: {text} B for {source} B of source"
            );
            assert!(text * 3 < array, "{name}: {text} B against {array} B");
        }
    }

    /// The body Studio stops at fits one request on the board, with a long
    /// path, and comes back in an overlay read inside one frame.
    #[test]
    fn the_largest_body_studio_sends_fits_one_request_and_one_reply() {
        use crate::budget::{MAX_ASSET_BODY_ENCODED_BYTES, PROJECT_READ_FRAME_MAX_BYTES};
        // Two characters to an escaped newline: the encoded body is exactly
        // the limit.
        let line = "vec3 c = vec3(0.5);\n";
        let mut body = line.repeat(MAX_ASSET_BODY_ENCODED_BYTES / (line.len() + 1));
        while lpc_model::body_bytes::encoded_len(body.as_bytes()) < MAX_ASSET_BODY_ENCODED_BYTES {
            body.push('x');
        }
        assert_eq!(
            lpc_model::body_bytes::encoded_len(body.as_bytes()),
            MAX_ASSET_BODY_ENCODED_BYTES
        );
        let path = alloc::format!("/{}.glsl", "deep/".repeat(150));
        let request = crate::json::to_string(&crate::ClientMessage {
            id: u64::from(u32::MAX),
            msg: crate::ClientRequest::ProjectCommand {
                handle: crate::WireProjectHandle::new(u32::MAX),
                command: crate::WireProjectCommand::MutateOverlay {
                    request: edit_of(&path, body.as_bytes()),
                },
            },
        })
        .unwrap();
        assert!(
            request.len() <= PROJECT_READ_FRAME_MAX_BYTES,
            "{} B request",
            request.len()
        );

        let mut overlay = lpc_model::ProjectOverlay::new();
        overlay.set_artifact_body(
            ArtifactLocation::file(path.as_str()),
            AssetBodyOverlay::ReplaceBody(body.into_bytes()),
        );
        let reply = crate::json::to_string(&crate::WireServerMessage::new(
            u64::from(u32::MAX),
            crate::server::ServerMsgBody::ProjectCommand {
                response: crate::WireProjectCommandResponse::ReadOverlay {
                    response: crate::WireOverlayReadResponse::new(
                        overlay,
                        Revision::new(i64::from(u32::MAX)),
                    ),
                },
            },
        ))
        .unwrap();
        assert!(
            reply.len() <= PROJECT_READ_FRAME_MAX_BYTES,
            "{} B reply",
            reply.len()
        );
    }

    #[test]
    fn a_body_round_trips_as_text_and_as_base64() {
        for body in [
            CHOKER_SHADER.as_bytes().to_vec(),
            b"abcd".to_vec(),
            alloc::vec![0x00, 0xff, 0x80, b'"', b'\\'],
        ] {
            let request = edit_of("/shader.glsl", &body);
            let json = crate::json::to_string(&request).unwrap();
            let decoded: WireOverlayMutationRequest = crate::json::from_str(&json).unwrap();
            assert_eq!(decoded, request, "{json}");
        }
        let json = crate::json::to_string(&edit_of("/a.bin", &[0xff, 0xfe])).unwrap();
        assert!(
            json.contains(r#""edit":{"replace_body":{"base64":"//4="}}"#),
            "{json}"
        );
        let json = crate::json::to_string(&edit_of("/a.glsl", b"void main() {}\n")).unwrap();
        assert!(
            json.contains(r#""edit":{"replace_body":"void main() {}\n"}"#),
            "{json}"
        );
    }

    /// The board writes bodies back (an overlay read) with `ser-write-json`:
    /// the same text as `serde_json`, for text and for a binary body.
    #[cfg(feature = "ser-write-json")]
    #[test]
    fn the_device_serializer_writes_bodies_as_serde_json_does() {
        for body in [CHOKER_SHADER.as_bytes(), &[0x00, 0xff, b'"', 0x80][..]] {
            let mut overlay = lpc_model::ProjectOverlay::new();
            overlay.set_artifact_body(
                ArtifactLocation::file("/shader.glsl"),
                AssetBodyOverlay::ReplaceBody(body.to_vec()),
            );
            let read = crate::WireOverlayReadResponse::new(overlay, Revision::new(3));
            let mut device = alloc::vec::Vec::new();
            ser_write_json::ser::to_writer(&mut device, &read).unwrap();
            assert_eq!(
                core::str::from_utf8(&device).unwrap(),
                crate::json::to_string(&read).unwrap()
            );
        }
    }

    #[test]
    fn overlay_mutation_response_round_trips() {
        let response = WireOverlayMutationResponse::new(
            MutationCmdBatchResult::new(vec![MutationCmdResult::accepted(
                MutationCmdId::new(1),
                MutationEffect::overlay_changed(true),
            )]),
            Revision::new(11),
        );

        let json = serde_json::to_string(&response).unwrap();
        let decoded: WireOverlayMutationResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, response);
        assert_eq!(decoded.overlay_revision, Revision::new(11));
        assert!(json.contains("overlay_changed"));
        assert!(json.contains("overlay_revision"));
        assert!(
            !json.contains("base_display"),
            "unannotated effects add nothing to the wire: {json}"
        );
    }

    #[test]
    fn base_display_annotation_round_trips_and_stays_optional() {
        // The base-value annotation is skip-if-none on every effect surface:
        // annotated effects round-trip it, unannotated effects (a firmware
        // server that derived nothing) keep the wire form unchanged.
        let response = WireOverlayMutationResponse::new(
            MutationCmdBatchResult::new(vec![
                MutationCmdResult::accepted(
                    MutationCmdId::new(1),
                    MutationEffect::overlay_changed(true).with_base_display(Some("1.0".into())),
                ),
                MutationCmdResult::accepted(
                    MutationCmdId::new(2),
                    MutationEffect::overlay_changed(true),
                ),
            ]),
            Revision::new(11),
        );

        let json = serde_json::to_string(&response).unwrap();
        let decoded: WireOverlayMutationResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, response);
        assert_eq!(
            json.matches("base_display").count(),
            1,
            "only the annotated command serializes the field: {json}"
        );
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
                    MutationEffect::normalized_to_removal(true)
                        .with_base_display(Some("1.0".into())),
                ),
                MutationCmdResult::accepted(
                    MutationCmdId::new(2),
                    MutationEffect::normalized_to_removal(false),
                ),
            ]),
            Revision::new(12),
        );

        let json = serde_json::to_string(&response).unwrap();
        let decoded: WireOverlayMutationResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, response);
        assert!(json.contains("normalized_to_removal"));
        assert_eq!(json.matches("base_display").count(), 1, "{json}");
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
                        StoredSlotEdit::put(SlotEdit::ensure_present(
                            SlotPath::parse("paths[1]").unwrap(),
                        )),
                        StoredSlotEdit::put(SlotEdit::assign_value(
                            SlotPath::parse("paths[1].PointList.first_channel").unwrap(),
                            LpValue::U32(5),
                        )),
                        StoredSlotEdit::put_with_base_display(
                            SlotEdit::remove(SlotPath::parse("paths[2]").unwrap()),
                            Some("{\"kind\":\"PointList\"}".into()),
                        ),
                        StoredSlotEdit::removed(SlotPath::parse("paths[0]").unwrap()),
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
        assert_eq!(
            json.matches("base_display").count(),
            1,
            "per-edit annotations are skip-if-none: {json}"
        );
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

    /// A one-command `SetArtifactBody` batch replacing `path`'s body.
    fn edit_of(path: &str, body: &[u8]) -> WireOverlayMutationRequest {
        WireOverlayMutationRequest::new(MutationCmdBatch::new(vec![MutationCmd {
            id: MutationCmdId::new(7),
            mutation: MutationOp::SetArtifactBody {
                artifact: ArtifactLocation::file(path),
                edit: AssetBodyOverlay::ReplaceBody(body.to_vec()),
            },
        }]))
    }

    /// Studio's edit of `body`, the whole client message as the board
    /// receives it.
    fn shader_edit_request(body: &[u8]) -> alloc::string::String {
        crate::json::to_string(&crate::ClientMessage {
            id: 31,
            msg: crate::ClientRequest::ProjectCommand {
                handle: crate::WireProjectHandle::new(1),
                command: crate::WireProjectCommand::MutateOverlay {
                    request: edit_of("/shader.glsl", body),
                },
            },
        })
        .unwrap()
    }

    /// [`shader_edit_request`] with the body as the array of numbers it was
    /// before wire 41, for the comparison.
    fn shader_edit_request_as_byte_array(body: &[u8]) -> alloc::string::String {
        let text = shader_edit_request(body);
        let as_text = alloc::format!(
            r#""replace_body":{}"#,
            serde_json::to_string(core::str::from_utf8(body).unwrap()).unwrap()
        );
        let as_array = alloc::format!(
            r#""replace_body":{}"#,
            serde_json::to_string(&body.to_vec()).unwrap()
        );
        assert!(text.contains(&as_text));
        text.replacen(&as_text, &as_array, 1)
    }
}
