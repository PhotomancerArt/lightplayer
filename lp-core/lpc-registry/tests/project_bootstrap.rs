mod support;

use lpc_model::{AssetKind, NodeKind};
use lpfs::{LpFs, LpPath};
use support::{
    RegistryScenario, TestProject, assert_artifact_asset_kinds, assert_loaded_def_kinds,
};

#[test]
fn can_create_fyeah_sign_project_from_empty_fs_with_artifact_body_mutations() {
    let fixture = TestProject::load("fyeah-sign");
    let mut scenario = RegistryScenario::empty();

    let apply = scenario.apply_batch(fixture.replace_body_batch());
    assert_eq!(apply.commands.results.len(), fixture.file_count());
    assert!(apply.changes.is_empty());

    let commit = scenario.commit();
    assert_eq!(commit.artifacts.added.len(), fixture.file_count());
    assert!(commit.changes.is_empty());
    assert!(
        scenario
            .fs()
            .file_exists(LpPath::new("/project.toml"))
            .unwrap()
    );

    let load = scenario.load_root("/project.toml");
    assert_eq!(load.changes.defs.added.len(), 9);
    assert_eq!(load.changes.assets.added.len(), 3);

    assert_loaded_def_kinds(
        scenario.registry(),
        &[
            ("/project.toml", NodeKind::Project),
            ("/blast.toml", NodeKind::Shader),
            ("/button.toml", NodeKind::Button),
            ("/clock.toml", NodeKind::Clock),
            ("/fixture.toml", NodeKind::Fixture),
            ("/idle.toml", NodeKind::Shader),
            ("/output.toml", NodeKind::Output),
            ("/playlist.toml", NodeKind::Playlist),
            ("/radio.toml", NodeKind::ControlRadio),
        ],
    );
    assert_artifact_asset_kinds(
        scenario.registry(),
        &[
            ("/blast.glsl", AssetKind::ShaderSource),
            ("/fyeah-mapping.svg", AssetKind::FixtureSvg),
            ("/idle.glsl", AssetKind::ShaderSource),
        ],
    );
}
