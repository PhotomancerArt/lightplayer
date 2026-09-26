//! The pre-tick residency step (`Engine::apply_residency`): one playlist
//! entry's subtree loads and unloads on the owner's request, with the tree,
//! the bindings and the resolver consistent after every switch, a failed
//! load rolled back, and knob values kept across dormancy.
//!
//! Fixture and stand-in owner: `entry_residency_support`.

mod entry_residency_support;

use entry_residency_support::{
    BROKEN, IDLE, Owner, OwnerEvent, PATTERN, entry_child, load, loaded_entries, playlist_id,
    playlist_use, project_fs,
};
use lpc_engine::EntryResidencyEvent;
use lpc_engine::dataflow::binding::BindingSource;
use lpc_engine::node::{NodeEntryState, ResidencyRequest, ScopeRef};
use lpc_model::{ChannelName, LpValue, MutationOp, NodeId, SlotEdit, SlotPath, current_revision};
use lpc_registry::ParseCtx;
use lpfs::{AsLpPath, FsEvent, FsEventKind, LpFs, LpPathBuf};

#[test]
fn a_one_two_one_cycle_holds_exactly_one_entry_and_rebinds_each_time() {
    let fs = project_fs();
    let mut rt = load(&fs);
    let owner = Owner::install(rt.engine_mut());
    assert_eq!(
        loaded_entries(rt.engine()),
        vec![IDLE],
        "idle entry at load"
    );
    let first_idle = entry_child(rt.engine(), IDLE).expect("idle child");

    let mut previous_ids = subtree(rt.engine(), first_idle);
    for (from, to) in [(IDLE, PATTERN), (PATTERN, IDLE)] {
        let epoch = rt.engine().resolver().structure_epoch();
        owner.request(ResidencyRequest::switch(from, to));
        rt.tick_with_residency(&fs, 16).expect("tick");

        let child = entry_child(rt.engine(), to).expect("loaded child");
        assert_eq!(
            owner.take_log(),
            vec![OwnerEvent::Unloaded(from), OwnerEvent::Loaded(to, child)],
            "{from} → {to}: the owner hears the unload, then the load"
        );
        assert_eq!(
            loaded_entries(rt.engine()),
            vec![to],
            "{from} → {to}: exactly one entry subtree"
        );
        assert!(
            rt.engine().resolver().structure_epoch() > epoch,
            "{from} → {to}: the resolver was invalidated"
        );
        for id in &previous_ids {
            assert!(
                rt.engine().tree().get(*id).is_none(),
                "{from} → {to}: old id {id:?} is gone"
            );
        }
        assert_bindings_are_consistent(rt.engine());
        assert!(
            rt.registry()
                .residency()
                .is_resident(&playlist_use(), to, IDLE)
                && !rt
                    .registry()
                    .residency()
                    .is_resident(&playlist_use(), from, IDLE),
            "{from} → {to}: the registry agrees"
        );
        previous_ids = subtree(rt.engine(), child);
    }

    // The pattern's shader consumed `bus:speed` while it was loaded; the
    // idle shader does not, so no binding of the pattern's survives.
    assert!(
        !rt.engine().tree().bindings().any(|binding| matches!(
            &binding.source,
            BindingSource::BusChannel(channel) if channel.0 == "speed"
        )),
        "the pattern's consumed binding left with it"
    );
    let fresh_idle = entry_child(rt.engine(), IDLE).expect("idle back");
    assert_ne!(fresh_idle, first_idle, "ids are never reused");
}

