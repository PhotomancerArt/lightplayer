mod support;

use lpc_model::{
    AssetBodyOverlay, AssetChange, AssetChangeKind, MutationOp, NodeDefChange, NodeDefChangeKind,
    NodeDefState, NodeKind, NodeUseChange, NodeUseChangeKind, NodeUseLocation, SlotPath,
};
use support::{RegistryScenario, artifact, artifact_asset, root_def};

#[test]
fn shader_source_file_refresh_reports_one_asset_body_change() {
    let (mut scenario, _) = RegistryScenario::load_fixture("fyeah-sign");

    let changes = scenario.replace_file_and_refresh("/idle.glsl", b"void main() { }");

    assert!(changes.defs.is_empty());
    assert_eq!(
        changes.assets.changed,
        vec![AssetChange::new(
            artifact_asset("/idle.glsl"),
            AssetChangeKind::Body,
        )]
    );
    assert!(changes.assets.added.is_empty());
    assert!(changes.assets.removed.is_empty());
    assert!(changes.uses.is_empty());
}

#[test]
fn unreferenced_file_refresh_does_not_change_effective_project() {
    let (mut scenario, _) = RegistryScenario::load_fixture("fyeah-sign");

    let changes = scenario.replace_file_and_refresh(
        "/not-referenced.json",
        br#"
{
  "kind": "Clock"
}
"#,
    );

    assert!(changes.is_empty());
    assert!(
        scenario
            .registry()
            .def(&root_def("/not-referenced.json"))
            .is_none()
    );
    assert!(
        scenario
            .registry()
            .asset(&artifact_asset("/not-referenced.glsl"))
            .is_none()
    );
}

#[test]
fn changed_registered_def_discovers_newly_referenced_file() {
    let mut scenario = RegistryScenario::empty();
    scenario.write_file(
        "/project.json",
        br#"
{
  "kind": "Project",
  "format": 1
}
"#,
    );
    scenario.write_file(
        "/clock.json",
        br#"
{
  "kind": "Clock"
}
"#,
    );
    scenario.load_root("/project.json");

    let changes = scenario.replace_file_and_refresh(
        "/project.json",
        br#"
{
  "kind": "Project",
  "format": 1,
  "nodes": {
    "clock": {
      "ref": "./clock.json"
    }
  }
}
"#,
    );

    assert_eq!(changes.defs.added, vec![root_def("/clock.json")]);
    assert_eq!(
        changes.defs.changed,
        vec![NodeDefChange::new(
            root_def("/project.json"),
            NodeDefChangeKind::Body,
        )]
    );
    assert_eq!(
        changes.uses.added,
        vec![NodeUseLocation::root().child(SlotPath::parse("nodes[clock]").unwrap())]
    );
}

#[test]
fn missing_referenced_def_recovers_when_file_is_created() {
    let mut scenario = RegistryScenario::empty();
    scenario.write_file(
        "/project.json",
        br#"
{
  "kind": "Project",
  "format": 1,
  "nodes": {
    "clock": {
      "ref": "./clock.json"
    }
  }
}
"#,
    );
    scenario.load_root("/project.json");

    assert_eq!(
        scenario
            .registry()
            .def(&root_def("/clock.json"))
            .map(|entry| &entry.state),
        Some(&NodeDefState::NotFound)
    );

    let changes = scenario.replace_file_and_refresh(
        "/clock.json",
        br#"
{
  "kind": "Clock"
}
"#,
    );

    assert_eq!(
        changes.defs.changed,
        vec![NodeDefChange::new(
            root_def("/clock.json"),
            NodeDefChangeKind::LeftError,
        )]
    );
    assert!(changes.uses.is_empty());
    assert!(matches!(
        scenario
            .registry()
            .def(&root_def("/clock.json"))
            .map(|entry| &entry.state),
        Some(NodeDefState::Loaded(_))
    ));
}

