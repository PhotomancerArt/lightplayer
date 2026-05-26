//! Commit promotion: overlay flush, filesystem write, and `SyncResult`.

mod common;

use common::fixtures;
use lpc_model::{LpValue, NodeDef, Revision, SlotPath, SlotShapeRegistry};
use lpc_node_registry::{
    ArtifactEdit, AssetEdit, NodeDefLoc, EditTarget, NodeDefEntry, NodeDefId, NodeDefRegistry,
    NodeDefState, ParseCtx, SlotEdit,
};
use lpfs::{FsEvent, FsEventKind, LpFs, LpPath, LpPathBuf};

fn parse_ctx() -> SlotShapeRegistry {
    SlotShapeRegistry::default()
}

fn apply_artifact_edit(registry: &mut NodeDefRegistry, fs: &dyn LpFs, change: &ArtifactEdit) {
    let shapes = parse_ctx();
    let ctx = ParseCtx { shapes: &shapes };
    registry
        .apply_artifact_edit(change, fs, &ctx, Revision::new(2))
        .unwrap();
}

fn clock_rate(entry: &NodeDefEntry) -> f32 {
    let NodeDefState::Loaded(NodeDef::Clock(def)) = &entry.state else {
        panic!("expected loaded clock def");
    };
    *def.controls.rate.value()
}

fn shader_render_order(entry: &NodeDefEntry) -> i32 {
    let NodeDefState::Loaded(NodeDef::Shader(def)) = &entry.state else {
        panic!("expected loaded shader def");
    };
    def.render_order()
}

fn inline_child_id(registry: &NodeDefRegistry, root: NodeDefId) -> NodeDefId {
    let artifact_id = registry.get(&root).unwrap().loc.artifact_id;
    registry
        .get_by_source(&NodeDefLoc {
            artifact_id,
            path: SlotPath::parse("entries[2].node").unwrap(),
        })
        .expect("inline child")
        .id
}

fn fs_modify(path: &str) -> FsEvent {
    FsEvent {
        path: LpPathBuf::from(path),
        kind: FsEventKind::Modify,
    }
}

#[test]
fn d2_commit_updates_committed_and_clears_overlay() {
    let fs = fixtures::load_clock();
    let mut registry = NodeDefRegistry::new();
    let shapes = parse_ctx();
    let ctx = ParseCtx { shapes: &shapes };
    let root = registry
        .load_root(&fs, LpPath::new("/clock.toml"), Revision::new(1), &ctx)
        .unwrap();

    apply_artifact_edit(
        &mut registry,
        &fs,
        &ArtifactEdit::slot(
            EditTarget::Path(LpPathBuf::from("/clock.toml")),
            vec![SlotEdit::AssignValue {
                path: SlotPath::parse("controls.rate").unwrap(),
                value: LpValue::F32(2.0),
            }],
        ),
    );

    assert!(registry.slot_overlay_active());
    assert_eq!(
        clock_rate(&registry.view().get(&root, &fs, &ctx).unwrap()),
        2.0
    );
    assert_eq!(clock_rate(registry.get(&root).unwrap()), 1.0);

    registry.commit(&fs, Revision::new(3), &ctx).unwrap();

    assert!(!registry.slot_overlay_active());
    assert_eq!(clock_rate(registry.get(&root).unwrap()), 2.0);
    assert_eq!(
        clock_rate(&registry.view().get(&root, &fs, &ctx).unwrap()),
        2.0
    );
}

#[test]
fn d2_commit_setbytes_updates_committed() {
    let fs = fixtures::load_clock();
    let mut registry = NodeDefRegistry::new();
    let shapes = parse_ctx();
    let ctx = ParseCtx { shapes: &shapes };
    let root = registry
        .load_root(&fs, LpPath::new("/clock.toml"), Revision::new(1), &ctx)
        .unwrap();

    apply_artifact_edit(
        &mut registry,
        &fs,
        &ArtifactEdit::asset(
            EditTarget::Path(LpPathBuf::from("/clock.toml")),
            vec![AssetEdit::ReplaceBody(
                r#"
kind = "Clock"

[controls]
rate = 3.0
"#
                .into(),
            )],
        ),
    );

    registry.commit(&fs, Revision::new(3), &ctx).unwrap();
    assert_eq!(clock_rate(registry.get(&root).unwrap()), 3.0);
}

#[test]
fn d2_commit_writes_slot_draft_to_fs() {
    let fs = fixtures::load_clock();
    let mut registry = NodeDefRegistry::new();
    let shapes = parse_ctx();
    let ctx = ParseCtx { shapes: &shapes };
    registry
        .load_root(&fs, LpPath::new("/clock.toml"), Revision::new(1), &ctx)
        .unwrap();

    apply_artifact_edit(
        &mut registry,
        &fs,
        &ArtifactEdit::slot(
            EditTarget::Path(LpPathBuf::from("/clock.toml")),
            vec![SlotEdit::AssignValue {
                path: SlotPath::parse("controls.rate").unwrap(),
                value: LpValue::F32(2.0),
            }],
        ),
    );

    registry.commit(&fs, Revision::new(3), &ctx).unwrap();

    let bytes = fs.read_file(LpPath::new("/clock.toml")).unwrap();
    let text = core::str::from_utf8(&bytes).unwrap();
    assert!(text.contains("rate = 2"));
}