#[test]
fn the_pattern_binds_its_consumed_channel_when_it_loads() {
    let fs = project_fs();
    let mut rt = load(&fs);
    let owner = Owner::install(rt.engine_mut());
    owner.request(ResidencyRequest::switch(IDLE, PATTERN));
    rt.tick_with_residency(&fs, 16).expect("tick");

    let pattern = entry_child(rt.engine(), PATTERN).expect("pattern loaded");
    let shader = shader_in(rt.engine(), pattern);
    assert!(
        matches!(
            rt.engine()
                .tree()
                .get(shader)
                .map(|node| node.state.value()),
            Some(NodeEntryState::Alive(_))
        ),
        "the pattern's shader is alive before the tick that demands it"
    );
    assert!(
        rt.engine()
            .tree()
            .bindings()
            .any(|binding| binding.owner == shader
                && matches!(&binding.source, BindingSource::BusChannel(c) if c.0 == "speed")),
        "the pattern's shader consumes bus:speed"
    );
}

#[test]
fn a_broken_entry_reports_load_failed_the_tick_succeeds_and_nothing_is_attached() {
    let fs = project_fs();
    let mut rt = load(&fs);
    let owner = Owner::install(rt.engine_mut());
    let nodes_before = rt.engine().tree().len();
    let idle_nodes = subtree(rt.engine(), entry_child(rt.engine(), IDLE).unwrap()).len();

    owner.request(ResidencyRequest::switch(IDLE, BROKEN));
    let applied = rt
        .apply_residency(&fs)
        .expect("the step never fails on a load");
    rt.tick(16).expect("the tick succeeds");

    let log = owner.take_log();
    assert_eq!(log[0], OwnerEvent::Unloaded(IDLE));
    let OwnerEvent::LoadFailed(entry, reason) = &log[1] else {
        panic!("expected a load failure, got {log:?}");
    };
    assert_eq!(*entry, BROKEN);
    assert!(
        reason.contains("missing.json"),
        "the reason names the file: {reason}"
    );
    assert!(matches!(
        applied.events.as_slice(),
        [
            EntryResidencyEvent::Unloaded { entry: IDLE, .. },
            EntryResidencyEvent::LoadFailed { entry: BROKEN, .. }
        ]
    ));

    assert!(
        loaded_entries(rt.engine()).is_empty(),
        "nothing is half-attached"
    );
    assert_eq!(
        rt.engine().tree().len(),
        nodes_before - idle_nodes,
        "the tree lost the idle entry and gained nothing"
    );
    assert!(
        !rt.registry()
            .residency()
            .is_resident(&playlist_use(), BROKEN, IDLE),
        "the registry rolled the broken entry back out"
    );
    assert!(
        rt.engine()
            .tree()
            .entries()
            .all(|node| !matches!(node.state.value(), NodeEntryState::Failed { .. })),
        "no failed node is left behind"
    );
    assert_bindings_are_consistent(rt.engine());

    // The owner can move on: the next load works.
    owner.request(ResidencyRequest::load(PATTERN));
    rt.tick_with_residency(&fs, 16).expect("tick");
    assert_eq!(loaded_entries(rt.engine()), vec![PATTERN]);
}

#[test]
fn a_refused_unload_changes_nothing_and_tells_the_owner() {
    let fs = project_fs();
    let mut rt = load(&fs);
    let owner = Owner::install(rt.engine_mut());
    let idle = entry_child(rt.engine(), IDLE).expect("idle child");
    // An edit a commit would write, on the playing entry.
    let shapes = rt.engine().slot_shapes().clone();
    rt.registry_mut()
        .mutate(
            &fs,
            MutationOp::PutSlotEdit {
                artifact: lpc_model::ArtifactLocation::file("/idle.json"),
                edit: SlotEdit::assign_value(
                    SlotPath::parse("render_order").unwrap(),
                    LpValue::I32(3),
                ),
            },
            current_revision(),
            &ParseCtx { shapes: &shapes },
        )
        .expect("stage an edit");

    owner.request(ResidencyRequest::switch(IDLE, PATTERN));
    rt.tick_with_residency(&fs, 16)
        .expect("a refusal never fails the tick");

    let log = owner.take_log();
    let [OwnerEvent::Refused(request, reason)] = log.as_slice() else {
        panic!("expected one refusal, got {log:?}");
    };
    assert_eq!(*request, ResidencyRequest::switch(IDLE, PATTERN));
    assert!(reason.contains("pending edits"), "{reason}");
    assert_eq!(
        loaded_entries(rt.engine()),
        vec![IDLE],
        "still playing idle"
    );
    assert_eq!(entry_child(rt.engine(), IDLE), Some(idle), "the same node");
}

