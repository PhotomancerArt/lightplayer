//! Playlist entry residency: derivation stops at dormant entries, and a
//! residency change reports and releases exactly the entries it moves.

mod support;

use lpc_model::{
    ArtifactLocation, LpValue, MutationOp, MutationRejectionReason, NodeUseLocation, SlotEdit,
    SlotPath,
};
use lpc_registry::{EntryResidencyError, ProjectRegistry};

use support::{RegistryScenario, artifact, artifact_asset, playlist_use, root_def};

#[test]
fn three_entry_playlist_loads_only_the_idle_entry() {
    let (scenario, load) = three_entry_project();
    let registry = scenario.registry();

    assert_entry_rows(registry, 1, true);
    assert_entry_rows(registry, 2, false);
    assert_entry_rows(registry, 3, false);
    assert!(load.changes.uses.added.contains(&entry_use(1)));
    assert!(!load.changes.uses.added.contains(&entry_use(2)));
    assert!(!load.changes.uses.added.contains(&entry_use(3)));
}

#[test]
fn switching_residency_reports_added_and_removed_and_leaves_nothing_behind() {
    let (mut scenario, _) = three_entry_project();

    let loaded = scenario
        .set_entry_resident("playlist", 2, true)
        .expect("entry 2 loads");
    assert_eq!(loaded.uses.added, vec![entry_use(2)]);
    assert!(loaded.uses.removed.is_empty());
    assert_eq!(loaded.defs.added, vec![root_def("/two/shader.json")]);
    assert_eq!(
        loaded.assets.added,
        vec![artifact_asset("/two/shader.glsl")]
    );

    let unloaded = scenario
        .set_entry_resident("playlist", 1, false)
        .expect("entry 1 unloads");
    assert_eq!(unloaded.uses.removed, vec![entry_use(1)]);
    assert!(unloaded.uses.added.is_empty());
    assert_eq!(unloaded.defs.removed, vec![root_def("/one/shader.json")]);
    assert_eq!(
        unloaded.assets.removed,
        vec![artifact_asset("/one/shader.glsl")]
    );

    let registry = scenario.registry();
    assert_entry_rows(registry, 1, false);
    assert_entry_rows(registry, 2, true);
    assert_entry_rows(registry, 3, false);
    assert_eq!(
        registry.residency().explicit_set(&playlist_use("playlist")),
        Some(&[2][..])
    );
}

#[test]
fn make_only_resident_swaps_entries_in_one_change() {
    let (mut scenario, _) = three_entry_project();

    let changes = scenario
        .make_only_resident("playlist", 3)
        .expect("entry 3 becomes the only resident");
    assert_eq!(changes.uses.added, vec![entry_use(3)]);
    assert_eq!(changes.uses.removed, vec![entry_use(1)]);

    let registry = scenario.registry();
    assert_entry_rows(registry, 1, false);
    assert_entry_rows(registry, 2, false);
    assert_entry_rows(registry, 3, true);

    let again = scenario
        .make_only_resident("playlist", 3)
        .expect("no-op change");
    assert!(again.is_empty());
}

#[test]
fn an_unrelated_edit_after_a_switch_does_not_re_add_dormant_entries() {
    let (mut scenario, _) = three_entry_project();
    scenario
        .make_only_resident("playlist", 2)
        .expect("entry 2 becomes the only resident");

    let edit = scenario.apply(MutationOp::PutSlotEdit {
        artifact: artifact("/clock.json"),
        edit: SlotEdit::assign_value(
            SlotPath::parse("transport.rate").unwrap(),
            LpValue::F32(2.0),
        ),
    });
    assert!(edit.changes.uses.added.is_empty());
    assert!(edit.changes.uses.removed.is_empty());
    assert!(edit.changes.defs.added.is_empty());
    assert!(edit.changes.assets.added.is_empty());

    let registry = scenario.registry();
    assert_entry_rows(registry, 1, false);
    assert_entry_rows(registry, 2, true);
    assert_entry_rows(registry, 3, false);
}

#[test]
fn editing_a_dormant_entry_names_the_entry_instead_of_an_unknown_artifact() {
    let (mut scenario, _) = three_entry_project();

    // The entry's own def, and a file under the entry's own directory.
    for path in ["/two/shader.json", "/two/inner.json"] {
        let batch = lpc_model::MutationCmdBatch::new(vec![lpc_model::MutationCmd {
            id: lpc_model::MutationCmdId::new(1),
            mutation: MutationOp::PutSlotEdit {
                artifact: artifact(path),
                edit: SlotEdit::assign_value(
                    SlotPath::parse("render_order").unwrap(),
                    LpValue::I32(1),
                ),
            },
        }]);
        let results = scenario.apply_batch(batch);
        let result = &results.commands.results[0];
        let lpc_model::MutationCmdStatus::Rejected { rejection } = &result.status else {
            panic!("edit of a dormant entry must reject: {result:?}");
        };
        assert_eq!(rejection.reason, MutationRejectionReason::UnknownArtifact);
        assert_eq!(
            rejection.message,
            "entry 2 (\"two\") of playlist /playlist.json is not loaded; load it to edit it",
            "{path}"
        );
    }

    // A file nothing references stays a plain unknown artifact.
    let batch = lpc_model::MutationCmdBatch::new(vec![lpc_model::MutationCmd {
        id: lpc_model::MutationCmdId::new(2),
        mutation: MutationOp::RemoveSlotEdit {
            artifact: artifact("/elsewhere.json"),
            path: SlotPath::parse("render_order").unwrap(),
        },
    }]);
    let results = scenario.apply_batch(batch);
    let lpc_model::MutationCmdStatus::Rejected { rejection } = &results.commands.results[0].status
    else {
        panic!("unknown artifact must reject");
    };
    assert_eq!(rejection.message, "unknown artifact /elsewhere.json");
}

