//! Filesystem change sync semantics.

mod common;

use common::fixtures;
use lpc_model::{NodeKind, Revision, SlotPath, SlotShapeRegistry};
use lpc_node_registry::{DefChangeDetail, DefSource, NodeDefRegistry, ParseCtx, SyncResult};
use lpfs::{ChangeType, FsChange, LpPath, LpPathBuf};

fn parse_ctx() -> SlotShapeRegistry {
    SlotShapeRegistry::default()
}

fn fs_modify(path: &str) -> FsChange {
    FsChange {
        path: LpPathBuf::from(path),
        change_type: ChangeType::Modify,
    }
}

fn sync_at(
    registry: &mut NodeDefRegistry,
    fs: &lpfs::LpFsMemory,
    path: &str,
    frame: i64,
    ctx: &ParseCtx<'_>,
) -> SyncResult {
    registry.sync_fs(fs, &[fs_modify(path)], Revision::new(frame), ctx)
}

fn inline_child_id(
    registry: &NodeDefRegistry,
    root: lpc_node_registry::NodeDefId,
) -> lpc_node_registry::NodeDefId {
    let artifact_id = registry.get(&root).unwrap().source.artifact_id;
    registry
        .get_by_source(&DefSource {
            artifact_id,
            path: SlotPath::parse("entries[2].node").unwrap(),
        })
        .expect("inline child")
        .id
}

#[test]
fn s1_leaf_toml_edit_marks_root_changed() {
    let mut fs = fixtures::load_clock();
    let mut registry = NodeDefRegistry::new();
    let shapes = parse_ctx();
    let ctx = ParseCtx { shapes: &shapes };
    let root = registry
        .load_root(&fs, LpPath::new("/clock.toml"), Revision::new(1), &ctx)
        .unwrap();

    fixtures::write_file(
        &mut fs,
        "/clock.toml",
        r#"
kind = "Clock"

[controls]
rate = 2.0
"#,
    );
    let result = sync_at(&mut registry, &fs, "/clock.toml", 2, &ctx);
    assert_eq!(result.def_updates.changed, vec![root]);
    assert!(result.def_updates.added.is_empty());
    assert!(result.def_updates.removed.is_empty());
}

#[test]
fn s2_glsl_edit_only_bumps_source_revision() {
    let mut fs = fixtures::load_shader_project();
    let mut registry = NodeDefRegistry::new();
    let shapes = parse_ctx();
    let ctx = ParseCtx { shapes: &shapes };
    let shader_id = registry
        .load_root(&fs, LpPath::new("/shader.toml"), Revision::new(1), &ctx)
        .unwrap();

    fixtures::write_file(
        &mut fs,
        "/shader.glsl",
        "void main() { gl_FragColor = vec4(0.0); }",
    );
    let result = sync_at(&mut registry, &fs, "/shader.glsl", 2, &ctx);
    assert!(result.def_updates.is_empty());
    assert!(
        result
            .source_revisions
            .iter()
            .any(|bump| bump.def_id == shader_id && bump.after > bump.before)
    );
}

#[test]
fn s3_svg_edit_only_bumps_source_revision() {
    let mut fs = fixtures::load_fixture_project();
    let mut registry = NodeDefRegistry::new();
    let shapes = parse_ctx();
    let ctx = ParseCtx { shapes: &shapes };
    let fixture_id = registry
        .load_root(&fs, LpPath::new("/fixture.toml"), Revision::new(1), &ctx)
        .unwrap();

    fixtures::write_file(
        &mut fs,
        "/mapping.svg",
        r#"<svg xmlns="http://www.w3.org/2000/svg"><path d="M1 1"/></svg>"#,
    );
    let result = sync_at(&mut registry, &fs, "/mapping.svg", 2, &ctx);
    assert!(result.def_updates.is_empty());
    assert!(
        result
            .source_revisions
            .iter()
            .any(|bump| bump.def_id == fixture_id && bump.after > bump.before)
    );
}