#[test]
fn knob_values_survive_unload_and_reload() {
    let fs = project_fs();
    let mut rt = load(&fs);
    let owner = Owner::install(rt.engine_mut());
    owner.request(ResidencyRequest::switch(IDLE, PATTERN));
    rt.tick_with_residency(&fs, 16).expect("tick");
    let playlist = playlist_id(rt.engine());
    let pattern = entry_child(rt.engine(), PATTERN).expect("pattern loaded");
    let shader = shader_in(rt.engine(), pattern);

    // Where the pattern's own knob lands: its shader reads `bus:speed` from
    // the scope it inhabits, which is the pattern MODULE's scope — keyed by
    // the module's runtime id — not the entry's sink.
    let knob_scope = rt.engine().tree().bus_read_scope(shader).expect("scope");
    assert_eq!(knob_scope, ScopeRef::Module { owner: pattern });
    let speed = ChannelName(String::from("speed"));
    let sink = ScopeRef::Sink {
        owner: playlist,
        entry: PATTERN,
    };
    rt.engine_mut()
        .panel_write(knob_scope, speed.clone(), LpValue::F32(0.25), None);
    rt.engine_mut()
        .panel_write(sink, speed.clone(), LpValue::F32(0.75), None);

    owner.request(ResidencyRequest::switch(PATTERN, IDLE));
    rt.tick_with_residency(&fs, 16).expect("tick");
    assert_eq!(
        rt.engine()
            .panel_writers()
            .get(sink, &speed)
            .map(|w| &w.value),
        Some(&LpValue::F32(0.75)),
        "the sink writer stays: its owner is the playlist"
    );
    assert_eq!(
        rt.engine().panel_writers().parked().count(),
        1,
        "the module-scope writer is parked by persist path while dormant"
    );

    owner.request(ResidencyRequest::switch(IDLE, PATTERN));
    rt.tick_with_residency(&fs, 16).expect("tick");
    let reloaded = entry_child(rt.engine(), PATTERN).expect("pattern back");
    assert_ne!(reloaded, pattern, "a reload is a new node");
    let new_scope = ScopeRef::Module { owner: reloaded };
    assert_eq!(
        rt.engine()
            .panel_writers()
            .get(new_scope, &speed)
            .map(|w| &w.value),
        Some(&LpValue::F32(0.25)),
        "the module knob came back under the new module id"
    );
    assert_eq!(rt.engine().panel_writers().parked().count(), 0);
    assert_eq!(
        rt.engine()
            .panel_writers()
            .get(sink, &speed)
            .map(|w| &w.value),
        Some(&LpValue::F32(0.75))
    );
}

