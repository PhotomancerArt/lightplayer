//! Asset overlay reads — C4a–d spot tests (M3).

mod common;

use common::fixtures;
use lpc_model::{Revision, SlotShapeRegistry, SourceFileSlot};
use lpc_node_registry::{
    ArtifactChange, ArtifactError, ArtifactOp, ArtifactReadFailure, ArtifactTarget,
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

#[test]
fn c4c_replace_glsl_via_overlay_def_unchanged() {
    let fs = fixtures::load_shader_project();
    let mut registry = NodeDefRegistry::new();
    let root = load_shader_root(&mut registry, &fs);
    let before = snapshot_entry(&registry, root);
    let slot = SourceFileSlot::from_path("./shader.glsl");

    registry
        .apply_change(&ArtifactChange {
            target: ArtifactTarget::Path(LpPathBuf::from("/shader.glsl")),
            ops: vec![ArtifactOp::SetBytes(
                "void main() { gl_FragColor = vec4(0.0, 1.0, 0.0, 1.0); }".into(),
            )],
        })
        .unwrap();

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

    registry
        .apply_change(&ArtifactChange {
            target: ArtifactTarget::Path(LpPathBuf::from("/extra.glsl")),
            ops: vec![ArtifactOp::SetBytes("void main() {}".into())],
        })
        .unwrap();

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

    registry
        .apply_change(&ArtifactChange {
            target: ArtifactTarget::Path(LpPathBuf::from("/shader.glsl")),
            ops: vec![ArtifactOp::Delete],
        })
        .unwrap();

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

    registry
        .apply_change(&ArtifactChange {
            target: ArtifactTarget::Path(LpPathBuf::from("/shader.glsl")),
            ops: vec![ArtifactOp::SetBytes("void main() { /* draft */ }".into())],
        })
        .unwrap();

    assert!(!registry.overlay_contains_path(LpPath::new("/shader.toml")));
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