#[test]
fn unloading_an_entry_with_pending_edits_is_refused_and_changes_nothing() {
    let (mut scenario, _) = three_entry_project();
    scenario
        .set_entry_resident("playlist", 2, true)
        .expect("entry 2 loads");
    scenario.apply(MutationOp::PutSlotEdit {
        artifact: artifact("/two/shader.json"),
        edit: SlotEdit::assign_value(SlotPath::parse("render_order").unwrap(), LpValue::I32(3)),
    });
    let locations_before: Vec<ArtifactLocation> =
        scenario.registry().artifacts().locations().collect();

    let refused = scenario
        .make_only_resident("playlist", 3)
        .expect_err("pending edits block the unload");
    assert_eq!(
        refused,
        EntryResidencyError::PendingEdits {
            playlist: artifact("/playlist.json"),
            entry: 3,
            artifacts: vec![artifact("/two/shader.json")],
        }
    );

    let registry = scenario.registry();
    assert_entry_rows(registry, 1, true);
    assert_entry_rows(registry, 2, true);
    assert_entry_rows(registry, 3, false);
    let locations_after: Vec<ArtifactLocation> = registry.artifacts().locations().collect();
    assert_eq!(locations_after, locations_before);

    // Committing clears the overlay; the unload then goes through.
    scenario.commit();
    scenario
        .make_only_resident("playlist", 3)
        .expect("unload after commit");
    assert_entry_rows(scenario.registry(), 2, false);
}

#[test]
fn residency_targets_must_be_a_loaded_playlist_entry() {
    let (mut scenario, _) = three_entry_project();

    assert_eq!(
        scenario.set_entry_resident("nope", 1, true),
        Err(EntryResidencyError::UnknownPlaylist)
    );
    assert_eq!(
        scenario.set_entry_resident("clock", 1, true),
        Err(EntryResidencyError::NotAPlaylist {
            def: artifact("/clock.json")
        })
    );
    assert_eq!(
        scenario.set_entry_resident("playlist", 9, true),
        Err(EntryResidencyError::UnknownEntry {
            playlist: artifact("/playlist.json"),
            entry: 9
        })
    );
}

/// A module with a clock and a three-entry playlist (idle = 1). Each entry is
/// a shader in its own directory with its own GLSL source.
fn three_entry_project() -> (RegistryScenario, lpc_registry::LoadResult) {
    let mut scenario = RegistryScenario::empty();
    scenario.write_container_manifest();
    scenario.write_file(
        "/module.json",
        r#"{
  "kind": "Module",
  "nodes": {
    "clock": { "ref": "./clock.json" },
    "playlist": { "ref": "./playlist.json" }
  }
}"#,
    );
    scenario.write_file(
        "/clock.json",
        r#"{
  "kind": "Clock",
  "transport": { "rate": 1.0 }
}"#,
    );
    scenario.write_file(
        "/playlist.json",
        r#"{
  "kind": "Playlist",
  "idle_entry": 1,
  "entries": {
    "1": { "name": "one", "node": { "ref": "./one/shader.json" } },
    "2": { "name": "two", "node": { "ref": "./two/shader.json" } },
    "3": { "name": "three", "node": { "ref": "./three/shader.json" } }
  }
}"#,
    );
    for name in ["one", "two", "three"] {
        scenario.write_file(
            &format!("/{name}/shader.json"),
            r#"{
  "kind": "Shader",
  "source": { "path": "./shader.glsl" }
}"#,
        );
        scenario.write_file(
            &format!("/{name}/shader.glsl"),
            "vec4 render_2d(vec2 pos) { return vec4(0.0); }",
        );
    }
    let load = scenario.load_root("/module.json");
    (scenario, load)
}

fn entry_name(entry: u32) -> &'static str {
    match entry {
        1 => "one",
        2 => "two",
        3 => "three",
        _ => unreachable!("fixture has three entries"),
    }
}

fn entry_use(entry: u32) -> NodeUseLocation {
    playlist_use("playlist").child(SlotPath::parse(&format!("entries[{entry}].node")).unwrap())
}

/// Entry `entry`'s rows are all present (resident) or all absent (dormant):
/// tree node, def row, asset row, and both artifact-store locations.
fn assert_entry_rows(registry: &ProjectRegistry, entry: u32, resident: bool) {
    let name = entry_name(entry);
    let def_path = format!("/{name}/shader.json");
    let glsl_path = format!("/{name}/shader.glsl");
    let inventory = registry.inventory();

    assert_eq!(
        inventory.tree.nodes.contains_key(&entry_use(entry)),
        resident,
        "tree node of entry {entry}"
    );
    assert_eq!(
        inventory.defs.contains_key(&root_def(&def_path)),
        resident,
        "def row of entry {entry}"
    );
    assert_eq!(
        inventory.assets.contains_key(&artifact_asset(&glsl_path)),
        resident,
        "asset row of entry {entry}"
    );
    assert_eq!(
        inventory
            .tree
            .asset_consumers
            .contains_key(&artifact_asset(&glsl_path)),
        resident,
        "asset consumer of entry {entry}"
    );
    for path in [&def_path, &glsl_path] {
        assert_eq!(
            registry.artifacts().entry(&artifact(path)).is_some(),
            resident,
            "artifact-store location {path}"
        );
    }
    // The playlist def always remembers the entry.
    let playlist = registry
        .def(&root_def("/playlist.json"))
        .and_then(|entry| entry.state.loaded_def())
        .and_then(lpc_model::NodeDef::as_playlist)
        .expect("playlist def");
    assert!(playlist.entries.entries.contains_key(&entry));
}
