//! Effective projection — view vs committed (M2).

mod common;

use common::fixtures;
use lpc_model::{NodeDef, Revision, SlotShapeRegistry};
use lpc_node_registry::{
    ArtifactChange, ArtifactOp, ArtifactTarget, NodeDefEntry, NodeDefId, NodeDefRegistry,
    NodeDefState, ParseCtx,
};
use lpfs::{LpPath, LpPathBuf};

fn parse_ctx() -> SlotShapeRegistry {
    SlotShapeRegistry::default()
}

fn clock_rate(entry: &NodeDefEntry) -> f32 {
    let NodeDefState::Loaded(NodeDef::Clock(def)) = &entry.state else {
        panic!("expected loaded clock def");
    };
    *def.controls.rate.value()
}

fn load_clock_root(registry: &mut NodeDefRegistry, fs: &dyn lpfs::LpFs) -> NodeDefId {
    let shapes = parse_ctx();
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
    let shapes = parse_ctx();
    let ctx = ParseCtx { shapes: &shapes };

    assert_eq!(clock_rate(registry.get(&root).unwrap()), 1.0);

    registry
        .apply_change(&ArtifactChange {
            target: ArtifactTarget::Path(LpPathBuf::from("/clock.toml")),
            ops: vec![ArtifactOp::SetBytes(
                r#"
kind = "Clock"

[controls]
rate = 2.0
"#
                .into(),
            )],
        })
        .unwrap();

    let effective = registry.view().get(&root, &fs, &ctx).unwrap();
    assert_eq!(clock_rate(&effective), 2.0);
    assert_eq!(clock_rate(registry.get(&root).unwrap()), 1.0);
}

#[test]
fn effective_view_matches_committed_without_overlay() {
    let fs = fixtures::load_clock();
    let mut registry = NodeDefRegistry::new();
    let root = load_clock_root(&mut registry, &fs);
    let shapes = parse_ctx();
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
    let shapes = parse_ctx();
    let ctx = ParseCtx { shapes: &shapes };

    registry
        .apply_change(&ArtifactChange {
            target: ArtifactTarget::Path(LpPathBuf::from("/clock.toml")),
            ops: vec![ArtifactOp::SetBytes(
                r#"
kind = "Clock"

[controls]
rate = 2.0
"#
                .into(),
            )],
        })
        .unwrap();
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
    let shapes = parse_ctx();
    let ctx = ParseCtx { shapes: &shapes };

    registry
        .apply_change(&ArtifactChange {
            target: ArtifactTarget::Path(LpPathBuf::from("/clock.toml")),
            ops: vec![ArtifactOp::Delete],
        })
        .unwrap();

    assert!(matches!(
        registry.view().state(&root, &fs, &ctx),
        Some(NodeDefState::ParseError(_))
    ));
    assert!(registry.get(&root).unwrap().state.is_loaded());
}