#[test]
fn d5_overlay_wins_over_stale_fs() {
    let mut fs = fixtures::load_clock();
    let mut registry = NodeDefRegistry::new();
    let shapes = parse_ctx();
    let ctx = ParseCtx { shapes: &shapes };
    let root = registry
        .load_root(&fs, LpPath::new("/clock.toml"), Revision::new(1), &ctx)
        .unwrap();

    apply_artifact_edit(
        &mut registry,
        &fs,
        &ArtifactEdit::slot(
            EditTarget::Path(LpPathBuf::from("/clock.toml")),
            vec![SlotEdit::AssignValue {
                path: SlotPath::parse("controls.rate").unwrap(),
                value: LpValue::F32(2.0),
            }],
        ),
    );

    fixtures::write_file(
        &mut fs,
        "/clock.toml",
        r#"
kind = "Clock"

[controls]
rate = 9.0
"#,
    );

    assert_eq!(
        clock_rate(&registry.view().get(&root, &fs, &ctx).unwrap()),
        2.0
    );
    assert_eq!(clock_rate(registry.get(&root).unwrap()), 1.0);
}

#[test]
fn d5_sync_fs_does_not_clobber_overlay_view() {
    let mut fs = fixtures::load_clock();
    let mut registry = NodeDefRegistry::new();
    let shapes = parse_ctx();
    let ctx = ParseCtx { shapes: &shapes };
    let root = registry
        .load_root(&fs, LpPath::new("/clock.toml"), Revision::new(1), &ctx)
        .unwrap();

    apply_artifact_edit(
        &mut registry,
        &fs,
        &ArtifactEdit::slot(
            EditTarget::Path(LpPathBuf::from("/clock.toml")),
            vec![SlotEdit::AssignValue {
                path: SlotPath::parse("controls.rate").unwrap(),
                value: LpValue::F32(2.0),
            }],
        ),
    );

    fixtures::write_file(
        &mut fs,
        "/clock.toml",
        r#"
kind = "Clock"

[controls]
rate = 9.0
"#,
    );
    registry.sync_fs(&fs, &[fs_modify("/clock.toml")], Revision::new(4), &ctx);

    assert_eq!(
        clock_rate(&registry.view().get(&root, &fs, &ctx).unwrap()),
        2.0
    );
}

#[test]
fn d5_post_commit_fs_sync_updates_committed() {
    let mut fs = fixtures::load_clock();
    let mut registry = NodeDefRegistry::new();
    let shapes = parse_ctx();
    let ctx = ParseCtx { shapes: &shapes };
    let root = registry
        .load_root(&fs, LpPath::new("/clock.toml"), Revision::new(1), &ctx)
        .unwrap();

    apply_artifact_edit(
        &mut registry,
        &fs,
        &ArtifactEdit::slot(
            EditTarget::Path(LpPathBuf::from("/clock.toml")),
            vec![SlotEdit::AssignValue {
                path: SlotPath::parse("controls.rate").unwrap(),
                value: LpValue::F32(2.0),
            }],
        ),
    );
    registry.commit(&fs, Revision::new(3), &ctx).unwrap();
    assert!(!registry.slot_overlay_active());

    fixtures::write_file(
        &mut fs,
        "/clock.toml",
        r#"
kind = "Clock"

[controls]
rate = 7.0
"#,
    );
    registry.sync_fs(&fs, &[fs_modify("/clock.toml")], Revision::new(5), &ctx);

    assert_eq!(clock_rate(registry.get(&root).unwrap()), 7.0);
}

#[test]
fn c2_inline_child_changed_after_commit() {
    let fs = fixtures::load_playlist_with_inline_child();
    let mut registry = NodeDefRegistry::new();
    let shapes = parse_ctx();
    let ctx = ParseCtx { shapes: &shapes };
    let root = registry
        .load_root(&fs, LpPath::new("/playlist.toml"), Revision::new(1), &ctx)
        .unwrap();
    let child = inline_child_id(&registry, root);

    apply_artifact_edit(
        &mut registry,
        &fs,
        &ArtifactEdit::slot(
            EditTarget::Path(LpPathBuf::from("/playlist.toml")),
            vec![SlotEdit::AssignValue {
                path: SlotPath::parse("entries[2].node.def.render_order").unwrap(),
                value: LpValue::I32(7),
            }],
        ),
    );

    let result = registry.commit(&fs, Revision::new(3), &ctx).unwrap();
    assert!(!result.def_updates.changed.contains(&root));
    assert_eq!(result.def_updates.changed, vec![child]);
    assert_eq!(shader_render_order(registry.get(&child).unwrap()), 7);
}

#[test]
fn commit_empty_overlay_is_noop() {
    let fs = fixtures::load_clock();
    let mut registry = NodeDefRegistry::new();
    let shapes = parse_ctx();
    let ctx = ParseCtx { shapes: &shapes };
    registry
        .load_root(&fs, LpPath::new("/clock.toml"), Revision::new(1), &ctx)
        .unwrap();

    let result = registry.commit(&fs, Revision::new(2), &ctx).unwrap();
    assert!(result.is_empty());
}
