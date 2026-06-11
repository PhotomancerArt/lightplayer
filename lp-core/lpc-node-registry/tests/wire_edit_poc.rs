//! Wire-shaped project overlay POC against the node registry.

use lpc_model::{
    ArtifactBodyEdit, ArtifactOverlay, DefinitionLocation, LpValue, OverlayMutation,
    OverlayMutationBatch, OverlayMutationCommand, OverlayMutationCommandId,
    OverlayMutationCommandStatus, OverlayMutationEffect, OverlayMutationRejectionReason, Revision,
    SlotEdit, SlotEditOp, SlotPath, SlotShapeRegistry, SourceFileSlot,
};
use lpc_node_registry::{NodeDefLoc, NodeDefRegistry, ParseCtx, SourceDiagnosticCtx};
use lpc_wire::{
    WireOverlayCommitRequest, WireOverlayCommitResponse, WireOverlayMutationRequest,
    WireOverlayMutationResponse, WireOverlayReadRequest, WireOverlayReadResponse,
};
use lpfs::{LpFs, LpFsMemory, LpPath, LpPathBuf};

#[test]
fn overlay_api_builds_graph_from_loaded_root_and_commits() {
    let fs = minimal_project_fs();
    let shapes = SlotShapeRegistry::default();
    let ctx = ParseCtx { shapes: &shapes };
    let mut registry = NodeDefRegistry::new();
    let root = registry
        .load_root(&fs, LpPath::new("/project.toml"), Revision::new(1), &ctx)
        .expect("load root");
    assert_eq!(root, loc("/project.toml", ""));

    let _: WireOverlayReadRequest =
        serde_json::from_str(&serde_json::to_string(&WireOverlayReadRequest).unwrap()).unwrap();
    let empty_overlay =
        round_trip_read_response(WireOverlayReadResponse::new(registry.overlay().clone()));
    assert!(empty_overlay.overlay.is_empty());

    let request = round_trip_mutation_request(WireOverlayMutationRequest::new(
        OverlayMutationBatch::new(vec![
            put_slot(
                1,
                "/project.toml",
                SlotEdit::ensure_present(SlotPath::parse("nodes[shader].ref").unwrap()),
            ),
            put_slot(
                2,
                "/project.toml",
                SlotEdit::assign_value(
                    SlotPath::parse("nodes[shader].ref").unwrap(),
                    LpValue::String(String::from("./shader.toml")),
                ),
            ),
            put_slot(
                3,
                "/project.toml",
                SlotEdit::ensure_present(SlotPath::parse("nodes[clock].def.Clock").unwrap()),
            ),
            put_slot(
                4,
                "/project.toml",
                SlotEdit::assign_value(
                    SlotPath::parse("nodes[clock].def.controls.rate").unwrap(),
                    LpValue::F32(2.0),
                ),
            ),
            put_slot(
                5,
                "/shader.toml",
                SlotEdit::ensure_present(SlotPath::parse("Shader").unwrap()),
            ),
            put_slot(
                6,
                "/shader.toml",
                SlotEdit::assign_value(
                    SlotPath::parse("source.path").unwrap(),
                    LpValue::String(String::from("./shader.glsl")),
                ),
            ),
            set_body(
                7,
                "/shader.glsl",
                ArtifactBodyEdit::ReplaceBody(b"void main() { /* created */ }".to_vec()),
            ),
            set_body(
                8,
                "/scratch.glsl",
                ArtifactBodyEdit::ReplaceBody(b"scratch".to_vec()),
            ),
            set_body(9, "/scratch.glsl", ArtifactBodyEdit::Delete),
        ]),
    ));

    let result = registry.apply_overlay_mutation_batch(&fs, &request.batch, Revision::new(2), &ctx);
    assert_all_mutations_accepted(&result.results);
    let response = round_trip_mutation_response(WireOverlayMutationResponse::new(result));
    assert_all_mutations_accepted(&response.result.results);

    let pending =
        round_trip_read_response(WireOverlayReadResponse::new(registry.overlay().clone()));
    assert_project_overlay_was_coalesced(&pending);

    let _: WireOverlayCommitRequest =
        serde_json::from_str(&serde_json::to_string(&WireOverlayCommitRequest).unwrap()).unwrap();
    let summary = registry
        .commit_overlay(&fs, Revision::new(3), &ctx)
        .expect("commit");
    let response = round_trip_commit_response(WireOverlayCommitResponse::new(summary));
    let summary = &response.summary;
    assert!(
        summary
            .def_updates
            .changed
            .contains(&definition_loc("/project.toml", SlotPath::root()))
    );
    assert!(summary.def_updates.added.contains(&definition_loc(
        "/project.toml",
        SlotPath::parse("nodes[clock]").unwrap()
    )));
    assert!(
        summary
            .def_updates
            .added
            .contains(&definition_loc("/shader.toml", SlotPath::root()))
    );
    assert!(!registry.overlay_active());

    let source = registry
        .materialize_source(
            &fs,
            LpPath::new("/shader.toml"),
            &SourceFileSlot::from_path("./shader.glsl"),
            &source_diag_ctx("/shader.toml"),
            Revision::new(4),
        )
        .expect("materialized source");
    assert!(source.text.contains("created"));
    assert!(!fs.file_exists(LpPath::new("/scratch.glsl")).unwrap());

    let mut reloaded = NodeDefRegistry::new();
    reloaded
        .load_root(&fs, LpPath::new("/project.toml"), Revision::new(5), &ctx)
        .expect("reload root");
    assert!(
        reloaded
            .get(&loc("/project.toml", "nodes[clock]"))
            .is_some()
    );
    assert!(reloaded.get(&loc("/shader.toml", "")).is_some());

    let second = WireOverlayMutationRequest::new(OverlayMutationBatch::new(vec![
        set_body(
            10,
            "/shader.glsl",
            ArtifactBodyEdit::ReplaceBody(b"void main() { /* replaced */ }".to_vec()),
        ),
        put_slot(
            11,
            "/project.toml",
            SlotEdit::remove(SlotPath::parse("nodes[shader]").unwrap()),
        ),
        set_body(12, "/shader.glsl", ArtifactBodyEdit::Delete),
    ]));
    let result = registry.apply_overlay_mutation_batch(&fs, &second.batch, Revision::new(6), &ctx);
    assert_all_mutations_accepted(&result.results);
    let summary = registry
        .commit_overlay(&fs, Revision::new(7), &ctx)
        .expect("second commit");
    assert!(
        summary
            .def_updates
            .removed
            .contains(&definition_loc("/shader.toml", SlotPath::root())),
        "expected shader def removal in summary: {summary:?}"
    );
    assert!(!fs.file_exists(LpPath::new("/shader.glsl")).unwrap());

    let mut final_reload = NodeDefRegistry::new();
    final_reload
        .load_root(&fs, LpPath::new("/project.toml"), Revision::new(8), &ctx)
        .expect("final reload");
    assert!(
        final_reload
            .get(&loc("/project.toml", "nodes[clock]"))
            .is_some()
    );
    assert!(final_reload.get(&loc("/shader.toml", "")).is_none());

    let project_text = read_text(&fs, "/project.toml");
    assert!(project_text.contains("[nodes.clock.def]"));
    assert!(!project_text.contains("[nodes.shader]"));
}

