//! The project-level fault verdict: what sets it, what must NOT, and how it
//! clears.
//!
//! The distinction these tests pin is the whole policy (D1/D3): a RUNTIME
//! failure faults the project and every output paints the never-black
//! pattern; an AUTHORING error does not, however loudly the node complains.
//! Getting that backwards would breathe red at anyone mid-edit.

use alloc::string::String;
use lpc_wire::NodeRuntimeStatus;

use crate::engine::FaultPresentation;

use super::test_support::{EngineTestBuilder, output, produced_slot};

/// The bench case's shape: the producer's tick fails (a crash-recovery
/// denial arrives at exactly this site as an `Err`), so the PRODUCER faults
/// and the project with it — no propagation along edges needed.
#[test]
fn a_failed_produce_faults_the_producer_and_the_project() {
    let mut h = EngineTestBuilder::new()
        .failing_producer(
            "shader",
            output("outputs[0]", 0.75),
            "intentional produce failure",
        )
        .tolerant_fixture(
            "fixture",
            NodeRuntimeStatus::Error(String::from("fixture input not resolved")),
        )
        .bind_demand_input("fixture", produced_slot("shader", "outputs[0]"))
        .demand_root("fixture")
        .build();

    h.tick(16)
        .expect("the tolerant consumer keeps the tick alive");

    assert!(
        matches!(h.status("shader"), NodeRuntimeStatus::Fault(message) if message.contains("intentional produce failure")),
        "the producer's tick failure is a Fault: {:?}",
        h.status("shader"),
    );
    assert!(
        matches!(h.status("fixture"), NodeRuntimeStatus::Error(_)),
        "the consumer keeps whatever IT reports: {:?}",
        h.status("fixture"),
    );

    let fault = h.engine.project_fault().expect("the project is faulted");
    assert_eq!(fault.nodes.len(), 1, "only the producer: {:?}", fault.nodes);
    assert!(
        fault.nodes[0].0.ends_with("shader"),
        "the verdict names the faulted node's tree path: {:?}",
        fault.nodes,
    );
}

/// The line that keeps this feature usable: a fixture with nothing bound, a
/// mapping that will not parse, a GLSL diagnostic — all loud, all fixed by
/// an EDIT, none of them a reason to paint every wire red.
#[test]
fn an_authoring_error_never_faults_the_project() {
    let mut h = EngineTestBuilder::new()
        .shader("shader", output("outputs[0]", 0.5))
        .tolerant_fixture(
            "fixture",
            NodeRuntimeStatus::Error(String::from("fixture input not resolved")),
        )
        .bind_demand_input("fixture", produced_slot("shader", "outputs[0]"))
        .demand_root("fixture")
        .build();

    h.tick(16).expect("tick");

    assert!(matches!(h.status("fixture"), NodeRuntimeStatus::Error(_)));
    assert!(
        h.engine.project_fault().is_none(),
        "an Error is not a Fault: {:?}",
        h.engine.project_fault(),
    );
}

/// A `Warn` is even further from a fault — the shader running on an input's
/// authored default is a normal, working show.
#[test]
fn a_warning_never_faults_the_project() {
    let mut h = EngineTestBuilder::new()
        .shader("shader", output("outputs[0]", 0.5))
        .tolerant_fixture(
            "fixture",
            NodeRuntimeStatus::Warn(String::from("input using its default")),
        )
        .bind_demand_input("fixture", produced_slot("shader", "outputs[0]"))
        .demand_root("fixture")
        .build();

    h.tick(16).expect("tick");

    assert!(h.engine.project_fault().is_none());
}

/// The clock is CONTINUOUS: it starts at the first faulted tick and does not
/// restart every frame, or the one-second persistence delay would never
/// elapse and the pattern would never paint.
#[test]
fn the_fault_clock_starts_at_the_first_faulted_tick_and_holds() {
    let mut h = EngineTestBuilder::new()
        .failing_producer("shader", output("outputs[0]", 0.75), "boom")
        .tolerant_fixture("fixture", NodeRuntimeStatus::Ok)
        .bind_demand_input("fixture", produced_slot("shader", "outputs[0]"))
        .demand_root("fixture")
        .build();

    h.tick(500).expect("tick");
    let since = h.engine.project_fault().expect("faulted").since_seconds;
    assert!((since - 0.5).abs() < 1e-6, "{since}");

    // The node list must not be REBUILT every frame either: the device this
    // feature exists for sat quarantined for two days, and re-allocating a
    // path string per faulted node per frame on a heap that already ran out
    // is the wrong way to report an out-of-memory fault. A stable buffer
    // pointer is the cheap proof the fingerprint short-circuit holds.
    let nodes_ptr = h.engine.project_fault().expect("faulted").nodes.as_ptr();

    for _ in 0..4 {
        h.tick(500).expect("tick");
    }
    let standing = h.engine.project_fault().expect("still faulted");
    let held = standing.since_seconds;
    assert!(
        (held - since).abs() < 1e-6,
        "the clock held at {since}, got {held}",
    );
    assert_eq!(
        standing.nodes.as_ptr(),
        nodes_ptr,
        "a standing fault re-derives to the same list without rebuilding it",
    );
}