#[test]
fn edits_after_switches_apply_and_never_resurrect_dormant_entries() {
    let fs = project_fs();
    let mut rt = load(&fs);
    let owner = Owner::install(rt.engine_mut());
    for request in [
        ResidencyRequest::switch(IDLE, PATTERN),
        ResidencyRequest::switch(PATTERN, IDLE),
        ResidencyRequest::switch(IDLE, PATTERN),
    ] {
        owner.request(request);
        rt.tick_with_residency(&fs, 16).expect("tick");
    }
    let pattern = entry_child(rt.engine(), PATTERN).expect("pattern loaded");

    // An edit to the resident entry: its GLSL changes on disk.
    fs.write_file(
        "/pulse/shader.glsl".as_path(),
        b"layout(binding = 1) uniform float phase;\nvec4 render_2d(vec2 p) { return vec4(phase); }",
    )
    .expect("rewrite glsl");
    let applied = refresh(&mut rt, &fs, "/pulse/shader.glsl");
    assert!(
        !applied.refreshed_nodes.is_empty(),
        "the resident entry's shader refreshed from the edit"
    );
    assert_eq!(loaded_entries(rt.engine()), vec![PATTERN]);
    assert_eq!(entry_child(rt.engine(), PATTERN), Some(pattern));

    // An edit to another node: nothing dormant comes back.
    fs.write_file(
        "/clock.json".as_path(),
        br#"{ "kind": "Clock", "transport": { "rate": 2.0 } }"#,
    )
    .expect("rewrite clock");
    refresh(&mut rt, &fs, "/clock.json");
    assert_eq!(loaded_entries(rt.engine()), vec![PATTERN]);

    // An edit to the playlist itself: still nothing dormant comes back.
    fs.write_file(
        "/playlist.json".as_path(),
        br#"{
  "kind": "Playlist",
  "idle_entry": 1,
  "default_fade": 0.5,
  "entries": {
    "1": { "name": "idle", "node": { "ref": "./idle.json" } },
    "2": { "name": "pulse", "node": { "ref": "./pulse/module.json" } },
    "3": { "name": "broken", "node": { "ref": "./missing.json" } }
  }
}"#,
    )
    .expect("rewrite playlist");
    refresh(&mut rt, &fs, "/playlist.json");
    assert_eq!(loaded_entries(rt.engine()), vec![PATTERN]);
    assert_bindings_are_consistent(rt.engine());
}

#[test]
fn a_tick_with_no_request_changes_nothing() {
    let fs = project_fs();
    let mut rt = load(&fs);
    Owner::install(rt.engine_mut());
    let epoch = rt.engine().resolver().structure_epoch();
    let applied = rt.apply_residency(&fs).expect("no-op step");
    assert!(applied.is_empty());
    assert_eq!(rt.engine().resolver().structure_epoch(), epoch);
}

// --- helpers ---

/// Every node in `root`'s subtree, root first.
fn subtree(engine: &lpc_engine::Engine, root: NodeId) -> Vec<NodeId> {
    let mut ids = vec![root];
    let mut next = 0;
    while next < ids.len() {
        let id = ids[next];
        next += 1;
        if let Some(node) = engine.tree().get(id) {
            ids.extend(node.children.value().iter().copied());
        }
    }
    ids
}

/// The shader node inside the pattern module.
fn shader_in(engine: &lpc_engine::Engine, pattern: NodeId) -> NodeId {
    subtree(engine, pattern)
        .into_iter()
        .find(|id| {
            engine
                .tree()
                .get(*id)
                .is_some_and(|node| node.path.to_string().ends_with(".shader"))
        })
        .expect("the pattern holds a shader")
}

/// Every binding belongs to a node that is in the tree, and every binding
/// that names a node names one that is in the tree.
fn assert_bindings_are_consistent(engine: &lpc_engine::Engine) {
    for binding in engine.tree().bindings() {
        assert!(
            engine.tree().get(binding.owner).is_some(),
            "binding owned by a removed node: {binding:?}"
        );
        if let BindingSource::ProducedSlot { node, .. } = &binding.source {
            assert!(
                engine.tree().get(*node).is_some(),
                "binding reads a removed node: {binding:?}"
            );
        }
        if let lpc_engine::dataflow::binding::BindingTarget::ConsumedSlot { node, .. } =
            &binding.target
        {
            assert!(
                engine.tree().get(*node).is_some(),
                "binding writes a removed node: {binding:?}"
            );
        }
    }
}

fn refresh(
    rt: &mut lpc_engine::engine::LoadedProjectRuntime,
    fs: &lpfs::LpFsMemory,
    path: &str,
) -> lpc_engine::RuntimeApplyResult {
    let shapes = rt.engine().slot_shapes().clone();
    let (engine, registry) = rt.split_mut();
    let changes = registry.refresh_artifacts(
        fs,
        &[FsEvent {
            path: LpPathBuf::from(path),
            kind: FsEventKind::Modify,
        }],
        current_revision(),
        &ParseCtx { shapes: &shapes },
    );
    engine
        .apply_project_changes(fs, registry, &changes)
        .expect("apply")
}
