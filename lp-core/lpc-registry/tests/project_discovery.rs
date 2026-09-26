mod support;

use lpc_model::{AssetContentType, NodeKind};

use support::{RegistryScenario, assert_artifact_asset_content_types, assert_loaded_def_kinds};

#[test]
fn fyeah_sign_discovers_referenced_node_defs_and_assets() {
    let (mut scenario, load) = RegistryScenario::load_fixture("fyeah-sign");

    assert_eq!(
        scenario.registry().root(),
        Some(&support::root_def("/module.json"))
    );
    assert!(load.changes.defs.changed.is_empty());
    assert!(load.changes.defs.removed.is_empty());
    assert!(load.changes.assets.changed.is_empty());
    assert!(load.changes.assets.removed.is_empty());
    // Only the idle entry is resident at load; the triggered blast entry is
    // loaded explicitly here so discovery covers every referenced file.
    let blast = scenario
        .set_entry_resident("playlist", 2, true)
        .expect("blast becomes resident");
    assert_eq!(blast.defs.added, vec![support::root_def("/blast.json")]);
    let registry = scenario.registry();

    assert_loaded_def_kinds(
        registry,
        &[
            ("/module.json", NodeKind::Module),
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
        registry,
        &[
            ("/blast.glsl", AssetContentType::ShaderSource),
            ("/fyeah.map2d.json", AssetContentType::FixtureMap2d),
            ("/idle.glsl", AssetContentType::ShaderSource),
        ],
    );

    assert_eq!(load.changes.defs.added.len(), 8);
    assert_eq!(load.changes.assets.added.len(), 2);
}

#[test]
fn fyeah_sign_derives_with_only_the_idle_entry_resident() {
    let (scenario, load) = RegistryScenario::load_fixture("fyeah-sign");
    let registry = scenario.registry();

    assert_loaded_def_kinds(
        registry,
        &[
            ("/module.json", NodeKind::Module),
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
        registry,
        &[
            ("/fyeah.map2d.json", AssetContentType::FixtureMap2d),
            ("/idle.glsl", AssetContentType::ShaderSource),
        ],
    );
    let blast_use = support::playlist_use("playlist")
        .child(lpc_model::SlotPath::parse("entries[2].node").unwrap());
    assert!(!registry.inventory().tree.nodes.contains_key(&blast_use));
    assert!(!load.changes.uses.added.contains(&blast_use));
    // The playlist def still remembers the dormant entry.
    let playlist = registry
        .def(&support::root_def("/playlist.json"))
        .and_then(|entry| entry.state.loaded_def())
        .and_then(lpc_model::NodeDef::as_playlist)
        .expect("playlist def");
    assert!(playlist.entries.entries.contains_key(&2));
    // Nothing of the blast entry was even registered.
    assert!(
        registry
            .artifacts()
            .locations()
            .all(|location| !location.file_path().as_str().starts_with("/blast"))
    );
}
