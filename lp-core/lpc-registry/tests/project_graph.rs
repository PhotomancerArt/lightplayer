mod support;

use lpc_model::{
    NodeDefLocation, NodeDefState, NodeUseLocation, ProjectNodeOrigin, ProjectNodePlacement,
    SlotPath,
};
use lpc_registry::{ParseCtx, ProjectRegistry};
use lpfs::{LpFsMemory, LpPath};

use support::{RegistryScenario, artifact, artifact_asset, root_def};

#[test]
fn fyeah_sign_graph_contains_project_children_playlist_entries_and_asset_consumers() {
    let (scenario, _) = RegistryScenario::load_fixture("fyeah-sign");
    let graph = &scenario.registry().inventory().tree;

    let root = NodeUseLocation::root();
    let playlist = root.child(SlotPath::parse("nodes[playlist]").unwrap());
    let idle = playlist.child(SlotPath::parse("entries[1].node").unwrap());
    let blast = playlist.child(SlotPath::parse("entries[2].node").unwrap());

    assert_eq!(graph.root, root);
    assert_eq!(graph.nodes.len(), 9);
    assert_eq!(graph.nodes[&root].def_location, root_def("/project.json"));
    assert_project_child(
        graph.nodes.get(&playlist).unwrap(),
        "playlist",
        "/playlist.json",
    );
    assert_playlist_entry(
        graph.nodes.get(&idle).unwrap(),
        1,
        Some("idle"),
        "/idle.json",
    );
    assert_playlist_entry(
        graph.nodes.get(&blast).unwrap(),
        2,
        Some("blast"),
        "/blast.json",
    );

    assert_eq!(
        graph
            .asset_consumers
            .get(&artifact_asset("/idle.glsl"))
            .unwrap(),
        &vec![idle]
    );
    assert_eq!(
        graph
            .asset_consumers
            .get(&artifact_asset("/blast.glsl"))
            .unwrap(),
        &vec![blast]
    );
    assert_eq!(
        graph
            .asset_consumers
            .get(&artifact_asset("/fyeah-mapping.svg"))
            .unwrap(),
        &vec![root.child(SlotPath::parse("nodes[fixture]").unwrap())]
    );
}

#[test]
fn duplicate_external_refs_share_def_entry_but_create_distinct_graph_nodes() {
    let (registry, _) = load_inline_project(
        r#"
{
  "kind": "Project",
  "nodes": {
    "a": {
      "ref": "./shader.json"
    },
    "b": {
      "ref": "./shader.json"
    }
  }
}
"#,
        &[(
            "/shader.json",
            r#"
{
  "kind": "Shader",
  "source": {
    "path": "shader.glsl"
  }
}
"#,
        )],
        &[("/shader.glsl", b"void main() {}".as_slice())],
    );
    let graph = &registry.inventory().tree;
    let shader = root_def("/shader.json");
    let a = NodeUseLocation::root().child(SlotPath::parse("nodes[a]").unwrap());
    let b = NodeUseLocation::root().child(SlotPath::parse("nodes[b]").unwrap());

    assert_eq!(registry.inventory().defs.len(), 2);
    assert_eq!(graph.def_instances.get(&shader).unwrap(), &vec![a, b]);
    assert_eq!(
        graph
            .asset_consumers
            .get(&artifact_asset("/shader.glsl"))
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn inline_and_missing_children_are_graph_nodes() {
    let (registry, _) = load_inline_project(
        r#"
{
  "kind": "Project",
  "nodes": {
    "clock": {
      "def": {
        "kind": "Clock"
      }
    },
    "missing": {
      "ref": "./missing.json"
    }
  }
}
"#,
        &[],
        &[],
    );
    let graph = &registry.inventory().tree;
    let inline_clock = NodeDefLocation {
        artifact: artifact("/project.json"),
        path: SlotPath::parse("nodes[clock]").unwrap(),
    };
    let missing = root_def("/missing.json");

    assert!(graph.def_instances.contains_key(&inline_clock));
    assert!(graph.def_instances.contains_key(&missing));
    assert_eq!(
        registry.def(&missing).map(|entry| &entry.state),
        Some(&NodeDefState::NotFound)
    );
}

fn assert_project_child(entry: &lpc_model::ProjectNode, name: &str, expected_def_path: &str) {
    assert_eq!(entry.def_location, root_def(expected_def_path));
    let ProjectNodeOrigin::Invocation { role, .. } = &entry.origin else {
        panic!("expected invocation origin");
    };
    assert_eq!(
        role,
        &ProjectNodePlacement::ProjectChild {
            name: name.to_string()
        }
    );
}

fn assert_playlist_entry(
    entry: &lpc_model::ProjectNode,
    key: u32,
    name: Option<&str>,
    expected_def_path: &str,
) {
    assert_eq!(entry.def_location, root_def(expected_def_path));
    let ProjectNodeOrigin::Invocation { role, .. } = &entry.origin else {
        panic!("expected invocation origin");
    };
    assert_eq!(
        role,
        &ProjectNodePlacement::PlaylistEntry {
            entry: key,
            name: name.map(str::to_string)
        }
    );
}

fn load_inline_project(
    project: &str,
    json_files: &[(&str, &str)],
    byte_files: &[(&str, &[u8])],
) -> (ProjectRegistry, LpFsMemory) {
    let shapes = lpc_model::SlotShapeRegistry::default();
    let ctx = ParseCtx { shapes: &shapes };
    let mut fs = LpFsMemory::new();
    fs.write_file_mut(LpPath::new("/project.json"), project.as_bytes())
        .unwrap();
    for (path, contents) in json_files {
        fs.write_file_mut(LpPath::new(path), contents.as_bytes())
            .unwrap();
    }
    for (path, bytes) in byte_files {
        fs.write_file_mut(LpPath::new(path), bytes).unwrap();
    }

    let mut registry = ProjectRegistry::new();
    registry
        .load_root(
            &fs,
            LpPath::new("/project.json"),
            lpc_model::Revision::new(1),
            &ctx,
        )
        .unwrap();
    (registry, fs)
}
