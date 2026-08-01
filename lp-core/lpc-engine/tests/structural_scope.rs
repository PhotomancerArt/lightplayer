//! Structural scope (engine C1, modules.md R1/R2): scope identity lives on
//! `RuntimeNodeEntry`, is queryable after load AND after apply, survives
//! reattach and broken defs, and models playlist-entry sink scopes as a
//! property — never a probe filter.

use lpc_engine::node::ScopeRef;
use lpc_engine::engine::LoadedProjectRuntime;
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
    fs.write_file("/idle.glsl".as_path(), b"vec4 render(vec2 p) { return vec4(1.0); }")
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
            path.ends_with(&format!("/entries[{}]", match scope {
                ScopeRef::Sink { entry, .. } => *entry,
                ScopeRef::Module { .. } => unreachable!(),
            })),
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
