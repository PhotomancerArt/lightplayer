//! Overlay apply and discard lifecycle.

mod common;

use common::fixtures;
use lpc_model::{LpValue, Revision, SlotPath, SlotShapeRegistry};
use lpc_node_registry::{
    ArtifactEdit, AssetEdit, EditBatch, EditBatchId, EditError, EditTarget, NodeDefEntry,
    NodeDefLoc, NodeDefRegistry, ParseCtx, SlotEdit,
};
use lpfs::{LpFsMemory, LpPath, LpPathBuf};

fn parse_ctx() -> SlotShapeRegistry {
    SlotShapeRegistry::default()
}

fn apply_artifact_edit(
    registry: &mut NodeDefRegistry,
    fs: &LpFsMemory,
    change: &ArtifactEdit,
) -> Result<(), EditError> {
    let shapes = parse_ctx();
    let ctx = ParseCtx { shapes: &shapes };
    registry.apply_artifact_edit(change, fs, &ctx, Revision::new(1))
}

fn snapshot_registry(registry: &NodeDefRegistry, root: &NodeDefLoc) -> NodeDefEntry {
    registry.get(root).expect("root entry").clone()
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
    let before = snapshot_registry(&registry, &root);

    apply_artifact_edit(
        &mut registry,
        &fs,
        &ArtifactEdit::asset(
            EditTarget::Path(LpPathBuf::from("/pending.glsl")),
            vec![AssetEdit::ReplaceBody("void main() {}".into())],
        ),
    )
    .unwrap();

    assert!(registry.slot_overlay_active());
    assert!(registry.slot_overlay_contains_path(LpPath::new("/pending.glsl")));
    assert_eq!(
        registry.slot_overlay_bytes(LpPath::new("/pending.glsl")),
        Some(b"void main() {}" as &[u8])
    );
    assert_eq!(snapshot_registry(&registry, &root), before);
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
    let before = snapshot_registry(&registry, &root);

    apply_artifact_edit(
        &mut registry,
        &fs,
        &ArtifactEdit::asset(
            EditTarget::Path(LpPathBuf::from("/pending.glsl")),
            vec![AssetEdit::ReplaceBody("pending".into())],
        ),
    )
    .unwrap();
    assert!(registry.slot_overlay_active());

    registry.discard_slot_overlay();

    assert!(!registry.slot_overlay_active());
    assert!(!registry.slot_overlay_contains_path(LpPath::new("/pending.glsl")));
    assert_eq!(snapshot_registry(&registry, &root), before);
}

#[test]
fn apply_rejects_relative_path() {
    let fs = LpFsMemory::new();
    let mut registry = NodeDefRegistry::new();
    let err = apply_artifact_edit(
        &mut registry,
        &fs,
        &ArtifactEdit::asset(
            EditTarget::Path(LpPathBuf::from("relative.glsl")),
            vec![AssetEdit::ReplaceBody("x".into())],
        ),
    )
    .unwrap_err();
    assert!(matches!(err, EditError::InvalidPath { .. }));
    assert!(!registry.slot_overlay_active());
}

#[test]
fn apply_replace_body_on_unloaded_path_implicit_create() {
    let fs = LpFsMemory::new();
    let mut registry = NodeDefRegistry::new();
    apply_artifact_edit(
        &mut registry,
        &fs,
        &ArtifactEdit::asset(
            EditTarget::Path(LpPathBuf::from("/new.shader.glsl")),
            vec![AssetEdit::ReplaceBody("body".into())],
        ),
    )
    .unwrap();
    assert!(registry.slot_overlay_contains_path(LpPath::new("/new.shader.glsl")));
}

#[test]
fn apply_edit_batch_batches_changes() {
    let fs = LpFsMemory::new();
    let mut registry = NodeDefRegistry::new();
    let shapes = parse_ctx();
    let ctx = ParseCtx { shapes: &shapes };
    registry
        .apply_edit_batch(
            &EditBatch::new(
                EditBatchId(1),
                vec![
                    ArtifactEdit::asset(
                        EditTarget::Path(LpPathBuf::from("/a.glsl")),
                        vec![AssetEdit::ReplaceBody("a".into())],
                    ),
                    ArtifactEdit::asset(
                        EditTarget::Path(LpPathBuf::from("/b.glsl")),
                        vec![AssetEdit::ReplaceBody("b".into())],
                    ),
                ],
            ),
            &fs,
            &ctx,
            Revision::new(1),
        )
        .unwrap();
    assert!(registry.slot_overlay_contains_path(LpPath::new("/a.glsl")));
    assert!(registry.slot_overlay_contains_path(LpPath::new("/b.glsl")));
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

    apply_artifact_edit(
        &mut registry,
        &fs,
        &ArtifactEdit::asset(
            EditTarget::Path(LpPathBuf::from("/shader.glsl")),
            vec![AssetEdit::Delete],
        ),
    )
    .unwrap();

    assert!(registry.slot_overlay_contains_path(LpPath::new("/shader.glsl")));
    assert_eq!(
        registry.slot_overlay_bytes(LpPath::new("/shader.glsl")),
        None
    );
}

#[test]
fn apply_slot_op_on_non_toml_path_errors() {
    let fs = LpFsMemory::new();
    let mut registry = NodeDefRegistry::new();
    let err = apply_artifact_edit(
        &mut registry,
        &fs,
        &ArtifactEdit::slot(
            EditTarget::Path(LpPathBuf::from("/shader.glsl")),
            vec![SlotEdit::AssignValue {
                path: SlotPath::root(),
                value: LpValue::F32(1.0),
            }],
        ),
    )
    .unwrap_err();
    assert!(matches!(err, EditError::InvalidPath { .. }));
    assert!(!registry.slot_overlay_active());
}
