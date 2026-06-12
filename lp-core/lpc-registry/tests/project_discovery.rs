mod support;

use lpc_model::{AssetContentType, NodeKind};

use support::{RegistryScenario, assert_artifact_asset_content_types, assert_loaded_def_kinds};

#[test]
fn fyeah_sign_discovers_referenced_node_defs_and_assets() {
    let (scenario, load) = RegistryScenario::load_fixture("fyeah-sign");
    let registry = scenario.registry();

    assert_eq!(registry.root(), Some(&support::root_def("/project.toml")));
    assert!(load.changes.defs.changed.is_empty());
    assert!(load.changes.defs.removed.is_empty());
    assert!(load.changes.assets.changed.is_empty());
    assert!(load.changes.assets.removed.is_empty());

    assert_loaded_def_kinds(
        registry,
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

    assert_artifact_asset_content_types(
        registry,
        &[
            ("/blast.glsl", AssetContentType::ShaderSource),
            ("/fyeah-mapping.svg", AssetContentType::FixtureSvg),
            ("/idle.glsl", AssetContentType::ShaderSource),
        ],
    );

    assert_eq!(load.changes.defs.added.len(), 9);
    assert_eq!(load.changes.assets.added.len(), 3);
}
