//! Overlay apply/discard lifecycle (D1, D3).

mod common;

use common::fixtures;
use lpc_model::{LpValue, Revision, SlotPath, SlotShapeRegistry};
use lpc_node_registry::{
    ArtifactChange, ArtifactOp, ArtifactTarget, ChangeError, ChangeSet, ChangeSetId, NodeDefEntry,
    NodeDefId, NodeDefRegistry, ParseCtx,
};
use lpfs::{LpFsMemory, LpPath, LpPathBuf};

fn parse_ctx() -> SlotShapeRegistry {
    SlotShapeRegistry::default()
}

fn apply_change(
    registry: &mut NodeDefRegistry,
    fs: &LpFsMemory,
    change: &ArtifactChange,
) -> Result<(), ChangeError> {
    let shapes = parse_ctx();
    let ctx = ParseCtx { shapes: &shapes };
    registry.apply_change(change, fs, &ctx, Revision::new(1))
}

fn snapshot_registry(registry: &NodeDefRegistry, root: NodeDefId) -> NodeDefEntry {
    registry.get(&root).expect("root entry").clone()
}

#[test]
fn d1_apply_populates_overlay_base_unchanged() {
    let fs = fixtures::load_clock();
    let mut registry = NodeDefRegistry::new();
    let shapes = parse_ctx();
    let ctx = ParseCtx { shapes: &shapes };
    let root = registry
        .load_root(&fs, LpPath::new("/clock.toml"), Revision::new(1), &ctx)
        .unwrap();
    let before = snapshot_registry(&registry, root);

    apply_change(
        &mut registry,
        &fs,
        &ArtifactChange {
            target: ArtifactTarget::Path(LpPathBuf::from("/pending.glsl")),
            ops: vec![ArtifactOp::SetBytes("void main() {}".into())],
        },
    )
    .unwrap();

    assert!(registry.overlay_active());
    assert!(registry.overlay_contains_path(LpPath::new("/pending.glsl")));
    assert_eq!(
        registry.overlay_bytes(LpPath::new("/pending.glsl")),
        Some(b"void main() {}" as &[u8])
    );
    assert_eq!(snapshot_registry(&registry, root), before);
}

#[test]
fn d3_discard_clears_overlay_entries_unchanged() {
    let fs = fixtures::load_clock();
    let mut registry = NodeDefRegistry::new();
    let shapes = parse_ctx();
    let ctx = ParseCtx { shapes: &shapes };
    let root = registry
        .load_root(&fs, LpPath::new("/clock.toml"), Revision::new(1), &ctx)
        .unwrap();
    let before = snapshot_registry(&registry, root);

    apply_change(
        &mut registry,
        &fs,
        &ArtifactChange {
            target: ArtifactTarget::Path(LpPathBuf::from("/pending.glsl")),
            ops: vec![ArtifactOp::SetBytes("pending".into())],
        },
    )
    .unwrap();
    assert!(registry.overlay_active());

    registry.discard_overlay();

    assert!(!registry.overlay_active());
    assert!(!registry.overlay_contains_path(LpPath::new("/pending.glsl")));
    assert_eq!(snapshot_registry(&registry, root), before);
}

#[test]
fn apply_rejects_relative_path() {
    let fs = LpFsMemory::new();
    let mut registry = NodeDefRegistry::new();
    let err = apply_change(
        &mut registry,
        &fs,
        &ArtifactChange {
            target: ArtifactTarget::Path(LpPathBuf::from("relative.glsl")),
            ops: vec![ArtifactOp::SetBytes("x".into())],
        },
    )
    .unwrap_err();
    assert!(matches!(err, ChangeError::InvalidPath { .. }));
    assert!(!registry.overlay_active());
}

#[test]
fn apply_setbytes_on_unloaded_path_implicit_create() {
    let fs = LpFsMemory::new();
    let mut registry = NodeDefRegistry::new();
    apply_change(
        &mut registry,
        &fs,
        &ArtifactChange {
            target: ArtifactTarget::Path(LpPathBuf::from("/new.shader.glsl")),
            ops: vec![ArtifactOp::SetBytes("body".into())],
        },
    )
    .unwrap();
    assert!(registry.overlay_contains_path(LpPath::new("/new.shader.glsl")));
}

#[test]
fn apply_changeset_batches_changes() {
    let fs = LpFsMemory::new();
    let mut registry = NodeDefRegistry::new();
    let shapes = parse_ctx();
    let ctx = ParseCtx { shapes: &shapes };
    registry
        .apply_changeset(
            &ChangeSet::new(
                ChangeSetId(1),
                vec![
                    ArtifactChange {
                        target: ArtifactTarget::Path(LpPathBuf::from("/a.glsl")),
                        ops: vec![ArtifactOp::SetBytes("a".into())],
                    },
                    ArtifactChange {
                        target: ArtifactTarget::Path(LpPathBuf::from("/b.glsl")),
                        ops: vec![ArtifactOp::SetBytes("b".into())],
                    },
                ],
            ),
            &fs,
            &ctx,
            Revision::new(1),
        )
        .unwrap();
    assert!(registry.overlay_contains_path(LpPath::new("/a.glsl")));
    assert!(registry.overlay_contains_path(LpPath::new("/b.glsl")));
}

#[test]
fn apply_delete_marks_overlay_entry() {
    let fs = fixtures::load_shader_project();
    let mut registry = NodeDefRegistry::new();
    let shapes = parse_ctx();
    let ctx = ParseCtx { shapes: &shapes };
    registry
        .load_root(&fs, LpPath::new("/shader.toml"), Revision::new(1), &ctx)
        .unwrap();

    apply_change(
        &mut registry,
        &fs,
        &ArtifactChange {
            target: ArtifactTarget::Path(LpPathBuf::from("/shader.glsl")),
            ops: vec![ArtifactOp::Delete],
        },
    )
    .unwrap();

    assert!(registry.overlay_contains_path(LpPath::new("/shader.glsl")));
    assert_eq!(registry.overlay_bytes(LpPath::new("/shader.glsl")), None);
}

#[test]
fn apply_slot_op_on_non_toml_path_errors() {
    let fs = LpFsMemory::new();
    let mut registry = NodeDefRegistry::new();
    let err = apply_change(
        &mut registry,
        &fs,
        &ArtifactChange {
            target: ArtifactTarget::Path(LpPathBuf::from("/shader.glsl")),
            ops: vec![ArtifactOp::SetSlot {
                path: SlotPath::root(),
                value: LpValue::String("Shader".into()),
            }],
        },
    )
    .unwrap_err();
    assert!(matches!(err, ChangeError::InvalidPath { .. }));
    assert!(!registry.overlay_active());
}
