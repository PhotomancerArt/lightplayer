//! Slot overlay apply and effective projection.

mod common;

use common::{fixtures, overlay};
use lpc_model::{LpValue, NodeDef, Revision, SlotPath};
use lpc_node_registry::{
    NodeDefEntry, NodeDefLocation, NodeDefRegistry, NodeDefState, ParseCtx, SlotEdit,
    serialize_slot_draft,
};
use lpfs::LpPath;

fn clock_rate(entry: &NodeDefEntry) -> f32 {
    let NodeDefState::Loaded(NodeDef::Clock(def)) = &entry.state else {
        panic!("expected loaded clock def");
    };
    *def.controls.rate.value()
}

fn clock_scrub_offset(entry: &NodeDefEntry) -> f32 {
    let NodeDefState::Loaded(NodeDef::Clock(def)) = &entry.state else {
        panic!("expected loaded clock def");
    };
    *def.controls.scrub_offset_seconds.value()
}

fn shader_render_order(entry: &NodeDefEntry) -> i32 {
    let NodeDefState::Loaded(NodeDef::Shader(def)) = &entry.state else {
        panic!("expected loaded shader def");
    };
    def.render_order()
}

fn inline_child_loc(root: &NodeDefLocation) -> NodeDefLocation {
    NodeDefLocation {
        artifact: root.artifact.clone(),
        path: SlotPath::parse("entries[2].node").unwrap(),
    }
}

#[test]
fn c1_setslot_patches_clock_rate_in_view() {
    let fs = fixtures::load_clock();
    let mut registry = NodeDefRegistry::new();
    let shapes = overlay::parse_ctx();
    let ctx = ParseCtx { shapes: &shapes };
    let root = registry
        .load_root(&fs, LpPath::new("/clock.toml"), Revision::new(1), &ctx)
        .unwrap();

    overlay::upsert_slot(
        &mut registry,
        &fs,
        "/clock.toml",
        SlotEdit::assign_value(SlotPath::parse("controls.rate").unwrap(), LpValue::F32(2.0)),
        Revision::new(2),
    );

    let effective = registry.view().get(&root, &fs, &ctx).unwrap();
    assert_eq!(clock_rate(&effective), 2.0);
    assert_eq!(clock_rate(registry.get(&root).unwrap()), 1.0);
}

#[test]
fn c1_slot_draft_serializes_to_toml() {
    let fs = fixtures::load_clock();
    let mut registry = NodeDefRegistry::new();
    let shapes = overlay::parse_ctx();
    let ctx = ParseCtx { shapes: &shapes };
    registry
        .load_root(&fs, LpPath::new("/clock.toml"), Revision::new(1), &ctx)
        .unwrap();

    overlay::upsert_slot(
        &mut registry,
        &fs,
        "/clock.toml",
        SlotEdit::assign_value(SlotPath::parse("controls.rate").unwrap(), LpValue::F32(2.0)),
        Revision::new(2),
    );

    let bytes = registry
        .read_effective_bytes(LpPath::new("/clock.toml"), &fs, &ctx)
        .unwrap()
        .expect("effective bytes");
    let text = core::str::from_utf8(&bytes).unwrap();
    assert!(text.contains("rate = 2"));
    let reparsed = NodeDef::read_toml(&shapes, text).unwrap();
    let NodeDef::Clock(def) = reparsed else {
        panic!("expected clock");
    };
    assert_eq!(*def.controls.rate.value(), 2.0);

    let draft_def = registry.overlay_contains_path(LpPath::new("/clock.toml"));
    assert!(draft_def);
    let effective = registry
        .view()
        .get(registry.root_loc().unwrap(), &fs, &ctx)
        .unwrap();
    let serialized = serialize_slot_draft(
        match effective.state {
            NodeDefState::Loaded(ref def) => def,
            _ => panic!("expected loaded"),
        },
        &ctx,
    )
    .unwrap();
    assert_eq!(serialized, bytes);
}

#[test]
fn c1_root_variant_path_preserves_existing_same_kind_payload() {
    let mut fs = fixtures::load_clock();
    fixtures::write_file(
        &mut fs,
        "/clock.toml",
        r#"
kind = "Clock"

[controls]
rate = 2.5
"#,
    );
    let mut registry = NodeDefRegistry::new();
    let shapes = overlay::parse_ctx();
    let ctx = ParseCtx { shapes: &shapes };
    let root = registry
        .load_root(&fs, LpPath::new("/clock.toml"), Revision::new(1), &ctx)
        .unwrap();

    overlay::upsert_slot(
        &mut registry,
        &fs,
        "/clock.toml",
        SlotEdit::assign_value(
            SlotPath::parse("Clock.controls.scrub_offset_seconds").unwrap(),
            LpValue::F32(4.0),
        ),
        Revision::new(2),
    );

    let effective = registry.view().get(&root, &fs, &ctx).unwrap();
    assert_eq!(clock_rate(&effective), 2.5);
    assert_eq!(clock_scrub_offset(&effective), 4.0);
}

fn playlist_idle_entry(entry: &NodeDefEntry) -> u32 {
    let NodeDefState::Loaded(NodeDef::Playlist(def)) = &entry.state else {
        panic!("expected loaded playlist def");
    };
    *def.idle_entry.value()
}

#[test]
fn c2_playlist_slot_patch_committed_children_unchanged() {
    let fs = fixtures::load_playlist_with_inline_child();
    let mut registry = NodeDefRegistry::new();
    let shapes = overlay::parse_ctx();
    let ctx = ParseCtx { shapes: &shapes };
    let root = registry
        .load_root(&fs, LpPath::new("/playlist.toml"), Revision::new(1), &ctx)
        .unwrap();
    let child = inline_child_loc(&root);
    let child_before = registry.get(&child).unwrap().clone();
    let committed_idle = playlist_idle_entry(registry.get(&root).unwrap());

    overlay::upsert_slot(
        &mut registry,
        &fs,
        "/playlist.toml",
        SlotEdit::assign_value(SlotPath::parse("idle_entry").unwrap(), LpValue::U32(99)),
        Revision::new(2),
    );

    let effective = registry.view().get(&root, &fs, &ctx).unwrap();
    assert_eq!(playlist_idle_entry(&effective), 99);
    assert_eq!(
        playlist_idle_entry(registry.get(&root).unwrap()),
        committed_idle
    );
    assert_eq!(registry.get(&child).unwrap(), &child_before);
}

#[test]
fn c2_inline_child_slot_patch_visible_in_view() {
    let fs = fixtures::load_playlist_with_inline_child();
    let mut registry = NodeDefRegistry::new();
    let shapes = overlay::parse_ctx();
    let ctx = ParseCtx { shapes: &shapes };
    let root = registry
        .load_root(&fs, LpPath::new("/playlist.toml"), Revision::new(1), &ctx)
        .unwrap();
    let child = inline_child_loc(&root);
    let before = registry.get(&child).unwrap().clone();

    overlay::upsert_slot(
        &mut registry,
        &fs,
        "/playlist.toml",
        SlotEdit::assign_value(
            SlotPath::parse("entries[2].node.def.render_order").unwrap(),
            LpValue::I32(7),
        ),
        Revision::new(2),
    );

    let effective = registry.view().get(&child, &fs, &ctx).unwrap();
    assert_eq!(shader_render_order(&effective), 7);
    assert_eq!(registry.get(&child).unwrap(), &before);
}
