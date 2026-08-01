//! Scoped-channel correctness obligations (engine C2, modules.md R2/R5).
//!
//! The three properties pinned here are exactly the silent failure modes
//! of rekeying the resolver: (1) cache soundness — the same channel name
//! in two scopes with different writers must produce distinct cached
//! resolutions in ONE session, no collision and no spurious cycle;
//! (2) writer-shadowing — a consumer resolves the nearest enclosing scope
//! with a writer, inheriting outward when its own scope has none;
//! (3) sink no-demand — a probe read with `include_values: true` never
//! resolves (never ticks) a sink-scope producer.
//!
//! Scopes are hand-assigned on the harness tree — loader-side assignment
//! has its own differential coverage in `tests/structural_scope.rs`; the
//! subject here is resolution itself. Harness bus bindings are owned by
//! their producer nodes and consumed bindings by their consumers, so a
//! node's assigned scope is exactly its bindings' scope.

use super::test_support::{EngineTestBuilder, EngineTestHarness, bus, output, produced_slot};
use crate::node::ScopeRef;
use lpc_model::NodeId;
use lpc_wire::{BindingGraphProbeRequest, BindingGraphProbeResult};

/// Mark the root as introducing the root scope and place `members` in it.
fn assign_root_scope(harness: &mut EngineTestHarness, members: &[NodeId]) -> ScopeRef {
    let root = harness.engine.tree().root();
    harness
        .engine
        .tree_mut()
        .get_mut(root)
        .expect("root entry")
        .introduces_scope = true;
    let scope = ScopeRef::Module { owner: root };
    place(harness, members, scope);
    scope
}

/// Introduce a module scope owned by `owner` (which itself inhabits the
/// root scope) and place `members` in it.
fn introduce_module_scope(
    harness: &mut EngineTestHarness,
    owner: NodeId,
    members: &[NodeId],
) -> ScopeRef {
    let root = harness.engine.tree().root();
    {
        let entry = harness
            .engine
            .tree_mut()
            .get_mut(owner)
            .expect("owner entry");
        entry.introduces_scope = true;
        entry.scope = Some(ScopeRef::Module { owner: root });
    }
    let scope = ScopeRef::Module { owner };
    place(harness, members, scope);
    scope
}

fn place(harness: &mut EngineTestHarness, members: &[NodeId], scope: ScopeRef) {
    for member in members {
        harness
            .engine
            .tree_mut()
            .get_mut(*member)
            .expect("member entry")
            .scope = Some(scope);
    }
}

#[test]
fn same_channel_in_two_scopes_resolves_distinctly_in_one_session() {
    // Two sibling module scopes, each with its own writer for the SAME
    // channel name, each with its own reader. One tick = one session = one
    // cache. A scope-blind cache key would hand one scope's value to the
    // other; a scope-blind cycle key would abort resolution entirely.
    let mut h = EngineTestBuilder::new()
        .shader("holder_a", output("outputs[0]", 0.0))
        .shader("holder_b", output("outputs[0]", 0.0))
        .shader("writer_a", output("outputs[0]", 1.0))
        .shader("writer_b", output("outputs[0]", 9.0))
        .bind_bus("chan", produced_slot("writer_a", "outputs[0]"))
        .bind_bus("chan", produced_slot("writer_b", "outputs[0]"))
        .output_node("out_a")
        .bind_demand_input("out_a", bus("chan"))
        .demand_root("out_a")
        .output_node("out_b")
        .bind_demand_input("out_b", bus("chan"))
        .demand_root("out_b")
        .build();

    let holder_a = h.node("holder_a");
    let holder_b = h.node("holder_b");
    let writer_a = h.node("writer_a");
    let writer_b = h.node("writer_b");
    let out_a = h.node("out_a");
    let out_b = h.node("out_b");

    assign_root_scope(&mut h, &[holder_a, holder_b]);
    let scope_a = introduce_module_scope(&mut h, holder_a, &[writer_a, out_a]);
    let scope_b = introduce_module_scope(&mut h, holder_b, &[writer_b, out_b]);
    assert_ne!(scope_a, scope_b);

    h.tick(16).expect("tick");
    assert_eq!(h.output_f32("out_a"), Some(1.0), "scope A reads A's writer");
    assert_eq!(h.output_f32("out_b"), Some(9.0), "scope B reads B's writer");
}

