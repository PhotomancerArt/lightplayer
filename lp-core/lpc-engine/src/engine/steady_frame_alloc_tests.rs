//! How many allocations does a steady frame make?
//!
//! The emulator heap-budget gate (`docs/heap-budget-gate.md`) ratchets the
//! per-frame allocation count of real projects in CI, but an emulator run is
//! minutes. These tests ask the same question of a small graph on the host in
//! milliseconds, through the test binary's counting allocator
//! (`crate::test_alloc_counter`), so a change to the resolver or the node
//! runtimes shows its per-frame churn in `cargo test`.
//!
//! [`STEADY_FRAME_ALLOC_BUDGET`] is a ratchet, not a target: it holds the
//! measured count and is lowered when the count drops. Raising it is a
//! reviewable decision, never a way to make the test pass.

use super::test_support::dummy_shader_node;
use super::test_support::{
    EngineTestBuilder, EngineTestHarness, bus, literal, output, produced_slot,
};
use crate::frame_plan::PlanMode;
use crate::test_alloc_counter::{AllocSnapshot, measure};
use lpc_model::{Revision, SlotPath};

/// Allocation requests one steady tick of [`steady_harness`]'s graph may make.
///
/// Measured 2026-09-02 (P2 of the zero-alloc-frames plan, then ratcheted in
/// P4 after resolver clone hygiene: `resolve_id`'s `QueryKey` borrow,
/// `Production`'s alloc-free `ProducedSlot` clone, and the borrowed
/// `PanelWriterStore` lookup, and again in P5 after the produced-slot path
/// handles were interned) with the resolver payload cache on. Ratcheted down
/// as the engine's per-frame churn is removed.
///
/// This graph builds on dummy nodes, not the real shader runtimes, so P5's
/// authored-def key cache does not show here; its own oracle is
/// `nodes::shader::shader_node::authored_sync_tests`.
///
/// 8 on 2026-10-24 (the engine frame plan, PR A): a produce dispatch copies
/// the interned slot handle instead of the path, and the tick keeps its
/// producer set and resolve trace across frames.
pub(crate) const STEADY_FRAME_ALLOC_BUDGET: u64 = 8;

/// The richest graph the test builder offers: two shaders behind a selector,
/// a bus channel driven by a third shader and read by a fixture, and an
/// output node fed a literal — one of every resolution route.
///
/// In [`PlanMode::Replay`]: these budgets price what the product allocates,
/// and the test build's default mode rebuilds every plan each frame to check
/// it.
fn steady_harness() -> EngineTestHarness {
    let mut harness = steady_harness_checked();
    harness.engine.set_plan_mode(PlanMode::Replay);
    harness
}

fn steady_harness_checked() -> EngineTestHarness {
    EngineTestBuilder::new()
        .shader("a", output("outputs[0]", 3.0))
        .shader("b", output("outputs[0]", 4.0))
        .shader("clock", output("outputs[0]", 0.5))
        .bind_bus("speed", produced_slot("clock", "outputs[0]"))
        .selector("sel", &[("a", "outputs[0]"), ("b", "outputs[0]")])
        .fixture("fix")
        .bind_demand_input("fix", bus("speed"))
        .output_node("out")
        .bind_demand_input("out", literal(1.0))
        .demand_root("sel")
        .demand_root("fix")
        .demand_root("out")
        .build()
}

fn measured_tick(harness: &mut EngineTestHarness) -> AllocSnapshot {
    let (result, churn) = measure(|| harness.tick(16));
    result.expect("tick");
    churn
}

fn path(p: &str) -> SlotPath {
    SlotPath::parse(p).expect("test slot path")
}

#[test]
fn steady_frame_allocation_budget() {
    let mut harness = steady_harness();
    harness.tick(16).expect("warm-up");
    harness.tick(16).expect("warm-up");

    let churn = measured_tick(&mut harness);
    // Printed so `cargo test steady_frame -- --nocapture` reports the number
    // to ratchet to.
    std::eprintln!("steady frame: {churn:?} (budget {STEADY_FRAME_ALLOC_BUDGET})");
    assert!(
        churn.allocs <= STEADY_FRAME_ALLOC_BUDGET,
        "a steady frame made {} allocation requests ({} B); the budget is {}. \
         If the growth is intentional, raise STEADY_FRAME_ALLOC_BUDGET in the same change \
         and say why in the commit.",
        churn.allocs,
        churn.bytes,
        STEADY_FRAME_ALLOC_BUDGET
    );
    assert_eq!(harness.output_f32("sel"), Some(3.0));
    assert_eq!(harness.fixture_f32("fix"), Some(0.5));
    assert_eq!(harness.output_f32("out"), Some(1.0));
}

