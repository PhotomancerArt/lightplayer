//! Project diff and post-commit equivalence checks.

use lpc_model::{Revision, SlotShapeRegistry};
use lpc_node_registry::{NodeDefRegistry, ParseCtx, ProjectSnapshot, assert_equivalent, diff};
use lpfs::{LpFsStd, LpPath};

fn parse_ctx() -> SlotShapeRegistry {
    SlotShapeRegistry::default()
}

fn examples_basic_snapshot() -> ProjectSnapshot {
    let fs = examples_basic_fs();
    ProjectSnapshot::from_fs(&fs).expect("basic snapshot")
}

fn examples_basic2_snapshot() -> ProjectSnapshot {
    let fs = examples_basic2_fs();
    ProjectSnapshot::from_fs(&fs).expect("basic2 snapshot")
}

fn examples_basic_fs() -> LpFsStd {
    LpFsStd::new(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/basic"))
}

fn examples_basic2_fs() -> LpFsStd {
    LpFsStd::new(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/basic2"))
}

#[test]
fn a1_diff_empty_to_basic_apply_commit_equivalent() {
    let shapes = parse_ctx();
    let ctx = ParseCtx { shapes: &shapes };
    let base = ProjectSnapshot::empty();
    let target = examples_basic_snapshot();
    let overlay = diff(&base, &target, &ctx).expect("diff");

    let fs = lpfs::LpFsMemory::new();
    let mut registry = NodeDefRegistry::new();
    registry.apply_overlay(&overlay);
    registry
        .commit(&fs, Revision::new(2), &ctx)
        .expect("commit");

    assert_equivalent(&fs, &target, &ctx).expect("equivalent");
}

#[test]
fn a1_roundtrip_load_root_after_commit() {
    let shapes = parse_ctx();
    let ctx = ParseCtx { shapes: &shapes };
    let target = examples_basic_snapshot();
    let overlay = diff(&ProjectSnapshot::empty(), &target, &ctx).expect("diff");

    let fs = lpfs::LpFsMemory::new();
    let mut registry = NodeDefRegistry::new();
    registry.apply_overlay(&overlay);
    registry.commit(&fs, Revision::new(2), &ctx).unwrap();

    let mut loaded = NodeDefRegistry::new();
    loaded
        .load_root(&fs, LpPath::new("/project.toml"), Revision::new(3), &ctx)
        .expect("load_root");
    assert!(loaded.root_loc().is_some());
}

#[test]
fn b1_diff_basic_to_basic2_apply_commit_equivalent() {
    let shapes = parse_ctx();
    let ctx = ParseCtx { shapes: &shapes };
    let base = examples_basic_snapshot();
    let target = examples_basic2_snapshot();
    let overlay = diff(&base, &target, &ctx).expect("diff");

    let fs = base.copy_to_memory_fs();
    let mut registry = NodeDefRegistry::new();
    registry
        .load_root(&fs, LpPath::new("/project.toml"), Revision::new(1), &ctx)
        .expect("load_root");
    registry.apply_overlay(&overlay);
    registry
        .commit(&fs, Revision::new(3), &ctx)
        .expect("commit");

    assert_equivalent(&fs, &target, &ctx).expect("equivalent");
}

#[test]
fn diff_identical_snapshots_is_empty() {
    let shapes = parse_ctx();
    let ctx = ParseCtx { shapes: &shapes };
    let snapshot = examples_basic_snapshot();
    let overlay = diff(&snapshot, &snapshot, &ctx).expect("diff");
    assert!(overlay.is_empty());
}
