//! What must still change when resolution stops being recomputed per frame.
//!
//! The resolver caches resolution across frames and drops that knowledge only
//! when [`Resolver::invalidate_structure`] says the graph changed shape. Every
//! test here names one way the graph can change and asserts the engine still
//! reports the new answer. They are written against observable values rather
//! than cache internals, so they pin behaviour that must hold whether or not
//! anything is cached at all — they were green before the caching existed, and
//! staying green is the point.
//!
//! The counter tests are the exception: they assert *how* a frame reached its
//! answer, which is the only way to catch resolution silently going back to
//! re-deriving the graph every tick.

use super::test_support::{EngineTestBuilder, bus, literal, output, produced_slot};
use crate::dataflow::binding::{BindingDraft, BindingPriority, BindingSource, BindingTarget};
use lpc_model::{Kind, LpValue, Revision, SlotPath};

fn path(p: &str) -> SlotPath {
    SlotPath::parse(p).expect("test slot path")
}

/// Re-binding a consumed slot mid-run must change what it resolves to.
///
/// This is the shape of the studio bind gesture and of every project apply:
/// bindings are load-time materializations, so a re-bind *replaces* the
/// binding set (`clear_bindings` then re-register from defs) rather than
/// adding a competing binding. Consumed slots resolve by owner depth and
/// registration order — priority arbitrates bus providers, not these — so
/// replacement is the only way a consumed slot's source changes.
#[test]
fn rebinding_a_consumed_slot_mid_run_changes_its_value() {
    let mut harness = EngineTestBuilder::new()
        .output_node("out")
        .bind_demand_input("out", literal(1.0))
        .demand_root("out")
        .build();

    harness.tick(16).expect("first tick");
    assert_eq!(harness.output_f32("out"), Some(1.0));

    let out = harness.node("out");
    harness.engine.clear_bindings(Revision::new(2));
    harness
        .engine
        .add_binding(
            BindingDraft {
                source: BindingSource::Literal(LpValue::F32(9.0)),
                target: BindingTarget::ConsumedSlot {
                    node: out,
                    slot: path("in"),
                },
                priority: BindingPriority::new(0),
                kind: Kind::Color,
                owner: out,
            },
            Revision::new(2),
        )
        .expect("re-register binding");

    harness.tick(16).expect("second tick");
    assert_eq!(
        harness.output_f32("out"),
        Some(9.0),
        "a re-bound slot must not keep resolving through the binding it no longer has"
    );
}

/// A higher-priority bus provider added mid-run must take the channel over.
///
/// Priority genuinely arbitrates here (unlike consumed slots), so this is the
/// one path where an *added* binding changes an existing answer.
#[test]
fn higher_priority_bus_provider_added_mid_run_wins_next_tick() {
    let mut harness = EngineTestBuilder::new()
        .output_node("out")
        .bind_bus("chan", literal(1.0))
        .bind_demand_input("out", bus("chan"))
        .demand_root("out")
        .build();

    harness.tick(16).expect("first tick");
    assert_eq!(harness.output_f32("out"), Some(1.0));

    let out = harness.node("out");
    harness
        .engine
        .add_binding(
            BindingDraft {
                source: BindingSource::Literal(LpValue::F32(9.0)),
                target: BindingTarget::BusChannel(lpc_model::ChannelName(
                    alloc::string::String::from("chan"),
                )),
                priority: BindingPriority::new(10),
                kind: Kind::Color,
                owner: out,
            },
            Revision::new(2),
        )
        .expect("add higher-priority provider");

    harness.tick(16).expect("second tick");
    assert_eq!(
        harness.output_f32("out"),
        Some(9.0),
        "a higher-priority provider must beat the cached resolution of the channel"
    );
}

/// Replacing a producer node must change what its consumers see.
#[test]
fn reattached_producer_node_supplies_the_new_value() {
    let mut harness = EngineTestBuilder::new()
        .shader("src", output("outputs[0]", 2.0))
        .output_node("out")
        .bind_demand_input("out", produced_slot("src", "outputs[0]"))
        .demand_root("out")
        .build();

    harness.tick(16).expect("first tick");
    assert_eq!(harness.output_f32("out"), Some(2.0));

    let src = harness.node("src");
    let replacement = super::test_support::dummy_shader_node(path("outputs[0]"), 7.0);
    harness
        .engine
        .reattach_runtime_node(src, replacement, Revision::new(3))
        .expect("reattach producer");

    harness.tick(16).expect("second tick");
    assert_eq!(
        harness.output_f32("out"),
        Some(7.0),
        "a replaced producer must not keep serving the old node's cached production"
    );
}

