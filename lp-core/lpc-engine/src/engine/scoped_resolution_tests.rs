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

    // Wire 8: the sink scope's own row DOES list (panel liveness for entry
    // children) — but its value is withheld, because resolving it would
    // have rendered the entry. Same no-demand property, kept at the
    // value-request seam instead of by omitting the row.
    let sink_row = graph
        .channels
        .iter()
        .find(|channel| {
            channel.name == "visual.out"
                && channel
                    .scope
                    .as_ref()
                    .is_some_and(lpc_wire::WireScopeRef::is_sink)
        })
        .expect("the sink scope's channel row lists");
    assert!(
        sink_row.value.is_none(),
        "a sink-producer-backed value is never resolved by a probe"
    );
}

#[test]
fn sink_channel_rows_carry_panel_writer_values_without_demand() {
    // The §4.1 shape at the engine seam: a playlist entry's consumed-only
    // channel lists as a sink row, an engaged panel writer surfaces on it
    // as a Panel-origin provider with a resolvable (literal) value, and
    // none of it ever ticks the entry's producer.
    let mut h = EngineTestBuilder::new()
        .shader("playlist_standin", output("outputs[0]", 0.0))
        .shader("entry_shader", output("outputs[0]", 0.9))
        .bind_bus("visual.out", produced_slot("entry_shader", "outputs[0]"))
        .bind_input("entry_shader", "glow", bus("glow"))
        .build();

    let playlist_standin = h.node("playlist_standin");
    let entry_shader = h.node("entry_shader");
    assign_root_scope(&mut h, &[playlist_standin]);
    let sink = ScopeRef::Sink {
        owner: playlist_standin,
        entry: 1,
    };
    place(&mut h, &[entry_shader], sink);
    h.tick(16).expect("tick");
    let before = h.shader_ticks("entry_shader");

    let channel = lpc_model::ChannelName(alloc::string::String::from("glow"));
    h.engine
        .panel_write(sink, channel, lpc_model::LpValue::F32(0.7), None);

    let result = h.engine.read_project_binding_graph_probe(
        &h.registry,
        BindingGraphProbeRequest {
            include_values: true,
        },
    );
    let BindingGraphProbeResult::Graph(graph) = result else {
        panic!("expected graph result");
    };
    let glow = graph
        .channels
        .iter()
        .find(|channel| channel.name == "glow")
        .expect("the sink-consumed channel lists");
    assert!(
        glow.scope.as_ref().is_some_and(lpc_wire::WireScopeRef::is_sink),
        "glow lists in the entry's sink scope: {:?}",
        glow.scope
    );
    assert_eq!(
        glow.value.as_ref().and_then(|value| value.value.clone()),
        Some(lpc_model::LpValue::F32(0.7)),
        "the engaged writer's literal resolves — no producer demand needed"
    );
    assert!(
        glow.providers.iter().any(|index| {
            graph
                .bindings
                .get(*index as usize)
                .is_some_and(|binding| binding.origin == lpc_wire::WireBindingOrigin::Panel)
        }),
        "the writer surfaces as a Panel-origin provider row"
    );
    assert_eq!(
        h.shader_ticks("entry_shader"),
        before,
        "none of this ticked the entry's producer"
    );
}

#[test]
fn probe_lists_same_named_channels_as_distinct_scoped_rows() {
    // Wire 6: channels list per scope, structured — two scopes using the
    // same channel name are distinct rows keyed by (scope, name), and the
    // probe never flattens scope into a display string.
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
    introduce_module_scope(&mut h, holder_a, &[writer_a, out_a]);
    introduce_module_scope(&mut h, holder_b, &[writer_b, out_b]);

    h.tick(16).expect("tick");
    let result = h.engine.read_project_binding_graph_probe(
        &h.registry,
        BindingGraphProbeRequest {
            include_values: true,
        },
    );
    let BindingGraphProbeResult::Graph(graph) = result else {
        panic!("expected graph result");
    };
    let rows: alloc::vec::Vec<_> = graph
        .channels
        .iter()
        .filter(|channel| channel.name == "chan")
        .collect();
    assert_eq!(rows.len(), 2, "one row per scope: {rows:?}");
    let scopes: alloc::vec::Vec<_> = rows.iter().map(|row| row.scope).collect();
    assert!(
        scopes.contains(&Some(lpc_wire::WireScopeRef::Module { owner: holder_a }))
            && scopes.contains(&Some(lpc_wire::WireScopeRef::Module { owner: holder_b })),
        "rows carry structured scopes: {scopes:?}"
    );
    for row in rows {
        let value = row.value.as_ref().expect("value requested");
        let expected = if row.scope == Some(lpc_wire::WireScopeRef::Module { owner: holder_a }) {
            1.0
        } else {
            9.0
        };
        assert_eq!(
            value.value,
            Some(lpc_model::LpValue::F32(expected)),
            "each row resolves in ITS scope"
        );
    }
}