#[test]
fn overlay_mutation_rejects_relative_artifact_path() {
    let fs = minimal_project_fs();
    let shapes = SlotShapeRegistry::default();
    let ctx = ParseCtx { shapes: &shapes };
    let mut registry = NodeDefRegistry::new();
    registry
        .load_root(&fs, LpPath::new("/project.toml"), Revision::new(1), &ctx)
        .expect("load root");

    let batch = OverlayMutationBatch::new(vec![set_body(
        1,
        "relative.glsl",
        ArtifactBodyEdit::ReplaceBody(b"x".to_vec()),
    )]);
    let result = registry.apply_overlay_mutation_batch(&fs, &batch, Revision::new(2), &ctx);

    assert!(matches!(
        &result.results[0].status,
        OverlayMutationCommandStatus::Rejected { rejection }
            if rejection.reason == OverlayMutationRejectionReason::InvalidPath
    ));
}

fn minimal_project_fs() -> LpFsMemory {
    let mut fs = LpFsMemory::new();
    fs.write_file_mut(
        LpPath::new("/project.toml"),
        br#"
kind = "Project"
"#,
    )
    .unwrap();
    fs
}

fn put_slot(id: u64, artifact_path: &str, edit: SlotEdit) -> OverlayMutationCommand {
    command(
        id,
        OverlayMutation::PutSlotEdit {
            artifact_path: LpPathBuf::from(artifact_path),
            edit,
        },
    )
}