/// A node that demands a different producer than last frame — the playlist's
/// switch — must get the newly demanded one, not the previously cached one.
#[test]
fn selector_switching_targets_resolves_the_newly_demanded_producer() {
    let mut harness = EngineTestBuilder::new()
        .shader("a", output("outputs[0]", 3.0))
        .shader("b", output("outputs[0]", 4.0))
        .selector("sel", &[("a", "outputs[0]"), ("b", "outputs[0]")])
        .demand_root("sel")
        .build();

    harness.tick(16).expect("first tick");
    assert_eq!(harness.output_f32("sel"), Some(3.0));

    // Runtime state only: no binding moves, no node is added or removed.
    harness.select("sel", 1);
    harness.tick(16).expect("second tick");
    assert_eq!(
        harness.output_f32("sel"),
        Some(4.0),
        "switching which producer is demanded is not a structural change, and must not need one"
    );

    harness.select("sel", 0);
    harness.tick(16).expect("third tick");
    assert_eq!(harness.output_f32("sel"), Some(3.0), "and back again");
}

/// Producers keep ticking every frame even when the graph is unchanged.
///
/// Persisting *resolution* must not persist *values*: a shader still runs, and
/// its output still carries the current frame's revision.
#[test]
fn producers_still_tick_every_frame_on_an_unchanged_graph() {
    let mut harness = EngineTestBuilder::new()
        .shader("src", output("outputs[0]", 5.0))
        .output_node("out")
        .bind_demand_input("out", produced_slot("src", "outputs[0]"))
        .demand_root("out")
        .build();

    harness.tick(16).expect("first tick");
    harness.reset_shader_ticks("src");

    for _ in 0..3 {
        harness.tick(16).expect("steady tick");
    }

    assert_eq!(
        harness.shader_ticks("src"),
        3,
        "a cached route must still run the producer behind it, once per frame"
    );
}

/// Steady-state frames over an unchanged graph do the same amount of work.
///
/// This is the weaker, always-true form of the zero-structural-work assertion:
/// whatever a frame costs, it must not grow frame over frame.
#[test]
fn steady_state_frames_report_stable_counters() {
    let mut harness = EngineTestBuilder::new()
        .shader("src", output("outputs[0]", 1.5))
        .output_node("out")
        .bind_demand_input("out", produced_slot("src", "outputs[0]"))
        .demand_root("out")
        .build();

    harness.tick(16).expect("warm-up tick");
    harness.tick(16).expect("second tick");
    let second = *harness.engine.resolver().last_frame_counters();
    harness.tick(16).expect("third tick");
    let third = *harness.engine.resolver().last_frame_counters();

    assert_eq!(
        second, third,
        "two steady frames over an unchanged graph must cost the same resolution work"
    );
}

/// Structural invalidation is observable and monotonic, so that a future
/// mutation site that forgets to call it can be caught by a test rather than
/// by a stale value in the field.
#[test]
fn structural_mutations_bump_the_epoch() {
    let mut harness = EngineTestBuilder::new()
        .shader("src", output("outputs[0]", 1.0))
        .output_node("out")
        .bind_demand_input("out", produced_slot("src", "outputs[0]"))
        .demand_root("out")
        .build();

    harness.tick(16).expect("tick");
    let before_tick = harness.engine.resolver().structure_epoch();
    harness.tick(16).expect("another tick");
    assert_eq!(
        harness.engine.resolver().structure_epoch(),
        before_tick,
        "ticking is not a structural change"
    );

    let out = harness.node("out");
    harness
        .engine
        .add_binding(
            BindingDraft {
                source: BindingSource::Literal(LpValue::F32(2.0)),
                target: BindingTarget::ConsumedSlot {
                    node: out,
                    slot: path("other"),
                },
                priority: BindingPriority::new(1),
                kind: Kind::Color,
                owner: out,
            },
            Revision::new(4),
        )
        .expect("add binding");
    assert!(
        harness.engine.resolver().structure_epoch() > before_tick,
        "adding a binding is a structural change"
    );

    let epoch_after_binding = harness.engine.resolver().structure_epoch();
    let src = harness.node("src");
    harness
        .engine
        .reattach_runtime_node(
            src,
            super::test_support::dummy_shader_node(path("outputs[0]"), 1.0),
            Revision::new(5),
        )
        .expect("reattach");
    assert!(
        harness.engine.resolver().structure_epoch() > epoch_after_binding,
        "reattaching a node is a structural change"
    );
}
