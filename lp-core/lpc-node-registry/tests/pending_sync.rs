//! Pending edit map and unified sync integration tests.

mod common;

use common::fixtures;
use lpc_model::{LpValue, Revision, SlotPath, SlotShapeRegistry};
use lpc_node_registry::{AssetEdit, NodeDefRegistry, ParseCtx, SlotEdit, SyncOp};
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
            &[SyncOp::SetPendingAsset {
                path: LpPathBuf::from("/a.glsl"),
                asset: AssetEdit::ReplaceBody(b"a".to_vec()),
            }],
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
    let path = LpPathBuf::from("/a.glsl");

    registry
        .sync(
            &fs,
            &[SyncOp::SetPendingAsset {
                path: path.clone(),
                asset: AssetEdit::ReplaceBody(b"a".to_vec()),
            }],
            Revision::new(1),
            &ctx,
        )
        .unwrap();

    let outcome = registry
        .sync(&fs, &[SyncOp::Remove { path }], Revision::new(1), &ctx)
        .unwrap();

    assert!(outcome.pending_changed);
    assert!(!registry.overlay_active());
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

    let outcome = registry
        .sync(
            &fs,
            &[
                SyncOp::UpsertSlot {
                    path: LpPathBuf::from("/clock.toml"),
                    op: SlotEdit::AssignValue {
                        path: SlotPath::parse("controls.rate").unwrap(),
                        value: LpValue::F32(2.0),
                    },
                },
                SyncOp::Commit,
            ],
            Revision::new(2),
            &ctx,
        )
        .unwrap();

    assert!(!outcome.committed.def_updates.changed.is_empty());
    assert!(!registry.overlay_active());
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
                SyncOp::UpsertSlot {
                    path: LpPathBuf::from("/shader.toml"),
                    op: SlotEdit::EnsurePresent {
                        path: SlotPath::parse("Shader").unwrap(),
                    },
                },
                SyncOp::Commit,
            ],
            Revision::new(2),
            &ctx,
        )
        .unwrap();

    assert!(outcome.pending_changed);
    assert!(!registry.overlay_active());
    assert!(outcome.committed.def_updates.is_empty());
    assert_eq!(
        registry.artifact_revision_for_path(LpPath::new("/shader.glsl")),
        Some(Revision::new(2))
    );
}