fn set_body(id: u64, artifact_path: &str, edit: ArtifactBodyEdit) -> OverlayMutationCommand {
    command(
        id,
        OverlayMutation::SetArtifactBody {
            artifact_path: LpPathBuf::from(artifact_path),
            edit,
        },
    )
}

fn command(id: u64, mutation: OverlayMutation) -> OverlayMutationCommand {
    OverlayMutationCommand {
        id: OverlayMutationCommandId::new(id),
        mutation,
    }
}

fn assert_all_mutations_accepted(results: &[lpc_model::OverlayMutationCommandResult]) {
    assert!(
        results.iter().all(|result| matches!(
            result.status,
            OverlayMutationCommandStatus::Accepted {
                effect: OverlayMutationEffect::OverlayChanged { .. }
            }
        )),
        "expected all overlay mutations to be accepted: {results:?}"
    );
}

fn assert_project_overlay_was_coalesced(response: &WireOverlayReadResponse) {
    let project = response
        .overlay
        .artifact(&LpPathBuf::from("/project.toml"))
        .expect("project overlay");
    let ArtifactOverlay::Slot { overlay } = project else {
        panic!("expected project slot overlay");
    };
    assert_eq!(
        overlay
            .edits
            .get(&SlotPath::parse("nodes[shader].ref").unwrap()),
        Some(&SlotEditOp::AssignValue(LpValue::String(String::from(
            "./shader.toml"
        ))))
    );

    let scratch = response
        .overlay
        .artifact(&LpPathBuf::from("/scratch.glsl"))
        .expect("scratch overlay");
    assert!(matches!(
        scratch,
        ArtifactOverlay::Body {
            edit: ArtifactBodyEdit::Delete
        }
    ));
}

fn round_trip_read_response(response: WireOverlayReadResponse) -> WireOverlayReadResponse {
    let json = serde_json::to_string(&response).unwrap();
    serde_json::from_str(&json).unwrap()
}

fn round_trip_mutation_request(request: WireOverlayMutationRequest) -> WireOverlayMutationRequest {
    let json = serde_json::to_string(&request).unwrap();
    serde_json::from_str(&json).unwrap()
}

fn round_trip_mutation_response(
    response: WireOverlayMutationResponse,
) -> WireOverlayMutationResponse {
    let json = serde_json::to_string(&response).unwrap();
    serde_json::from_str(&json).unwrap()
}

fn round_trip_commit_response(response: WireOverlayCommitResponse) -> WireOverlayCommitResponse {
    let json = serde_json::to_string(&response).unwrap();
    serde_json::from_str(&json).unwrap()
}

fn source_diag_ctx(containing_file: &str) -> SourceDiagnosticCtx {
    SourceDiagnosticCtx {
        containing_file: String::from(containing_file),
        slot_path: None,
    }
}

fn definition_loc(path: &str, slot_path: SlotPath) -> DefinitionLocation {
    DefinitionLocation::new(LpPathBuf::from(path), slot_path)
}

fn loc(path: &str, slot_path: &str) -> NodeDefLoc {
    NodeDefLoc {
        artifact: lpc_node_registry::ArtifactLoc::file(path),
        path: if slot_path.is_empty() {
            SlotPath::root()
        } else {
            SlotPath::parse(slot_path).unwrap()
        },
    }
}

fn read_text(fs: &dyn LpFs, path: &str) -> String {
    let bytes = fs.read_file(LpPath::new(path)).unwrap();
    String::from_utf8(bytes).unwrap()
}
