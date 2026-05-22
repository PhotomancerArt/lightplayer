//! Pending edit map and unified sync integration tests.

mod common;

use common::fixtures;
use lpc_model::{LpValue, Revision, SlotPath, SlotShapeRegistry};
use lpc_node_registry::{
    ArtifactEdit, EditBatch, EditBatchId, EditOp, EditTarget, NodeDefRegistry, ParseCtx, SyncOp,
};
use lpfs::{FsEvent, FsEventKind, LpFsMemory, LpPath, LpPathBuf};

fn parse_ctx() -> SlotShapeRegistry {
    SlotShapeRegistry::default()
}

fn fs_modify(path: &str) -> FsEvent {
    FsEvent {
        path: LpPathBuf::from(path),
        kind: FsEventKind::Modify,
    }
}

#[test]
fn sync_apply_updates_overlay() {
    let fs = LpFsMemory::new();
    let mut registry = NodeDefRegistry::new();
    let shapes = parse_ctx();
    let ctx = ParseCtx { shapes: &shapes };

    let outcome = registry
        .sync(
            &fs,
            &[SyncOp::Apply(ArtifactEdit {
                target: EditTarget::Path(LpPathBuf::from("/a.glsl")),
                ops: vec![EditOp::SetBytes("a".into())],
            })],
            Revision::new(1),
            &ctx,
        )
        .unwrap();

    assert!(outcome.pending_changed);
    assert!(registry.slot_overlay_contains_path(LpPath::new("/a.glsl")));
}

#[test]
fn sync_remove_drops_one_pending_artifact() {
    let fs = LpFsMemory::new();
    let mut registry = NodeDefRegistry::new();
    let shapes = parse_ctx();
    let ctx = ParseCtx { shapes: &shapes };
    let target = EditTarget::Path(LpPathBuf::from("/a.glsl"));

    registry
        .sync(
            &fs,
            &[SyncOp::Apply(ArtifactEdit {
                target: target.clone(),
                ops: vec![EditOp::SetBytes("a".into())],
            })],
            Revision::new(1),
            &ctx,
        )
        .unwrap();

    let outcome = registry
        .sync(&fs, &[SyncOp::Remove(target)], Revision::new(1), &ctx)
        .unwrap();

    assert!(outcome.pending_changed);
    assert!(!registry.slot_overlay_active());
}

#[test]
fn sync_apply_then_commit_clears_overlay() {
    let fs = fixtures::load_clock();
    let mut registry = NodeDefRegistry::new();
    let shapes = parse_ctx();
    let ctx = ParseCtx { shapes: &shapes };
    registry
        .load_root(&fs, LpPath::new("/clock.toml"), Revision::new(1), &ctx)
        .unwrap();

    let batch = EditBatch::new(
        EditBatchId(1),
        vec![ArtifactEdit {
            target: EditTarget::Path(LpPathBuf::from("/clock.toml")),
            ops: vec![EditOp::SetSlot {
                path: SlotPath::parse("controls.rate").unwrap(),
                value: LpValue::F32(2.0),
            }],
        }],
    );

    let outcome = registry
        .sync(
            &fs,
            &[SyncOp::Apply(batch.edits[0].clone()), SyncOp::Commit],
            Revision::new(2),
            &ctx,
        )
        .unwrap();

    assert!(!outcome.committed.def_updates.changed.is_empty());
    assert!(!registry.slot_overlay_active());
}

#[test]
fn sync_fs_and_commit_in_one_batch() {
    let mut fs = fixtures::load_shader_project();
    let mut registry = NodeDefRegistry::new();
    let shapes = parse_ctx();
    let ctx = ParseCtx { shapes: &shapes };
    registry
        .load_root(&fs, LpPath::new("/shader.toml"), Revision::new(1), &ctx)
        .unwrap();

    fixtures::write_file(
        &mut fs,
        "/shader.glsl",
        "void main() { gl_FragColor = vec4(1.0); }",
    );

    let outcome = registry
        .sync(
            &fs,
            &[
                SyncOp::Fs(fs_modify("/shader.glsl")),
                SyncOp::Apply(ArtifactEdit {
                    target: EditTarget::Path(LpPathBuf::from("/shader.toml")),
                    ops: vec![EditOp::VariantSet {
                        path: SlotPath::root(),
                        variant: "Shader".into(),
                    }],
                }),
                SyncOp::Commit,
            ],
            Revision::new(2),
            &ctx,
        )
        .unwrap();

    assert!(
        !outcome.committed.source_revisions.is_empty() || !outcome.committed.def_updates.is_empty()
    );
}
