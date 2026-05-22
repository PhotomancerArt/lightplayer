//! Asset overlay reads through materialize.

mod common;

use common::fixtures;
use lpc_model::{Revision, SlotShapeRegistry, SourceFileSlot};
use lpc_node_registry::{
    ArtifactEdit, ArtifactError, EditOp, ArtifactReadFailure, EditTarget,
    MaterializeError, NodeDefEntry, NodeDefId, NodeDefRegistry, ParseCtx, SourceDiagnosticCtx,
};
use lpfs::{LpPath, LpPathBuf};

fn parse_ctx() -> SlotShapeRegistry {
    SlotShapeRegistry::default()
}

fn diag_ctx() -> SourceDiagnosticCtx {
    SourceDiagnosticCtx {
        containing_file: String::from("/shader.toml"),
        slot_path: None,
    }
}

fn load_shader_root(registry: &mut NodeDefRegistry, fs: &dyn lpfs::LpFs) -> NodeDefId {
    let shapes = parse_ctx();
    let ctx = ParseCtx { shapes: &shapes };
    registry
        .load_root(fs, LpPath::new("/shader.toml"), Revision::new(1), &ctx)
        .unwrap()
}

fn snapshot_entry(registry: &NodeDefRegistry, id: NodeDefId) -> NodeDefEntry {
    registry.get(&id).expect("entry").clone()
}

fn apply_artifact_edit(registry: &mut NodeDefRegistry, fs: &dyn lpfs::LpFs, change: &ArtifactEdit) {
    let shapes = parse_ctx();
    let ctx = ParseCtx { shapes: &shapes };
    registry
        .apply_artifact_edit(change, fs, &ctx, Revision::new(1))
        .unwrap();
}

#[test]
fn c4c_replace_glsl_via_overlay_def_unchanged() {
    let fs = fixtures::load_shader_project();
    let mut registry = NodeDefRegistry::new();
    let root = load_shader_root(&mut registry, &fs);
    let before = snapshot_entry(&registry, root);
    let slot = SourceFileSlot::from_path("./shader.glsl");

    apply_artifact_edit(
        &mut registry,
        &fs,
        &ArtifactEdit {
            target: EditTarget::Path(LpPathBuf::from("/shader.glsl")),
            ops: vec![EditOp::SetBytes(
                "void main() { gl_FragColor = vec4(0.0, 1.0, 0.0, 1.0); }".into(),
            )],
        },
    );

    let effective = registry
        .materialize_source(
            &fs,
            LpPath::new("/shader.toml"),
            &slot,
            &diag_ctx(),
            Revision::new(1),
        )
        .unwrap();
    assert!(effective.text.contains("0.0, 1.0, 0.0"));
    assert_eq!(snapshot_entry(&registry, root), before);
}

#[test]
fn c4a_add_asset_via_overlay_implicit_create() {
    let fs = fixtures::load_shader_project();
    let mut registry = NodeDefRegistry::new();
    load_shader_root(&mut registry, &fs);

    apply_artifact_edit(
        &mut registry,
        &fs,
        &ArtifactEdit {
            target: EditTarget::Path(LpPathBuf::from("/extra.glsl")),
            ops: vec![EditOp::SetBytes("void main() {}".into())],
        },
    );

    let slot = SourceFileSlot::from_path("./extra.glsl");
    let materialized = registry
        .materialize_source(
            &fs,
            LpPath::new("/shader.toml"),
            &slot,
            &diag_ctx(),
            Revision::new(1),
        )
        .unwrap();
    assert_eq!(materialized.text, "void main() {}");
}

#[test]
fn c4b_delete_asset_via_overlay() {
    let fs = fixtures::load_shader_project();
    let mut registry = NodeDefRegistry::new();
    load_shader_root(&mut registry, &fs);
    let slot = SourceFileSlot::from_path("./shader.glsl");

    apply_artifact_edit(
        &mut registry,
        &fs,
        &ArtifactEdit {
            target: EditTarget::Path(LpPathBuf::from("/shader.glsl")),
            ops: vec![EditOp::Delete],
        },
    );

    let err = registry
        .materialize_source(
            &fs,
            LpPath::new("/shader.toml"),
            &slot,
            &diag_ctx(),
            Revision::new(1),
        )
        .unwrap_err();
    assert_eq!(
        err,
        MaterializeError::Artifact(ArtifactError::Read(ArtifactReadFailure::Deleted))
    );
}

#[test]
fn c4d_replace_asset_without_touching_def_toml() {
    let fs = fixtures::load_shader_project();
    let mut registry = NodeDefRegistry::new();
    let root = load_shader_root(&mut registry, &fs);
    let before = snapshot_entry(&registry, root);
    let slot = SourceFileSlot::from_path("./shader.glsl");
    let slot_revision = slot.revision();

    apply_artifact_edit(
        &mut registry,
        &fs,
        &ArtifactEdit {
            target: EditTarget::Path(LpPathBuf::from("/shader.glsl")),
            ops: vec![EditOp::SetBytes("void main() { /* draft */ }".into())],
        },
    );

    assert!(!registry.slot_overlay_contains_path(LpPath::new("/shader.toml")));
    let effective = registry
        .materialize_source(
            &fs,
            LpPath::new("/shader.toml"),
            &slot,
            &diag_ctx(),
            Revision::new(1),
        )
        .unwrap();
    assert!(effective.text.contains("draft"));
    assert_eq!(effective.version, slot_revision);
    assert_eq!(snapshot_entry(&registry, root), before);
}
