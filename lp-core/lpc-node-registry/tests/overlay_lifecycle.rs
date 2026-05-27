//! Overlay apply and discard lifecycle.

mod common;

use common::{fixtures, overlay};
use lpc_model::{LpValue, Revision, SlotPath};
use lpc_node_registry::{EditError, NodeDefEntry, NodeDefLoc, NodeDefRegistry, ParseCtx, SlotEdit};
use lpfs::{LpFsMemory, LpPath, LpPathBuf};

fn snapshot_registry(registry: &NodeDefRegistry, root: &NodeDefLoc) -> NodeDefEntry {
    registry.get(root).expect("root entry").clone()
}

#[test]
fn d1_apply_populates_overlay_base_unchanged() {
    let fs = fixtures::load_clock();
    let mut registry = NodeDefRegistry::new();
    let shapes = overlay::parse_ctx();
    let ctx = ParseCtx { shapes: &shapes };
    let root = registry
        .load_root(&fs, LpPath::new("/clock.toml"), Revision::new(1), &ctx)
        .unwrap();
    let before = snapshot_registry(&registry, &root);

    overlay::set_pending_asset_text(&mut registry, "/pending.glsl", "void main() {}");

    assert!(registry.overlay_active());
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
    let shapes = overlay::parse_ctx();
    let ctx = ParseCtx { shapes: &shapes };
    let root = registry
        .load_root(&fs, LpPath::new("/clock.toml"), Revision::new(1), &ctx)
        .unwrap();
    let before = snapshot_registry(&registry, &root);

    overlay::set_pending_asset_text(&mut registry, "/pending.glsl", "pending");
    assert!(registry.overlay_active());

    registry.discard_slot_overlay();

    assert!(!registry.overlay_active());
    assert!(!registry.slot_overlay_contains_path(LpPath::new("/pending.glsl")));
    assert_eq!(snapshot_registry(&registry, &root), before);
}

#[test]
fn apply_rejects_relative_path() {
    let _fs = LpFsMemory::new();
    let mut registry = NodeDefRegistry::new();
    let err = registry
        .set_pending_asset(
            LpPathBuf::from("relative.glsl"),
            lpc_node_registry::AssetEdit::ReplaceBody(b"x".to_vec()),
        )
        .unwrap_err();
    assert!(matches!(err, EditError::InvalidPath { .. }));
    assert!(!registry.overlay_active());
}

#[test]
fn apply_replace_body_on_unloaded_path_implicit_create() {
    let _fs = LpFsMemory::new();
    let mut registry = NodeDefRegistry::new();
    overlay::set_pending_asset_text(&mut registry, "/new.shader.glsl", "body");
    assert!(registry.slot_overlay_contains_path(LpPath::new("/new.shader.glsl")));
}

#[test]
fn apply_multiple_pending_assets() {
    let _fs = LpFsMemory::new();
    let mut registry = NodeDefRegistry::new();
    overlay::set_pending_asset_text(&mut registry, "/a.glsl", "a");
    overlay::set_pending_asset_text(&mut registry, "/b.glsl", "b");
    assert!(registry.slot_overlay_contains_path(LpPath::new("/a.glsl")));
    assert!(registry.slot_overlay_contains_path(LpPath::new("/b.glsl")));
}

#[test]
fn apply_delete_marks_overlay_entry() {
    let fs = fixtures::load_shader_project();
    let mut registry = NodeDefRegistry::new();
    let shapes = overlay::parse_ctx();
    let ctx = ParseCtx { shapes: &shapes };
    registry
        .load_root(&fs, LpPath::new("/shader.toml"), Revision::new(1), &ctx)
        .unwrap();

    overlay::delete_pending_asset(&mut registry, "/shader.glsl");

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
    let shapes = overlay::parse_ctx();
    let ctx = ParseCtx { shapes: &shapes };
    let err = registry
        .upsert_slot_edit(
            LpPathBuf::from("/shader.glsl"),
            SlotEdit::AssignValue {
                path: SlotPath::root(),
                value: LpValue::F32(1.0),
            },
            &fs,
            &ctx,
            Revision::new(1),
        )
        .unwrap_err();
    assert!(matches!(err, EditError::InvalidPath { .. }));
    assert!(!registry.overlay_active());
}