#[test]
fn panel_writer_engages_shadows_and_clears() {
    // panel.md P1-P4 at the resolution seam: engage -> the in-scope
    // consumer resolves the panel value while an outer-scope consumer is
    // unaffected; an engaged OUTER scope is inherited by a writerless
    // inner scope (P5 shadowing composes); clear -> authored wiring again.
    let mut h = EngineTestBuilder::new()
        .shader("holder", output("outputs[0]", 0.0))
        .shader("root_writer", output("outputs[0]", 0.25))
        .bind_bus("chan", produced_slot("root_writer", "outputs[0]"))
        .output_node("root_reader")
        .bind_demand_input("root_reader", bus("chan"))
        .demand_root("root_reader")
        .output_node("inner_reader")
        .bind_demand_input("inner_reader", bus("chan"))
        .demand_root("inner_reader")
        .build();

    let holder = h.node("holder");
    let root_writer = h.node("root_writer");
    let root_reader = h.node("root_reader");
    let inner_reader = h.node("inner_reader");
    let root_scope = assign_root_scope(&mut h, &[root_writer, root_reader]);
    let inner_scope = introduce_module_scope(&mut h, holder, &[inner_reader]);

    // Untouched: authored wiring; the writerless inner scope inherits it.
    h.tick(16).expect("tick");
    assert_eq!(h.output_f32("root_reader"), Some(0.25));
    assert_eq!(h.output_f32("inner_reader"), Some(0.25));

    // Engage in the INNER scope: its reader detaches; root unaffected.
    h.engine.panel_write(
        inner_scope,
        lpc_model::ChannelName(alloc::string::String::from("chan")),
        lpc_model::LpValue::F32(0.9),
        None,
    );
    h.tick(16).expect("tick");
    assert_eq!(
        h.output_f32("inner_reader"),
        Some(0.9),
        "engaged scope reads its panel writer"
    );
    assert_eq!(
        h.output_f32("root_reader"),
        Some(0.25),
        "outer scope unaffected by an inner engage"
    );

    // Engage ROOT instead: the writerless inner scope inherits the panel
    // writer through the same shadowing walk.
    assert!(h.engine.panel_clear(
        inner_scope,
        &lpc_model::ChannelName(alloc::string::String::from("chan"))
    ));
    h.engine.panel_write(
        root_scope,
        lpc_model::ChannelName(alloc::string::String::from("chan")),
        lpc_model::LpValue::F32(0.6),
        None,
    );
    h.tick(16).expect("tick");
    assert_eq!(h.output_f32("root_reader"), Some(0.6));
    assert_eq!(
        h.output_f32("inner_reader"),
        Some(0.6),
        "a writerless scope inherits the enclosing panel writer"
    );

    // Clear: authored wiring returns everywhere.
    assert!(h.engine.panel_clear(
        root_scope,
        &lpc_model::ChannelName(alloc::string::String::from("chan"))
    ));
    h.tick(16).expect("tick");
    assert_eq!(h.output_f32("root_reader"), Some(0.25));
    assert_eq!(h.output_f32("inner_reader"), Some(0.25));
}

#[test]
fn engaged_panel_writer_replaces_the_scopes_provider_set() {
    // The settled ByKey decision: an engaged panel writer REPLACES the
    // scope's provider set (max priority wins) — pinned at the host seam
    // both merge expansion and single-select read, so map-kinded (ByKey)
    // channels shadow instead of merging the panel value into authored
    // providers.
    let mut h = EngineTestBuilder::new()
        .shader("writer_a", output("outputs[0]", 0.25))
        .shader("writer_b", output("outputs[0]", 0.75))
        .bind_bus("chan", produced_slot("writer_a", "outputs[0]"))
        .bind_bus("chan", produced_slot("writer_b", "outputs[0]"))
        .output_node("reader")
        .bind_demand_input("reader", bus("chan"))
        .demand_root("reader")
        .build();

    let writer_a = h.node("writer_a");
    let writer_b = h.node("writer_b");
    let reader = h.node("reader");
    let root_scope = assign_root_scope(&mut h, &[writer_a, writer_b, reader]);

    h.engine.panel_write(
        root_scope,
        lpc_model::ChannelName(alloc::string::String::from("chan")),
        lpc_model::LpValue::F32(0.5),
        None,
    );
    h.tick(16).expect("tick");
    // Two authored providers would be AMBIGUOUS (equal priority) — the
    // panel writer replacing the set is what makes this resolve at all.
    assert_eq!(
        h.output_f32("reader"),
        Some(0.5),
        "the engaged writer replaces the provider set outright"
    );
}