#[test]
fn changing_shader_def_kind_removes_its_referenced_source_asset() {
    let (mut scenario, _) = RegistryScenario::load_fixture("fyeah-sign");

    let result = scenario.apply(MutationOp::SetArtifactBody {
        artifact: artifact("/idle.json"),
        edit: AssetBodyOverlay::ReplaceBody(
            br#"{
  "kind": "Clock"
}"#
            .to_vec(),
        ),
    });

    assert_eq!(
        result.changes.defs.changed,
        vec![NodeDefChange::new(
            root_def("/idle.json"),
            NodeDefChangeKind::KindChanged {
                from: NodeKind::Shader,
                to: NodeKind::Clock,
            },
        )]
    );
    assert_eq!(
        result.changes.assets.removed,
        vec![artifact_asset("/idle.glsl")]
    );
    assert!(result.changes.assets.changed.is_empty());
    assert!(result.changes.uses.is_empty());
}

#[test]
fn deleting_referenced_fixture_svg_reports_asset_entered_error() {
    let (mut scenario, _) = RegistryScenario::load_fixture("fyeah-sign");

    let changes = scenario.delete_file_and_refresh("/fyeah-mapping.svg");

    assert!(changes.defs.is_empty());
    assert_eq!(
        changes.assets.changed,
        vec![AssetChange::new(
            artifact_asset("/fyeah-mapping.svg"),
            AssetChangeKind::EnteredError,
        )]
    );
    assert!(changes.uses.is_empty());
}

#[test]
fn removing_playlist_reference_removes_child_def_and_its_asset() {
    let (mut scenario, _) = RegistryScenario::load_fixture("fyeah-sign");

    let changes = scenario.replace_file_and_refresh(
        "/playlist.json",
        br#"
{
  "kind": "Playlist",
  "idle_entry": 1,
  "default_fade": 0.35,
  "bindings": {
    "time": {
      "source": "bus:time"
    }
  },
  "entries": {
    "1": {
      "name": "idle",
      "fade_after": 0.12,
      "node": {
        "ref": "./idle.json"
      }
    }
  }
}
"#,
    );

    assert_eq!(changes.defs.removed, vec![root_def("/blast.json")]);
    assert_eq!(
        changes.defs.changed,
        vec![NodeDefChange::new(
            root_def("/playlist.json"),
            NodeDefChangeKind::Body,
        )]
    );
    assert_eq!(changes.assets.removed, vec![artifact_asset("/blast.glsl")]);
    assert!(changes.assets.changed.is_empty());
    assert_eq!(
        changes.uses.removed,
        vec![
            NodeUseLocation::root()
                .child(SlotPath::parse("nodes[playlist]").unwrap())
                .child(SlotPath::parse("entries[2].node").unwrap())
        ]
    );
    assert!(changes.uses.changed.is_empty());
}

#[test]
fn changing_project_child_ref_reports_node_use_definition_change() {
    let (mut scenario, _) = RegistryScenario::load_fixture("fyeah-sign");

    let changes = scenario.replace_file_and_refresh(
        "/project.json",
        br#"
{
  "kind": "Project",
  "format": 1,
  "name": "fyeah-sign",
  "nodes": {
    "output": {
      "ref": "./output.json"
    },
    "clock": {
      "ref": "./idle.json"
    },
    "button": {
      "ref": "./button.json"
    },
    "radio": {
      "ref": "./radio.json"
    },
    "playlist": {
      "ref": "./playlist.json"
    },
    "fixture": {
      "ref": "./fixture.json"
    }
  }
}
"#,
    );

    assert_eq!(
        changes.uses.changed,
        vec![NodeUseChange::new(
            NodeUseLocation::root().child(SlotPath::parse("nodes[clock]").unwrap()),
            NodeUseChangeKind::DefinitionChanged {
                from: root_def("/clock.json"),
                to: root_def("/idle.json"),
            },
        )]
    );
}

#[test]
fn same_kind_body_value_edit_does_not_report_node_use_change() {
    let (mut scenario, _) = RegistryScenario::load_fixture("fyeah-sign");

    let changes = scenario.replace_file_and_refresh(
        "/output.json",
        br#"
{
  "kind": "Output",
  "endpoint": "ws281x:rmt:D10",
  "bindings": {
    "input": {
      "source": "bus:control.out"
    }
  },
  "options": {
    "white_point": [
      0.9,
      1.0,
      1.0
    ],
    "brightness": 0.25,
    "interpolation_enabled": true,
    "dithering_enabled": false,
    "lut_enabled": true
  }
}
"#,
    );

    assert_eq!(
        changes.defs.changed,
        vec![NodeDefChange::new(
            root_def("/output.json"),
            NodeDefChangeKind::Body,
        )]
    );
    assert!(changes.uses.is_empty());
}
