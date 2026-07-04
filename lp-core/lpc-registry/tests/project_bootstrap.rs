mod support;

use lpc_model::{AssetContentType, NodeKind};
use lpfs::{LpFs, LpPath};
use support::{
    RegistryScenario, TestProject, assert_artifact_asset_content_types, assert_loaded_def_kinds,
};

#[test]
fn can_create_fyeah_sign_project_from_empty_fs_with_artifact_body_mutations() {
    let fixture = TestProject::load("fyeah-sign");
    let mut scenario = RegistryScenario::empty();

    let apply = scenario.apply_batch(fixture.replace_body_batch());
    assert_eq!(apply.commands.results.len(), fixture.file_count());
    assert!(apply.changes.is_empty());

    let commit = scenario.commit();
    assert_eq!(commit.artifact_changes.added.len(), fixture.file_count());
    assert!(
        scenario
            .fs()
            .file_exists(LpPath::new("/project.json"))
            .unwrap()
    );

    let load = scenario.load_root("/project.json");
    assert_eq!(load.changes.defs.added.len(), 9);
    assert_eq!(load.changes.assets.added.len(), 3);

    assert_loaded_def_kinds(
        scenario.registry(),
        &[
            ("/project.json", NodeKind::Project),
            ("/blast.json", NodeKind::Shader),
            ("/button.json", NodeKind::Button),
            ("/clock.json", NodeKind::Clock),
            ("/fixture.json", NodeKind::Fixture),
            ("/idle.json", NodeKind::Shader),
            ("/output.json", NodeKind::Output),
            ("/playlist.json", NodeKind::Playlist),
            ("/radio.json", NodeKind::ControlRadio),
        ],
    );
    assert_artifact_asset_content_types(
        scenario.registry(),
        &[
            ("/blast.glsl", AssetContentType::ShaderSource),
            ("/fyeah-mapping.svg", AssetContentType::FixtureSvg),
            ("/idle.glsl", AssetContentType::ShaderSource),
        ],
    );
}