#[test]
fn probe_reports_engaged_writers_with_panel_origin() {
    // The probe host (the site that is easy to miss) must see the panel
    // overlay: the engaged value resolves in the probe's value read AND
    // surfaces as a Panel-origin provider row for the UI's engaged state.
    let mut h = EngineTestBuilder::new()
        .shader("writer", output("outputs[0]", 0.25))
        .bind_bus("chan", produced_slot("writer", "outputs[0]"))
        .output_node("reader")
        .bind_demand_input("reader", bus("chan"))
        .demand_root("reader")
        .build();
    let writer = h.node("writer");
    let reader = h.node("reader");
    let root_scope = assign_root_scope(&mut h, &[writer, reader]);
    h.engine.panel_write(
        root_scope,
        lpc_model::ChannelName(alloc::string::String::from("chan")),
        lpc_model::LpValue::F32(0.9),
        None,
    );
    h.tick(16).expect("tick");

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
        .find(|channel| channel.name == "chan")
        .expect("channel listed");
    let value = channel.value.as_ref().expect("value requested");
    assert_eq!(
        value.value,
        Some(lpc_model::LpValue::F32(0.9)),
        "the probe's value read sees the panel overlay"
    );
    let first = &graph.bindings[channel.providers[0] as usize];
    assert_eq!(
        first.origin,
        lpc_wire::WireBindingOrigin::Panel,
        "the engaged writer leads the provider list with Panel origin"
    );
}

#[test]
fn momentary_writers_despawn_on_ttl_expiry() {
    // panel.md P14: gesture channels write while active and despawn past
    // their renewal deadline — the despawn IS the release fallback for a
    // dropped client. Renewal is just another write.
    let mut h = EngineTestBuilder::new()
        .shader("writer", output("outputs[0]", 0.25))
        .bind_bus("chan", produced_slot("writer", "outputs[0]"))
        .output_node("reader")
        .bind_demand_input("reader", bus("chan"))
        .demand_root("reader")
        .build();
    let writer = h.node("writer");
    let reader = h.node("reader");
    let root_scope = assign_root_scope(&mut h, &[writer, reader]);
    let channel = lpc_model::ChannelName(alloc::string::String::from("chan"));

    h.engine.panel_write(
        root_scope,
        channel.clone(),
        lpc_model::LpValue::F32(0.9),
        Some(40),
    );
    h.tick(16).expect("tick");
    assert_eq!(
        h.output_f32("reader"),
        Some(0.9),
        "gesture writes while live"
    );

    // Renewal keeps it alive past the original deadline…
    h.tick(16).expect("tick");
    h.engine.panel_write(
        root_scope,
        channel.clone(),
        lpc_model::LpValue::F32(0.8),
        Some(40),
    );
    h.tick(16).expect("tick");
    assert_eq!(h.output_f32("reader"), Some(0.8));

    // …and silence despawns it: the authored writer returns.
    h.tick(60).expect("tick");
    assert!(
        h.engine.panel_writers().get(root_scope, &channel).is_none(),
        "expired gesture despawns"
    );
    h.tick(16).expect("tick");
    assert_eq!(
        h.output_f32("reader"),
        Some(0.25),
        "despawn IS the fallback — authored wiring returns"
    );
}

#[test]
fn clear_all_reaches_sink_scopes() {
    // Settled P-Q4: clear-all clears EVERYTHING, including a playlist
    // entry's sink-scope latched value.
    let mut h = EngineTestBuilder::new()
        .shader("standin", output("outputs[0]", 0.0))
        .shader("writer", output("outputs[0]", 0.25))
        .bind_bus("chan", produced_slot("writer", "outputs[0]"))
        .output_node("reader")
        .bind_demand_input("reader", bus("chan"))
        .demand_root("reader")
        .build();
    let standin = h.node("standin");
    let writer = h.node("writer");
    let reader = h.node("reader");
    let root_scope = assign_root_scope(&mut h, &[standin, writer, reader]);
    let sink = ScopeRef::Sink {
        owner: standin,
        entry: 3,
    };
    let channel = lpc_model::ChannelName(alloc::string::String::from("chan"));
    h.engine.panel_write(
        root_scope,
        channel.clone(),
        lpc_model::LpValue::F32(0.9),
        None,
    );
    h.engine
        .panel_write(sink, channel.clone(), lpc_model::LpValue::F32(0.7), None);
    assert_eq!(h.engine.panel_writers().len(), 2);

    assert_eq!(
        h.engine.panel_clear_all(),
        2,
        "sink-scope writers clear too"
    );
    assert!(h.engine.panel_writers().is_empty());
}