#[test]
fn consumer_inherits_the_nearest_enclosing_writer() {
    // Depth-2 writer-shadowing (the E5 shape, minus the P6 output mirror):
    // the inner scope's writer shadows root's for the inner reader; a
    // reader in a writerless sibling scope inherits root's writer.
    let mut h = EngineTestBuilder::new()
        .shader("holder_a", output("outputs[0]", 0.0))
        .shader("holder_b", output("outputs[0]", 0.0))
        .shader("root_writer", output("outputs[0]", 0.25))
        .shader("inner_writer", output("outputs[0]", 0.75))
        .bind_bus("chan", produced_slot("root_writer", "outputs[0]"))
        .bind_bus("chan", produced_slot("inner_writer", "outputs[0]"))
        .output_node("inner_reader")
        .bind_demand_input("inner_reader", bus("chan"))
        .demand_root("inner_reader")
        .output_node("sibling_reader")
        .bind_demand_input("sibling_reader", bus("chan"))
        .demand_root("sibling_reader")
        .build();

    let holder_a = h.node("holder_a");
    let holder_b = h.node("holder_b");
    let root_writer = h.node("root_writer");
    let inner_writer = h.node("inner_writer");
    let inner_reader = h.node("inner_reader");
    let sibling_reader = h.node("sibling_reader");

    assign_root_scope(&mut h, &[holder_a, holder_b, root_writer]);
    introduce_module_scope(&mut h, holder_a, &[inner_writer, inner_reader]);
    introduce_module_scope(&mut h, holder_b, &[sibling_reader]);

    h.tick(16).expect("tick");
    assert_eq!(
        h.output_f32("inner_reader"),
        Some(0.75),
        "the inner scope's writer shadows root's"
    );
    assert_eq!(
        h.output_f32("sibling_reader"),
        Some(0.25),
        "a writerless scope inherits the nearest enclosing writer"
    );
}

#[test]
fn probe_values_never_tick_sink_scope_producers() {
    // R2 no-demand BY CONSTRUCTION: a probe read with include_values
    // resolves every listed channel from the ROOT scope; a sink-scope
    // publisher must never be selected, so its producer never ticks —
    // the "every Studio refresh renders every inactive playlist entry"
    // failure class, pinned at the resolution layer rather than by a
    // probe-side filter.
    let mut h = EngineTestBuilder::new()
        .shader("playlist_standin", output("outputs[0]", 0.0))
        .shader("root_writer", output("outputs[0]", 0.5))
        .shader("entry_shader", output("outputs[0]", 0.9))
        .bind_bus("visual.out", produced_slot("root_writer", "outputs[0]"))
        .bind_bus("visual.out", produced_slot("entry_shader", "outputs[0]"))
        .output_node("reader")
        .bind_demand_input("reader", bus("visual.out"))
        .demand_root("reader")
        .build();

    let playlist_standin = h.node("playlist_standin");
    let root_writer = h.node("root_writer");
    let entry_shader = h.node("entry_shader");
    let reader = h.node("reader");

    assign_root_scope(&mut h, &[playlist_standin, root_writer, reader]);
    place(
        &mut h,
        &[entry_shader],
        ScopeRef::Sink {
            owner: playlist_standin,
            entry: 1,
        },
    );

    h.tick(16).expect("tick");
    let before = h.shader_ticks("entry_shader");

    let result = h.engine.read_project_binding_graph_probe(
        &h.registry,
        BindingGraphProbeRequest {
            include_values: true,
        },
    );
    let BindingGraphProbeResult::Graph(graph) = result else {
        panic!("expected graph result");
    };
    let channel = graph
        .channels
        .iter()
        .find(|channel| channel.name == "visual.out")
        .expect("channel listed");
    let value = channel.value.as_ref().expect("value requested");
    assert_eq!(value.error, None, "the root-scope read resolves cleanly");

    assert_eq!(
        h.shader_ticks("entry_shader"),
        before,
        "probe demand must never tick a sink-scope producer"
    );
    assert!(
        h.shader_ticks("root_writer") > 0,
        "the root writer is what the probe resolves"
    );
}