#[test]
fn s4_inline_child_edit_isolated() {
    let mut fs = fixtures::load_playlist_with_inline_child();
    let mut registry = NodeDefRegistry::new();
    let shapes = parse_ctx();
    let ctx = ParseCtx { shapes: &shapes };
    let root = registry
        .load_root(&fs, LpPath::new("/playlist.toml"), Revision::new(1), &ctx)
        .unwrap();
    let child = inline_child_id(&registry, root);

    fixtures::write_file(
        &mut fs,
        "/playlist.toml",
        r#"
kind = "Playlist"

[entries.2.node.def]
kind = "Shader"
source = { path = "b.glsl" }
"#,
    );
    let result = sync_at(&mut registry, &fs, "/playlist.toml", 2, &ctx);
    assert!(!result.def_updates.changed.contains(&root));
    assert_eq!(result.def_updates.changed, vec![child]);
}

#[test]
fn s5a_leaf_parse_error_reports_entered_error() {
    let mut fs = fixtures::load_clock();
    let mut registry = NodeDefRegistry::new();
    let shapes = parse_ctx();
    let ctx = ParseCtx { shapes: &shapes };
    let root = registry
        .load_root(&fs, LpPath::new("/clock.toml"), Revision::new(1), &ctx)
        .unwrap();

    fixtures::write_file(&mut fs, "/clock.toml", "kind = \"Clock\"\nrate = ");
    let result = sync_at(&mut registry, &fs, "/clock.toml", 2, &ctx);
    assert_eq!(result.def_updates.changed, vec![root]);
    assert!(matches!(
        result.change_details.as_slice(),
        [(id, DefChangeDetail::EnteredError)] if *id == root
    ));
}

#[test]
fn s5b_path_child_parse_error_reports_entered_error() {
    let mut fs = lpfs::LpFsMemory::new();
    fixtures::write_file(
        &mut fs,
        "/playlist.toml",
        r#"
kind = "Playlist"

[entries.2]
node = { def = { path = "./child.toml" } }
"#,
    );
    fixtures::write_file(&mut fs, "/child.toml", "kind = \"Shader\"\n");
    let mut registry = NodeDefRegistry::new();
    let shapes = parse_ctx();
    let ctx = ParseCtx { shapes: &shapes };
    let root = registry
        .load_root(&fs, LpPath::new("/playlist.toml"), Revision::new(1), &ctx)
        .unwrap();
    let child = registry
        .iter_entries()
        .find(|entry| entry.source.path.is_root() && entry.id != root)
        .expect("path child")
        .id;

    fixtures::write_file(&mut fs, "/child.toml", "kind = \"Shader\"\nsource = ");
    let result = sync_at(&mut registry, &fs, "/child.toml", 2, &ctx);
    assert!(!result.def_updates.changed.contains(&root));
    assert_eq!(result.def_updates.changed, vec![child]);
    assert!(matches!(
        result.change_details.as_slice(),
        [(id, DefChangeDetail::EnteredError)] if *id == child
    ));
}

#[test]
fn s6_kind_change_reports_kind_changed() {
    let mut fs = fixtures::load_playlist_with_inline_child();
    let mut registry = NodeDefRegistry::new();
    let shapes = parse_ctx();
    let ctx = ParseCtx { shapes: &shapes };
    let root = registry
        .load_root(&fs, LpPath::new("/playlist.toml"), Revision::new(1), &ctx)
        .unwrap();
    let child = inline_child_id(&registry, root);

    fixtures::write_file(
        &mut fs,
        "/playlist.toml",
        r#"
kind = "Playlist"

[entries.2.node.def]
kind = "Clock"
"#,
    );
    let result = sync_at(&mut registry, &fs, "/playlist.toml", 2, &ctx);
    assert!(result.def_updates.changed.contains(&root));
    assert!(result.def_updates.changed.contains(&child));
    assert!(result.change_details.iter().any(|(id, detail)| *id == child
        && matches!(
            detail,
            DefChangeDetail::KindChanged {
                from: NodeKind::Shader,
                to: NodeKind::Clock
            }
        )));
}
