//! Structural scope (engine C1, modules.md R1/R2): scope identity lives on
//! `RuntimeNodeEntry`, is queryable after load AND after apply, survives
//! reattach and broken defs, and models playlist-entry sink scopes as a
//! property — never a probe filter.

use lpc_engine::engine::LoadedProjectRuntime;
use lpc_engine::node::ScopeRef;
use lpc_engine::{EngineServices, ProjectLoader};
use lpc_model::{NodeUseLocation, SlotPath, TreePath, current_revision};
use lpc_registry::ParseCtx;
use lpfs::{AsLpPath, FsEvent, FsEventKind, LpFs, LpFsMemory, LpPathBuf};

fn project_fs() -> LpFsMemory {
    let fs = LpFsMemory::new();
    fs.write_file("/project.json".as_path(), b"{\n  \"format\": 3\n}\n")
        .expect("container manifest");
    fs.write_file(
        "/module.json".as_path(),
        br#"
{
  "kind": "Module",
  "nodes": {
    "clock": {
      "ref": "./clock.json"
    },
    "list": {
      "ref": "./playlist.json"
    }
  }
}
"#,
    )
    .expect("module.json");
    fs.write_file(
        "/playlist.json".as_path(),
        br#"
{
  "kind": "Playlist",
  "entries": {
    "1": {
      "name": "idle",
      "node": {
        "ref": "./idle.json"
      }
    },
    "7": {
      "name": "active",
      "node": {
        "ref": "./active.json"
      }
    }
  }
}
"#,
    )
    .expect("playlist.json");
    fs.write_file("/clock.json".as_path(), br#"{ "kind": "Clock" }"#)
        .expect("clock.json");
    fs.write_file(
        "/idle.json".as_path(),
        br#"{ "kind": "Shader", "source": { "path": "idle.glsl" } }"#,
    )
    .expect("idle.json");
    fs.write_file(
        "/active.json".as_path(),
        br#"{ "kind": "Shader", "source": { "path": "active.glsl" } }"#,
    )
    .expect("active.json");
    fs.write_file(
        "/idle.glsl".as_path(),
        b"vec4 render(vec2 p) { return vec4(1.0); }",
    )
    .expect("idle.glsl");
    fs.write_file(
        "/active.glsl".as_path(),
        b"vec4 render(vec2 p) { return vec4(0.5); }",
    )
    .expect("active.glsl");
    fs
}

fn load(fs: &LpFsMemory) -> LoadedProjectRuntime {
    let services = EngineServices::new(TreePath::parse("/scope_test.show").expect("path"));
    ProjectLoader::load_from_root(fs, services).expect("load scope project")
}

fn use_location(path: &str) -> NodeUseLocation {
    NodeUseLocation::root().child(SlotPath::parse(path).expect("slot path"))
}

/// `(persist-path, is_sink)` per scope-carrying node, keyed by a stable
/// label — the comparable "scope table" for differential assertions.
fn scope_table(engine: &lpc_engine::Engine) -> Vec<(String, Option<String>, bool)> {
    let mut rows = Vec::new();
    for entry in engine.tree().entries() {
        let scope = entry.scope;
        let persist = scope.and_then(|scope| engine.tree().scope_persist_path(scope));
        rows.push((
            entry.path.to_string(),
            persist,
            scope.is_some_and(|scope| scope.is_sink()),
        ));
    }
    rows.sort();
    rows
}

#[test]
fn scope_is_queryable_after_load_with_sink_entries_modeled() {
    let fs = project_fs();
    let rt = load(&fs);
    let engine = rt.engine();
    let tree = engine.tree();
    let root = tree.root();

    // Root: no containing scope, introduces the root scope.
    assert_eq!(tree.scope_of(root), None);
    let root_scope = tree.scope_introduced_by(root).expect("root introduces");
    assert_eq!(root_scope, ScopeRef::Module { owner: root });
    assert!(!root_scope.is_sink());

    // Project children inhabit the root module's scope.
    let clock = engine
        .project_runtime_index()
        .node_id(&use_location("nodes[clock]"))
        .expect("clock projected");
    let list = engine
        .project_runtime_index()
        .node_id(&use_location("nodes[list]"))
        .expect("playlist projected");
    assert_eq!(tree.scope_of(clock), Some(root_scope));
    assert_eq!(tree.scope_of(list), Some(root_scope));
    assert_eq!(tree.scope_introduced_by(clock), None);
    assert_eq!(
        tree.scope_introduced_by(list),
        None,
        "playlists introduce per-entry sinks, not a module scope"
    );

    // Playlist entries inhabit per-entry sink scopes keyed by the
    // authored entry key.
    let entries = tree
        .scopes()
        .into_iter()
        .filter(ScopeRef::is_sink)
        .collect::<Vec<_>>();
    assert_eq!(
        entries,
        vec![
            ScopeRef::Sink {
                owner: list,
                entry: 1
            },
            ScopeRef::Sink {
                owner: list,
                entry: 7
            },
        ]
    );
    for scope in &entries {
        let path = engine
            .tree()
            .scope_persist_path(*scope)
            .expect("sink scope persist path");
        assert!(
            path.ends_with(&format!(
                "/entries[{}]",
                match scope {
                    ScopeRef::Sink { entry, .. } => *entry,
                    ScopeRef::Module { .. } => unreachable!(),
                }
            )),
            "sink scopes key by authored entry, got {path}"
        );
    }
}

#[test]
fn load_and_trivial_apply_produce_identical_scope_tables() {
    // The critical invariant: scope is recomputed identically on BOTH
    // entry points — a project must never wear different scopes after an
    // edit than after a reload.
    let fs = project_fs();
    let baseline = scope_table(load(&fs).engine());

    let fs = project_fs();
    let rt = load(&fs);
    let (mut engine, mut registry) = rt.into_parts();
    // Trivial content change: touch the clock def body.
    fs.write_file(
        "/clock.json".as_path(),
        br#"{ "kind": "Clock", "controls": { "rate": 2.0 } }"#,
    )
    .expect("rewrite clock");
    let shapes = engine.slot_shapes().clone();
    let changes = registry.refresh_artifacts(
        &fs,
        &[FsEvent {
            path: LpPathBuf::from("/clock.json"),
            kind: FsEventKind::Modify,
        }],
        current_revision(),
        &ParseCtx { shapes: &shapes },
    );
    engine
        .apply_project_changes(&fs, &mut registry, &changes)
        .expect("apply");

    let applied = scope_table(&engine);
    assert_eq!(applied, baseline, "load vs load+apply scope tables differ");
}

#[test]
fn load_and_apply_produce_identical_bus_wiring() {
    // Extends the scope-table differential to RESOLVED WIRING: for every
    // (scope, channel) pair, the winning provider set must be identical
    // through fresh load and through load + apply.
    fn winner_table(engine: &lpc_engine::Engine) -> Vec<(String, String, Vec<String>)> {
        let tree = engine.tree();
        let mut rows = Vec::new();
        for scope in tree.scopes() {
            let scope_path = tree.scope_persist_path(scope).expect("scope path");
            for (channel, _) in tree.scope_channels(scope) {
                let winners = tree
                    .providers_for_bus_read(Some(scope), &channel)
                    .into_iter()
                    .map(|(_, entry)| {
                        tree.get(entry.owner)
                            .map(|owner| owner.path.to_string())
                            .unwrap_or_default()
                    })
                    .collect::<Vec<_>>();
                rows.push((scope_path.clone(), channel.0.clone(), winners));
            }
        }
        rows.sort();
        rows
    }

    let fs = project_fs();
    let baseline = winner_table(load(&fs).engine());
    assert!(
        !baseline.is_empty(),
        "the wiring table must actually cover channels"
    );

    let fs = project_fs();
    let rt = load(&fs);
    let (mut engine, mut registry) = rt.into_parts();
    fs.write_file(
        "/clock.json".as_path(),
        br#"{ "kind": "Clock", "controls": { "rate": 2.0 } }"#,
    )
    .expect("rewrite clock");
    let shapes = engine.slot_shapes().clone();
    let changes = registry.refresh_artifacts(
        &fs,
        &[FsEvent {
            path: LpPathBuf::from("/clock.json"),
            kind: FsEventKind::Modify,
        }],
        current_revision(),
        &ParseCtx { shapes: &shapes },
    );
    engine
        .apply_project_changes(&fs, &mut registry, &changes)
        .expect("apply");
    assert_eq!(
        winner_table(&engine),
        baseline,
        "load vs load+apply bus wiring differs"
    );
}

#[test]
fn failed_defs_still_carry_scope() {
    // R1: the engine must always answer — a node whose def is broken still
    // inhabits its structural scope.
    let fs = project_fs();
    fs.write_file("/clock.json".as_path(), b"not valid json {{{")
        .expect("break clock def");
    let rt = load(&fs);
    let engine = rt.engine();
    let tree = engine.tree();
    let root_scope = tree
        .scope_introduced_by(tree.root())
        .expect("root introduces");
    let clock = engine
        .project_runtime_index()
        .node_id(&use_location("nodes[clock]"))
        .expect("broken clock still projected");
    assert_eq!(tree.scope_of(clock), Some(root_scope));
}

#[test]
fn scope_persist_paths_are_tree_path_stable() {
    let fs = project_fs();
    let rt = load(&fs);
    let engine = rt.engine();
    let tree = engine.tree();
    let root_scope = tree.scope_introduced_by(tree.root()).expect("root scope");
    let root_path = tree
        .scope_persist_path(root_scope)
        .expect("root persist path");
    assert!(
        root_path.starts_with("/scope_test."),
        "module scopes key by the owner's tree path, got {root_path}"
    );
}

#[test]
fn e5_depth_2_consumer_resolves_the_sibling_modules_publish() {
    // modules.md E5, the exact shape that must be pinned: H contains
    // M_outer; M_outer contains M_inner (which has a visual writer) and a
    // consumer C beside it. C's `visual.out` read finds M_inner's R7
    // publish IN Scope(M_outer) — module publishes count as writers like
    // any producer — and never walks to root. The failure mode being
    // pinned: writer accounting that omits module publishes works at
    // depth 1 by coincidence and resolves C to ROOT's visual at depth 2.
    let fs = LpFsMemory::new();
    fs.write_file("/project.json".as_path(), b"{\n  \"format\": 3\n}\n")
        .expect("container manifest");
    fs.write_file(
        "/module.json".as_path(),
        br#"
{
  "kind": "Module",
  "nodes": {
    "root_shader": { "ref": "./root_shader.json" },
    "outer": { "ref": "./outer/module.json" }
  }
}
"#,
    )
    .expect("module.json");
    fs.write_file(
        "/root_shader.json".as_path(),
        br#"{ "kind": "Shader", "source": { "path": "root.glsl" } }"#,
    )
    .expect("root shader");
    fs.write_file(
        "/root.glsl".as_path(),
        b"vec4 render(vec2 p) { return vec4(1.0); }",
    )
    .expect("root glsl");
    fs.write_file(
        "/outer/module.json".as_path(),
        br#"
{
  "kind": "Module",
  "nodes": {
    "inner": { "ref": "./inner/module.json" },
    "analyzer": { "ref": "./analyzer.json" }
  }
}
"#,
    )
    .expect("outer module");
    fs.write_file(
        "/outer/inner/module.json".as_path(),
        br#"
{
  "kind": "Module",
  "nodes": {
    "plasma": { "ref": "./plasma.json" }
  }
}
"#,
    )
    .expect("inner module");
    fs.write_file(
        "/outer/inner/plasma.json".as_path(),
        br#"{ "kind": "Shader", "source": { "path": "plasma.glsl" } }"#,
    )
    .expect("plasma shader");
    fs.write_file(
        "/outer/inner/plasma.glsl".as_path(),
        b"vec4 render(vec2 p) { return vec4(0.5); }",
    )
    .expect("plasma glsl");
    fs.write_file(
        "/outer/analyzer.json".as_path(),
        br#"
{
  "kind": "Texture",
  "size": { "width": 2, "height": 2 },
  "bindings": {
    "input": { "source": "bus:visual.out" }
  }
}
"#,
    )
    .expect("analyzer");

    let rt = load(&fs);
    let engine = rt.engine();
    let tree = engine.tree();

    let outer = engine
        .project_runtime_index()
        .node_id(&use_location("nodes[outer]"))
        .expect("outer module projected");
    let inner = engine
        .project_runtime_index()
        .node_id(
            &use_location("nodes[outer]").child(SlotPath::parse("nodes[inner]").expect("path")),
        )
        .expect("inner module projected");
    let analyzer = engine
        .project_runtime_index()
        .node_id(
            &use_location("nodes[outer]").child(SlotPath::parse("nodes[analyzer]").expect("path")),
        )
        .expect("analyzer projected");

    // The analyzer reads from Scope(outer); the winning provider set for
    // that read must be M_inner's R7 publish — not root's shader.
    let scope_outer = tree.scope_of(analyzer).expect("analyzer scope");
    assert_eq!(
        scope_outer,
        lpc_engine::node::ScopeRef::Module { owner: outer }
    );
    let winners = tree.providers_for_bus_read(
        Some(scope_outer),
        &lpc_model::ChannelName(String::from("visual.out")),
    );
    assert!(
        !winners.is_empty(),
        "Scope(outer) must see the inner module's publish as a writer"
    );
    assert!(
        winners.iter().all(|(_, entry)| entry.owner == inner),
        "the depth-2 read must resolve to the sibling module's publish, \
         never walk to root: {winners:?}"
    );
}

#[test]
fn r7_authored_export_and_root_module_runtime() {
    // R7: an authored export republishes an inner channel outward under
    // the export's name; the root wears a real module runtime (its output
    // interface exists like any module's).
    let fs = LpFsMemory::new();
    fs.write_file("/project.json".as_path(), b"{\n  \"format\": 3\n}\n")
        .expect("container manifest");
    fs.write_file(
        "/module.json".as_path(),
        br#"
{
  "kind": "Module",
  "nodes": {
    "clock": { "ref": "./clock.json" },
    "audio": { "ref": "./audio/module.json" }
  }
}
"#,
    )
    .expect("module.json");
    fs.write_file("/clock.json".as_path(), br#"{ "kind": "Clock" }"#)
        .expect("clock");
    fs.write_file(
        "/audio/module.json".as_path(),
        br#"
{
  "kind": "Module",
  "nodes": {
    "beat_clock": { "ref": "./beat_clock.json" }
  },
  "exports": {
    "energy": "bus:time"
  }
}
"#,
    )
    .expect("audio module");
    fs.write_file(
        "/audio/beat_clock.json".as_path(),
        br#"{ "kind": "Clock" }"#,
    )
    .expect("beat clock");

    let rt = load(&fs);
    let engine = rt.engine();
    let tree = engine.tree();
    let audio = engine
        .project_runtime_index()
        .node_id(&use_location("nodes[audio]"))
        .expect("audio module projected");

    // The export surfaces as a writer of `energy` in the module's own
    // nearest scope (root), owned by the module node.
    let root_scope = tree.node_scope(tree.root()).expect("root scope");
    let winners = tree.providers_for_bus_read(
        Some(root_scope),
        &lpc_model::ChannelName(String::from("energy")),
    );
    assert!(
        winners.iter().any(|(_, entry)| entry.owner == audio),
        "the authored export must appear as a writer in the containing scope"
    );

    // The embedded module also default-publishes visual.out outward (R7
    // automatic publish), and the ROOT is alive with a real module
    // runtime exposing the mirror's `output` state row.
    let winners = tree.providers_for_bus_read(
        Some(root_scope),
        &lpc_model::ChannelName(String::from("visual.out")),
    );
    assert!(
        winners.iter().any(|(_, entry)| entry.owner == audio),
        "an embedded module default-publishes visual.out at fallback priority"
    );
    let root_entry = tree.get(tree.root()).expect("root entry");
    assert!(
        matches!(
            root_entry.state.value(),
            lpc_engine::node::NodeEntryState::Alive(_)
        ),
        "the root wears a live module runtime"
    );
}

#[test]
fn panel_writer_survives_apply_project_changes() {
    // The side-store's reason to exist: apply_project_changes rebuilds
    // ALL bindings from defs (clear + re-register), and an engaged panel
    // writer must ride through untouched — still engaged, still winning.
    let fs = project_fs();
    let rt = load(&fs);
    let (mut engine, mut registry) = rt.into_parts();
    let root_scope = engine
        .tree()
        .node_scope(engine.tree().root())
        .expect("root scope");
    let channel = lpc_model::ChannelName(String::from("time"));
    engine.panel_write(
        root_scope,
        channel.clone(),
        lpc_model::LpValue::F32(42.0),
        None,
    );

    fs.write_file(
        "/clock.json".as_path(),
        br#"{ "kind": "Clock", "controls": { "rate": 2.0 } }"#,
    )
    .expect("rewrite clock");
    let shapes = engine.slot_shapes().clone();
    let changes = registry.refresh_artifacts(
        &fs,
        &[FsEvent {
            path: LpPathBuf::from("/clock.json"),
            kind: FsEventKind::Modify,
        }],
        current_revision(),
        &ParseCtx { shapes: &shapes },
    );
    engine
        .apply_project_changes(&fs, &mut registry, &changes)
        .expect("apply");

    assert!(
        engine.panel_writers().get(root_scope, &channel).is_some(),
        "the engaged writer survives the binding rebuild"
    );
    let winners = engine
        .tree()
        .providers_for_bus_in_scope(root_scope, &channel);
    // The authored clock target still exists in the tree…
    assert!(!winners.is_empty());
    // …but the probe's value read (the resolve path) still answers with
    // the engaged value.
    let result = engine.read_project_binding_graph_probe(
        &registry,
        lpc_wire::BindingGraphProbeRequest {
            include_values: true,
        },
    );
    let lpc_wire::BindingGraphProbeResult::Graph(graph) = result else {
        panic!("expected graph result");
    };
    let row = graph
        .channels
        .iter()
        .find(|row| row.name == "time" && !row.scope.is_none())
        .expect("scoped time row");
    assert_eq!(
        row.value.as_ref().and_then(|value| value.value.clone()),
        Some(lpc_model::LpValue::F32(42.0)),
        "the panel value still wins after apply"
    );
}
