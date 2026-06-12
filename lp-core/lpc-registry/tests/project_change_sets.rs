mod support;

use lpc_model::{
    AssetChange, AssetChangeKind, AssetOverlay, MutationOp, NodeDefChange, NodeDefChangeKind,
    NodeKind,
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
}

#[test]
fn changing_shader_def_kind_removes_its_referenced_source_asset() {
    let (mut scenario, _) = RegistryScenario::load_fixture("fyeah-sign");

    let result = scenario.apply(MutationOp::SetArtifactBody {
        artifact: artifact("/idle.toml"),
        edit: AssetOverlay::ReplaceBody(br#"kind = "Clock""#.to_vec()),
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
}