/// Derived, never latched: the tick after the failure stops is clean, with
/// nothing to invalidate.
#[test]
fn a_fault_that_stops_clears_on_the_next_tick() {
    let mut h = EngineTestBuilder::new()
        .failing_producer("shader", output("outputs[0]", 0.75), "boom")
        .tolerant_fixture("fixture", NodeRuntimeStatus::Ok)
        .bind_demand_input("fixture", produced_slot("shader", "outputs[0]"))
        .demand_root("fixture")
        .build();

    h.tick(16).expect("tick");
    assert!(h.engine.project_fault().is_some());

    h.set_failing("shader", false);
    h.tick(16).expect("tick");

    assert!(h.engine.project_fault().is_none());
    assert!(matches!(h.status("shader"), NodeRuntimeStatus::Ok));
}

/// `clear_faults` resets the statuses so the NEXT tick re-derives the truth
/// — and re-derives it honestly: a failure that is still there faults again
/// straight away, which is what makes the verb safe to offer.
#[test]
fn clear_faults_resets_statuses_and_the_next_tick_re_derives() {
    let mut h = EngineTestBuilder::new()
        .failing_producer("shader", output("outputs[0]", 0.75), "boom")
        .tolerant_fixture("fixture", NodeRuntimeStatus::Ok)
        .bind_demand_input("fixture", produced_slot("shader", "outputs[0]"))
        .demand_root("fixture")
        .build();

    h.tick(16).expect("tick");
    assert!(h.engine.project_fault().is_some());

    h.engine.clear_faults();
    assert!(h.engine.project_fault().is_none(), "cleared on the spot");
    assert!(matches!(h.status("shader"), NodeRuntimeStatus::Ok));

    h.tick(16).expect("tick");
    assert!(
        h.engine.project_fault().is_some(),
        "the failure is still there, so it faults again — clearing is not fixing",
    );

    h.set_failing("shader", false);
    h.engine.clear_faults();
    h.tick(16).expect("tick");
    assert!(h.engine.project_fault().is_none());
}

/// The knob's default is the honest one (D2).
#[test]
fn the_presentation_knob_defaults_to_pattern_and_is_settable() {
    let mut h = EngineTestBuilder::new().build();
    assert_eq!(h.engine.fault_presentation(), FaultPresentation::Pattern);
    h.engine.set_fault_presentation(FaultPresentation::Black);
    assert_eq!(h.engine.fault_presentation(), FaultPresentation::Black);
}

/// One output failing must not take its siblings dark for the frame (G1
/// bench, 2026-09-02): the walk used to stop at the first failing demand
/// root, so every later output kept a stale buffer — and under a project
/// fault never got to paint the pattern itself. Now every root gets its
/// turn and the FIRST error is still the frame's verdict.
#[test]
fn a_failing_demand_root_does_not_stop_the_roots_after_it() {
    let mut h = EngineTestBuilder::new()
        .failing_producer(
            "broken_shader",
            output("outputs[0]", 0.5),
            "intentional produce failure",
        )
        .fixture("broken_fixture")
        .bind_demand_input(
            "broken_fixture",
            produced_slot("broken_shader", "outputs[0]"),
        )
        .shader("healthy_shader", output("outputs[0]", 0.25))
        .fixture("healthy_fixture")
        .bind_demand_input(
            "healthy_fixture",
            produced_slot("healthy_shader", "outputs[0]"),
        )
        .demand_root("broken_fixture")
        .demand_root("healthy_fixture")
        .build();

    let result = h.tick(16);
    assert!(result.is_err(), "the broken root still fails the frame");
    assert_eq!(
        h.shader_ticks("healthy_shader"),
        1,
        "the healthy root after the broken one was still consumed this frame",
    );
    assert!(
        matches!(h.status("broken_shader"), NodeRuntimeStatus::Fault(_)),
        "the broken producer faulted: {:?}",
        h.status("broken_shader"),
    );
    assert!(
        h.engine.project_fault().is_some(),
        "and the project verdict stands for the next frame's paint",
    );
}