/// Two consecutive steady frames allocate the same amount: per-frame growth
/// (a cache that appends instead of overwriting, a table regrown each tick)
/// shows up here before it shows up as a slow leak on device.
#[test]
fn steady_frame_allocation_is_stable() {
    let mut harness = steady_harness();
    harness.tick(16).expect("warm-up");
    harness.tick(16).expect("warm-up");

    let first = measured_tick(&mut harness);
    let second = measured_tick(&mut harness);
    assert_eq!(
        first, second,
        "steady frames must allocate identically: {first:?} then {second:?}"
    );
}

/// A structural change is allowed to cost the frame that absorbs it; the
/// frame after must be back to exactly the steady figure — the caches
/// refill, they do not stay cold, and a same-shape graph churns the same.
#[test]
fn structural_change_then_steady_again() {
    let mut harness = steady_harness();
    harness.tick(16).expect("warm-up");
    harness.tick(16).expect("warm-up");
    let steady_before = measured_tick(&mut harness);

    let a = harness.node("a");
    harness
        .engine
        .reattach_runtime_node(
            a,
            dummy_shader_node(path("outputs[0]"), 6.0),
            Revision::new(9),
        )
        .expect("reattach");

    harness.tick(16).expect("the frame that absorbs the change");
    assert_eq!(harness.output_f32("sel"), Some(6.0));

    let steady_after = measured_tick(&mut harness);
    assert_eq!(
        steady_after, steady_before,
        "a same-shape graph must churn the same after re-resolution"
    );
}

/// The classic runs the resolver with payloads off. That configuration may
/// allocate more per frame (defaults are re-materialised), but it must be
/// just as stable frame to frame.
#[test]
fn payloads_off_is_stable_too() {
    let mut harness = steady_harness();
    harness.engine.resolver_mut().set_retain_payloads(false);
    harness.tick(16).expect("warm-up");
    harness.tick(16).expect("warm-up");

    let first = measured_tick(&mut harness);
    let second = measured_tick(&mut harness);
    assert_eq!(
        first, second,
        "payloads-off frames must allocate identically"
    );
}

/// Steady-frame allocations per lane (a fixture and the output it feeds) on
/// the REAL fixture, shader and output nodes, measured as the difference
/// between two projects with the same 1,024 LEDs: `projects/test/lanes-8x128`
/// minus `lanes-2x512`, over the six lanes between them. The LED term cancels,
/// so what is left is what the engine re-derives per lane per frame.
///
/// A ratchet, not a target (planning `lp2025/2026-10-24-0208-engine-frame-plan`,
/// AC3): each frame-plan phase lowers it, and the plan ends at ≤ 2. 45 before
/// the plan; 42 after PR A (the engine's own per-dispatch and per-frame
/// scratch).
pub(crate) const PER_LANE_STEADY_ALLOCS_BUDGET: u64 = 42;

#[test]
fn steady_frame_allocations_do_not_scale_with_lanes() {
    let two = steady_project_allocs("lanes-2x512");
    let eight = steady_project_allocs("lanes-8x128");
    let per_lane = eight.allocs.saturating_sub(two.allocs).div_ceil(6);
    std::eprintln!(
        "steady frame: lanes-2x512 {two:?}, lanes-8x128 {eight:?}; \
         {per_lane} allocations per lane (budget {PER_LANE_STEADY_ALLOCS_BUDGET})"
    );
    assert!(
        per_lane <= PER_LANE_STEADY_ALLOCS_BUDGET,
        "a steady frame makes {per_lane} allocation requests per lane; the budget is \
         {PER_LANE_STEADY_ALLOCS_BUDGET}. If the growth is intentional, raise \
         PER_LANE_STEADY_ALLOCS_BUDGET in the same change and say why in the commit."
    );
}

/// One steady frame of a checked-in test project, loaded through the project
/// loader with the CPU graphics backend: three warm-up frames (the first
/// compiles), then the measured one.
fn steady_project_allocs(project: &str) -> AllocSnapshot {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../projects/test")
        .join(project);
    let fs = lpfs::LpFsStd::new(dir);
    let root = alloc::format!("/{}.show", project.replace('-', "_"));
    let services = crate::EngineServices::new(lpc_model::TreePath::parse(&root).expect("root"));
    let mut rt = crate::ProjectLoader::load_from_root(&fs, services).expect("load lane project");
    rt.set_plan_mode(PlanMode::Replay);
    rt.set_graphics(Some(alloc::sync::Arc::new(
        lp_gfx_lpvm::TargetLpvmGraphics::new(lp_shader::ShaderFrontend::LpsGlsl),
    )));
    for _ in 0..3 {
        rt.tick(16).expect("warm-up");
    }
    let (result, churn) = measure(|| rt.tick(16));
    result.expect("measured tick");
    churn
}
