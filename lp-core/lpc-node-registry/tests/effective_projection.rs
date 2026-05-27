//! Effective projection: [`NodeDefView`] vs committed cache.

mod common;

use common::{fixtures, overlay};
use lpc_model::{NodeDef, Revision};
use lpc_node_registry::{NodeDefEntry, NodeDefLoc, NodeDefRegistry, NodeDefState, ParseCtx};
use lpfs::LpPath;

fn clock_rate(entry: &NodeDefEntry) -> f32 {
    let NodeDefState::Loaded(NodeDef::Clock(def)) = &entry.state else {
        panic!("expected loaded clock def");
    };
    *def.controls.rate.value()
}

fn load_clock_root(registry: &mut NodeDefRegistry, fs: &dyn lpfs::LpFs) -> NodeDefLoc {
    let shapes = overlay::parse_ctx();
    let ctx = ParseCtx { shapes: &shapes };
    registry
        .load_root(fs, LpPath::new("/clock.toml"), Revision::new(1), &ctx)
        .unwrap()
}

#[test]
fn effective_view_differs_after_toml_setbytes() {
    let fs = fixtures::load_clock();
    let mut registry = NodeDefRegistry::new();
    let root = load_clock_root(&mut registry, &fs);
    let shapes = overlay::parse_ctx();
    let ctx = ParseCtx { shapes: &shapes };

    assert_eq!(clock_rate(registry.get(&root).unwrap()), 1.0);

    overlay::set_pending_asset_text(
        &mut registry,
        "/clock.toml",
        r#"
kind = "Clock"

[controls]
rate = 2.0
"#,
    );

    let effective = registry.view().get(&root, &fs, &ctx).unwrap();
    assert_eq!(clock_rate(&effective), 2.0);
    assert_eq!(clock_rate(registry.get(&root).unwrap()), 1.0);
}

#[test]
fn effective_view_matches_committed_without_overlay() {
    let fs = fixtures::load_clock();
    let mut registry = NodeDefRegistry::new();
    let root = load_clock_root(&mut registry, &fs);
    let shapes = overlay::parse_ctx();
    let ctx = ParseCtx { shapes: &shapes };

    let committed = registry.get(&root).unwrap().clone();
    let effective = registry.view().get(&root, &fs, &ctx).unwrap();
    assert_eq!(effective, committed);
}

#[test]
fn discard_restores_effective_view_to_committed() {
    let fs = fixtures::load_clock();
    let mut registry = NodeDefRegistry::new();
    let root = load_clock_root(&mut registry, &fs);
    let shapes = overlay::parse_ctx();
    let ctx = ParseCtx { shapes: &shapes };

    overlay::set_pending_asset_text(
        &mut registry,
        "/clock.toml",
        r#"
kind = "Clock"

[controls]
rate = 2.0
"#,
    );
    assert_eq!(
        clock_rate(&registry.view().get(&root, &fs, &ctx).unwrap()),
        2.0
    );

    registry.discard_overlay();

    let committed = registry.get(&root).unwrap().clone();
    let effective = registry.view().get(&root, &fs, &ctx).unwrap();
    assert_eq!(effective, committed);
}

#[test]
fn effective_deleted_overlay_yields_parse_error() {
    let fs = fixtures::load_clock();
    let mut registry = NodeDefRegistry::new();
    let root = load_clock_root(&mut registry, &fs);
    let shapes = overlay::parse_ctx();
    let ctx = ParseCtx { shapes: &shapes };

    overlay::delete_pending_asset(&mut registry, "/clock.toml");

    assert!(matches!(
        registry.view().state(&root, &fs, &ctx),
        Some(NodeDefState::ParseError(_))
    ));
    assert!(registry.get(&root).unwrap().state.is_loaded());
}
