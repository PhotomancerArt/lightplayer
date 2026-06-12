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
        "/not-referenced.toml",
        br#"
kind = "Clock"
"#,
    );

    assert!(changes.is_empty());
    assert!(
        scenario
            .registry()
            .def(&root_def("/not-referenced.toml"))
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
        "/project.toml",
        br#"
kind = "Project"
"#,
    );
    scenario.write_file(
        "/clock.toml",
        br#"
kind = "Clock"
"#,
    );
    scenario.load_root("/project.toml");

    let changes = scenario.replace_file_and_refresh(
        "/project.toml",
        br#"
kind = "Project"

[nodes.clock]
ref = "./clock.toml"
"#,
    );

    assert_eq!(changes.defs.added, vec![root_def("/clock.toml")]);
    assert_eq!(
        changes.defs.changed,
        vec![NodeDefChange::new(
            root_def("/project.toml"),
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
        "/project.toml",
        br#"
kind = "Project"

[nodes.clock]
ref = "./clock.toml"
"#,
    );
    scenario.load_root("/project.toml");

    assert_eq!(
        scenario
            .registry()
            .def(&root_def("/clock.toml"))
            .map(|entry| &entry.state),
        Some(&NodeDefState::NotFound)
    );

    let changes = scenario.replace_file_and_refresh(
        "/clock.toml",
        br#"
kind = "Clock"
"#,
    );

    assert_eq!(
        changes.defs.changed,
        vec![NodeDefChange::new(
            root_def("/clock.toml"),
            NodeDefChangeKind::LeftError,
        )]
    );
    assert!(changes.uses.is_empty());
    assert!(matches!(
        scenario
            .registry()
            .def(&root_def("/clock.toml"))
            .map(|entry| &entry.state),
        Some(NodeDefState::Loaded(_))
    ));
}

#[test]
fn changing_shader_def_kind_removes_its_referenced_source_asset() {
    let (mut scenario, _) = RegistryScenario::load_fixture("fyeah-sign");

    let result = scenario.apply(MutationOp::SetArtifactBody {
        artifact: artifact("/idle.toml"),
        edit: AssetBodyOverlay::ReplaceBody(br#"kind = "Clock""#.to_vec()),
    });

    assert_eq!(
        result.changes.defs.changed,
        vec![NodeDefChange::new(
            root_def("/idle.toml"),
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
        "/playlist.toml",
        br#"
kind = "Playlist"
idle_entry = 1
default_fade = 0.35

[bindings.time]
source = "bus#time.seconds"

[entries.1]
name = "idle"
fade_after = 0.12
node = { ref = "./idle.toml" }
"#,
    );

    assert_eq!(changes.defs.removed, vec![root_def("/blast.toml")]);
    assert_eq!(
        changes.defs.changed,
        vec![NodeDefChange::new(
            root_def("/playlist.toml"),
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
        "/project.toml",
        br#"
kind = "Project"
name = "fyeah-sign"

[nodes.output]
ref = "./output.toml"

[nodes.clock]
ref = "./idle.toml"

[nodes.button]
ref = "./button.toml"

[nodes.radio]
ref = "./radio.toml"

[nodes.playlist]
ref = "./playlist.toml"

[nodes.fixture]
ref = "./fixture.toml"
"#,
    );

    assert_eq!(
        changes.uses.changed,
        vec![NodeUseChange::new(
            NodeUseLocation::root().child(SlotPath::parse("nodes[clock]").unwrap()),
            NodeUseChangeKind::DefinitionChanged {
                from: root_def("/clock.toml"),
                to: root_def("/idle.toml"),
            },
        )]
    );
}

#[test]
fn same_kind_body_value_edit_does_not_report_node_use_change() {
    let (mut scenario, _) = RegistryScenario::load_fixture("fyeah-sign");

    let changes = scenario.replace_file_and_refresh(
        "/output.toml",
        br#"
kind = "Output"
endpoint = "ws281x:rmt:D10"

[bindings.input]
source = "bus#control.out"

[options]
white_point = [0.9, 1.0, 1.0]
brightness = 0.25
interpolation_enabled = true
dithering_enabled = false
lut_enabled = true
"#,
    );

    assert_eq!(
        changes.defs.changed,
        vec![NodeDefChange::new(
            root_def("/output.toml"),
            NodeDefChangeKind::Body,
        )]
    );
    assert!(changes.uses.is_empty());
}
