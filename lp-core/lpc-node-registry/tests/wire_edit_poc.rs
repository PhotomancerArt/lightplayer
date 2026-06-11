//! Wire-shaped project edit POC against the node registry.

use lpc_model::{
    ArtifactBodyEdit, ArtifactEdit, DefinitionLocation, LpValue, ProjectEditBatch,
    ProjectEditCommand, ProjectEditCommandId, ProjectEditCommandStatus, ProjectEditEffect,
    ProjectEditOp, Revision, SlotPath, SlotShapeRegistry, SourceFileSlot,
};
use lpc_node_registry::{NodeDefLoc, NodeDefRegistry, ParseCtx, SourceDiagnosticCtx};
use lpc_wire::{WireProjectEditRequest, WireProjectEditResponse};
use lpfs::{LpFs, LpFsMemory, LpPath, LpPathBuf};

#[test]
fn project_edit_batch_builds_graph_from_loaded_root_and_commits() {
    let fs = minimal_project_fs();
    let shapes = SlotShapeRegistry::default();
    let ctx = ParseCtx { shapes: &shapes };
    let mut registry = NodeDefRegistry::new();
    let root = registry
        .load_root(&fs, LpPath::new("/project.toml"), Revision::new(1), &ctx)
        .expect("load root");
    assert_eq!(root, loc("/project.toml", ""));

    let request = round_trip_request(WireProjectEditRequest::new(ProjectEditBatch::new(vec![
        slot_command(
            1,
            "/project.toml",
            lpc_model::SlotEdit::EnsurePresent {
                path: SlotPath::parse("nodes[shader].ref").unwrap(),
            },
        ),
        slot_command(
            2,
            "/project.toml",
            lpc_model::SlotEdit::AssignValue {
                path: SlotPath::parse("nodes[shader].ref").unwrap(),
                value: LpValue::String(String::from("./shader.toml")),
            },
        ),
        slot_command(
            3,
            "/project.toml",
            lpc_model::SlotEdit::EnsurePresent {
                path: SlotPath::parse("nodes[clock].def.Clock").unwrap(),
            },
        ),
        slot_command(
            4,
            "/project.toml",
            lpc_model::SlotEdit::AssignValue {
                path: SlotPath::parse("nodes[clock].def.controls.rate").unwrap(),
                value: LpValue::F32(2.0),
            },
        ),
        slot_command(
            5,
            "/shader.toml",
            lpc_model::SlotEdit::EnsurePresent {
                path: SlotPath::parse("Shader").unwrap(),
            },
        ),
        slot_command(
            6,
            "/shader.toml",
            lpc_model::SlotEdit::AssignValue {
                path: SlotPath::parse("source.path").unwrap(),
                value: LpValue::String(String::from("./shader.glsl")),
            },
        ),
        body_command(
            7,
            "/shader.glsl",
            ArtifactBodyEdit::ReplaceBody(b"void main() { /* created */ }".to_vec()),
        ),
        body_command(
            8,
            "/scratch.glsl",
            ArtifactBodyEdit::ReplaceBody(b"scratch".to_vec()),
        ),
        body_command(9, "/scratch.glsl", ArtifactBodyEdit::Delete),
        command(10, ProjectEditOp::Commit),
    ])));

    let result = registry.apply_project_edit_batch(&fs, &request.batch, Revision::new(2), &ctx);
    assert_all_accepted(&result.results);
    let response = round_trip_response(WireProjectEditResponse::new(result));
    let summary = committed_summary(&response, 10);
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
            Revision::new(3),
        )
        .expect("materialized source");
    assert!(source.text.contains("created"));
    assert!(!fs.file_exists(LpPath::new("/scratch.glsl")).unwrap());

    let mut reloaded = NodeDefRegistry::new();
    reloaded
        .load_root(&fs, LpPath::new("/project.toml"), Revision::new(4), &ctx)
        .expect("reload root");
    assert!(
        reloaded
            .get(&loc("/project.toml", "nodes[clock]"))
            .is_some()
    );
    assert!(reloaded.get(&loc("/shader.toml", "")).is_some());

    let second = WireProjectEditRequest::new(ProjectEditBatch::new(vec![
        body_command(
            11,
            "/shader.glsl",
            ArtifactBodyEdit::ReplaceBody(b"void main() { /* replaced */ }".to_vec()),
        ),
        slot_command(
            12,
            "/project.toml",
            lpc_model::SlotEdit::Remove {
                path: SlotPath::parse("nodes[shader]").unwrap(),
            },
        ),
        body_command(13, "/shader.glsl", ArtifactBodyEdit::Delete),
        command(14, ProjectEditOp::Commit),
    ]));
    let result = registry.apply_project_edit_batch(&fs, &second.batch, Revision::new(5), &ctx);
    assert_all_accepted(&result.results);
    let response = WireProjectEditResponse::new(result);
    let summary = committed_summary(&response, 14);
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
        .load_root(&fs, LpPath::new("/project.toml"), Revision::new(6), &ctx)
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
fn project_edit_batch_rejects_relative_artifact_path() {
    let fs = minimal_project_fs();
    let shapes = SlotShapeRegistry::default();
    let ctx = ParseCtx { shapes: &shapes };
    let mut registry = NodeDefRegistry::new();
    registry
        .load_root(&fs, LpPath::new("/project.toml"), Revision::new(1), &ctx)
        .expect("load root");

    let batch = ProjectEditBatch::new(vec![body_command(
        1,
        "relative.glsl",
        ArtifactBodyEdit::ReplaceBody(b"x".to_vec()),
    )]);
    let result = registry.apply_project_edit_batch(&fs, &batch, Revision::new(2), &ctx);

    assert!(matches!(
        &result.results[0].status,
        ProjectEditCommandStatus::Rejected { rejection }
            if rejection.reason == lpc_model::ProjectEditRejectionReason::InvalidPath
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

fn command(id: u64, op: ProjectEditOp) -> ProjectEditCommand {
    ProjectEditCommand {
        id: ProjectEditCommandId::new(id),
        op,
    }
}

fn slot_command(id: u64, artifact_path: &str, edit: lpc_model::SlotEdit) -> ProjectEditCommand {
    command(
        id,
        ProjectEditOp::ApplyArtifactEdit {
            edit: ArtifactEdit::slot(LpPathBuf::from(artifact_path), edit),
        },
    )
}

fn body_command(id: u64, artifact_path: &str, edit: ArtifactBodyEdit) -> ProjectEditCommand {
    command(
        id,
        ProjectEditOp::ApplyArtifactEdit {
            edit: ArtifactEdit::body(LpPathBuf::from(artifact_path), edit),
        },
    )
}

fn assert_all_accepted(results: &[lpc_model::ProjectEditCommandResult]) {
    assert!(
        results
            .iter()
            .all(|result| matches!(result.status, ProjectEditCommandStatus::Accepted { .. })),
        "expected all project edit commands to be accepted: {results:?}"
    );
}

fn committed_summary(
    response: &WireProjectEditResponse,
    command_id: u64,
) -> &lpc_model::ProjectCommitSummary {
    response
        .result
        .results
        .iter()
        .find_map(|result| {
            if result.id != ProjectEditCommandId::new(command_id) {
                return None;
            }
            match &result.status {
                ProjectEditCommandStatus::Accepted {
                    effect: ProjectEditEffect::Committed { summary },
                } => Some(summary),
                _ => None,
            }
        })
        .expect("commit summary")
}

fn round_trip_request(request: WireProjectEditRequest) -> WireProjectEditRequest {
    let json = serde_json::to_string(&request).unwrap();
    serde_json::from_str(&json).unwrap()
}

fn round_trip_response(response: WireProjectEditResponse) -> WireProjectEditResponse {
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
    String::from_utf8(fs.read_file(LpPath::new(path)).unwrap()).unwrap()
}
