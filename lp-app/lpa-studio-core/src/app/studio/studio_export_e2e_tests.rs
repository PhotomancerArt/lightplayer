//! End-to-end export **designation** tests (module authoring unit, P3).
//!
//! No shipped example carries a folder sub-module, so this file owns the
//! fixture: a project whose `effect/` folder holds its own `module.json`,
//! installed into a real [`crate::app::library::LibraryStore`] and opened
//! through the ordinary `HomeOp::OpenPackage` path against an in-process
//! server. That is the only shape designation applies to, and it is what
//! the popup's enable rule looks for.
//!
//! What these pin, end to end:
//!
//! - the child module card carries a live designation row naming its own
//!   folder, and the ROOT card carries none (an export must not point at
//!   the root — vision Q3);
//! - toggling it patches the library manifest through P1's canonical
//!   writer, upgrading `General` → `Pattern` on the first export and back
//!   on the last;
//! - the workspace child column's exports/rig grouping (G1 R-A, which
//!   replaced P3's on-face rail) appears and disappears with it, with the
//!   aggregate lint verdict attached, WITHOUT reopening the project;
//! - the runtime copy of `project.json` moves with the library copy, so
//!   the save path's library/runtime hash tripwire stays quiet.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use lpc_model::AsLpPath;
use lpfs::LpFsMemory;

use crate::app::library::{LibraryStore, MemoryLibraryHost, PackageProvenance, PackageSummary};
use crate::app::studio::studio_edit_e2e_tests::{
    InProcessServerIo, device_e2e_server, drive, project_editor,
};
use crate::{
    ControllerId, HOME_NODE_ID, HomeOp, ModuleExportOp, ProjectController, StudioActor,
    StudioCommand, StudioController, StudioServerClient, UiAction, UiExportsGroup, UiModuleExport,
    UiNodeFace, UiStudioView,
};

/// The fixture project: a root module with a clock plus an `effect/` FOLDER
/// sub-module — the one shape an export can name.
///
/// The effect module carries authored provenance with a license, so a clean
/// designation reads clean (the static half warns about an unlicensed
/// export, and this file wants to see both states deliberately).
fn folder_module_files() -> Vec<(String, Vec<u8>)> {
    let files: &[(&str, &str)] = &[
        ("project.json", "{\n  \"format\": 5\n}\n"),
        (
            "module.json",
            r#"{
  "kind": "Module",
  "nodes": {
    "clock": { "ref": "./clock.json" },
    "effect": { "ref": "./effect/module.json" }
  }
}"#,
        ),
        (
            "clock.json",
            r#"{
  "kind": "Clock",
  "transport": {
    "running": true,
    "rate": 1.0
  }
}"#,
        ),
        (
            "effect/module.json",
            r#"{
  "kind": "Module",
  "nodes": {
    "pixels": { "ref": "./fixture.json" }
  },
  "provenance": {
    "author": "Yona",
    "version": "1",
    "license": "CC0-1.0"
  }
}"#,
        ),
        (
            "effect/fixture.json",
            r#"{
  "kind": "Fixture",
  "render_size": { "width": 10, "height": 10 },
  "bindings": {
    "input": { "source": "bus:visual.out" },
    "output": { "target": "bus:control.out" }
  }
}"#,
        ),
    ];
    files
        .iter()
        .map(|(name, body)| ((*name).to_string(), body.as_bytes().to_vec()))
        .collect()
}

/// A live studio over `files`, with the package installed in a real
/// library store and opened through the ordinary home path.
///
/// Returns the driving parts by value: the actor is generic over its timer
/// factory, so it cannot ride a plain struct field without infecting every
/// helper with the parameter.
macro_rules! open_fixture {
    ($files:expr, $name:literal) => {{
        let server = Rc::new(RefCell::new(device_e2e_server()));
        let io = InProcessServerIo {
            server: Rc::clone(&server),
            inbox: Rc::new(RefCell::new(VecDeque::new())),
            sent: Rc::new(RefCell::new(Vec::new())),
        };
        let client = StudioServerClient::from_io_for_test("in-process", Box::new(io));
        let mut controller = StudioController::connected_with_client_for_test(client);

        let store = LibraryStore::new(
            Rc::new(RefCell::new(LpFsMemory::new())),
            Rc::new(|| [9u8; 16]),
            Rc::new(|| "2026-08-07-1017".to_string()),
        );
        let summary = store
            .install_package($name, &$files, PackageProvenance::Created, 1.0)
            .expect("install the folder-module fixture");
        controller.attach_library(Rc::new(MemoryLibraryHost::new(
            store.clone(),
            Rc::new(|| 2.0),
        )));

        let (mut actor, handle) = StudioActor::new(controller, |_| core::future::ready(()));
        let mut view = handle.view;
        handle.tx.send(StudioCommand::Action(UiAction::from_op(
            ControllerId::new(HOME_NODE_ID),
            HomeOp::OpenPackage {
                key: summary.uid.to_string(),
            },
        )));
        drive(actor.run_one_batch_for_test());
        let snapshot = view.try_recv().expect("open emits a snapshot");
        (actor, handle.tx, view, store, summary, server, snapshot)
    }};
}

/// Dispatch one designation and take the snapshot it emits.
macro_rules! designate {
    ($actor:expr, $tx:expr, $view:expr, $folder:expr, $export:expr) => {{
        $tx.send(StudioCommand::Action(UiAction::from_op(
            ControllerId::new(ProjectController::NODE_ID),
            ModuleExportOp {
                folder: $folder.to_string(),
                export: $export,
            },
        )));
        drive($actor.run_one_batch_for_test());
        $view.try_recv().expect("designation emits a snapshot")
    }};
}

/// The library copy's `project.json`, as text.
fn library_manifest(store: &LibraryStore, summary: &PackageSummary) -> String {
    let handle = store.open(summary.uid).expect("library reopens");
    let bytes = handle
        .package_fs
        .borrow()
        .read_file("/project.json".as_path())
        .expect("library project.json");
    String::from_utf8(bytes).expect("utf8 manifest")
}

/// The root card's module face.
fn root_face(view: &UiStudioView) -> crate::UiModuleFace {
    let Some(UiNodeFace::Module(face)) = project_editor(view)
        .nodes
        .first()
        .expect("the root module card")
        .face
        .clone()
    else {
        panic!("the root card wears a module face");
    };
    face
}

/// How the root card's CHILD COLUMN is grouped (R-A): the exports/rig split
/// that replaced P3's on-face rail.
fn root_exports(view: &UiStudioView) -> Option<UiExportsGroup> {
    project_editor(view)
        .nodes
        .first()
        .expect("the root module card")
        .exports
        .clone()
}

/// The labels of the child cards the grouping puts under `exports`.
fn exported_child_labels(view: &UiStudioView) -> Vec<String> {
    let root = project_editor(view)
        .nodes
        .first()
        .expect("the root module card")
        .clone();
    let keys = root.exports.map(|group| group.keys).unwrap_or_default();
    root.children
        .iter()
        .filter(|child| keys.contains(&child.detail))
        .map(|child| child.label.clone())
        .collect()
}

/// The `effect` child card's designation row.
fn effect_export(view: &UiStudioView) -> UiModuleExport {
    let root = project_editor(view)
        .nodes
        .first()
        .expect("the root module card");
    let child = root
        .children
        .iter()
        .find(|child| child.label.eq_ignore_ascii_case("effect"))
        .unwrap_or_else(|| {
            panic!(
                "the effect child card; saw {:?}",
                root.children
                    .iter()
                    .map(|c| (c.label.clone(), c.kind.clone(), c.detail.clone()))
                    .collect::<Vec<_>>()
            )
        });
    let Some(UiNodeFace::Module(face)) = child.face.clone() else {
        panic!("the effect card wears a module face");
    };
    face.export.expect("the effect card offers designation")
}

#[test]
fn designation_round_trips_from_the_module_card_to_the_child_grouping() {
    let (mut actor, tx, mut view, store, summary, server, snapshot) =
        open_fixture!(folder_module_files(), "Yona noise");

    // -- before: a plain General project ----------------------------------
    assert_eq!(
        root_exports(&snapshot),
        None,
        "a project that exports nothing keeps its child column ungrouped"
    );
    assert_eq!(
        root_face(&snapshot).export,
        None,
        "the root module is never offered as an export (vision Q3)"
    );
    let before = effect_export(&snapshot);
    assert_eq!(before.folder, "effect");
    assert_eq!(before.project, "Yona noise");
    assert!(!before.designated);
    assert_eq!(
        before.disabled_reason, None,
        "a folder module directly under the root is designatable"
    );
    assert!(
        before.upgrades_to_pattern,
        "the first export on a General project is the upgrade gesture (D14)"
    );

    // -- designate --------------------------------------------------------
    let snapshot = designate!(actor, tx, view, "effect", true);
    let manifest = library_manifest(&store, &summary);
    assert!(
        manifest.contains("\"kind\": \"pattern\"") && manifest.contains("\"effect\""),
        "the library manifest carries the designation: {manifest}"
    );

    let exports: UiExportsGroup =
        root_exports(&snapshot).expect("the child column grouped without a reopen");
    assert_eq!(exports.keys.len(), 1);
    assert_eq!(
        exported_child_labels(&snapshot),
        vec!["Effect".to_string()],
        "the effect card moved under the EXPORTS header"
    );
    assert_eq!(
        exports.worst(),
        None,
        "a licensed, self-contained folder reads clean: {:?}",
        exports.findings
    );
    let after = effect_export(&snapshot);
    assert!(after.designated);
    assert!(
        !after.upgrades_to_pattern,
        "the project is already a pattern project"
    );

    // -- the runtime copy moved with the library copy ---------------------
    let runtime = {
        let server = server.borrow();
        let bytes = server
            .base_fs()
            .read_file("/projects/studio/project.json".as_path())
            .expect("runtime project.json");
        String::from_utf8(bytes).expect("utf8")
    };
    assert_eq!(
        runtime, manifest,
        "library and runtime copies must stay byte-identical or the save-path \
         hash tripwire fires"
    );

    // -- undesignate: back to General -------------------------------------
    let snapshot = designate!(actor, tx, view, "effect", false);
    let manifest = library_manifest(&store, &summary);
    assert!(
        !manifest.contains("kind") && !manifest.contains("exports"),
        "removing the last export clears both keys: {manifest}"
    );
    assert_eq!(
        root_exports(&snapshot),
        None,
        "the grouping leaves with the last export"
    );
    assert!(!effect_export(&snapshot).designated);
}

/// The lint verdict reaches BOTH surfaces, and it refreshes after a
/// designation even though the package write never advanced `last_synced`
/// (P2's manual epoch is what makes that true).
#[test]
fn the_lint_verdict_reaches_the_popup_row_and_the_exports_preamble() {
    // The same fixture with the effect module's provenance stripped: the
    // static half's "an importer cannot tell who wrote this" warning.
    let mut files = folder_module_files();
    for (name, body) in files.iter_mut() {
        if name == "effect/module.json" {
            *body = br#"{
  "kind": "Module",
  "nodes": {
    "pixels": { "ref": "./fixture.json" }
  }
}"#
            .to_vec();
        }
    }
    let (mut actor, tx, mut view, _store, _summary, _server, _snapshot) =
        open_fixture!(files, "Unlicensed");

    let snapshot = designate!(actor, tx, view, "effect", true);

    let exports = root_exports(&snapshot).expect("the grouping appears with the first export");
    assert_eq!(
        exports.worst(),
        Some(lpc_model::ExportSeverity::Warning),
        "an export with no provenance is a warning, not an error: {:?}",
        exports.findings
    );
    let row = effect_export(&snapshot);
    assert!(row.designated);
    assert!(
        row.findings
            .iter()
            .any(|finding| finding.severity == lpc_model::ExportSeverity::Warning),
        "the same finding renders in the module's own popup: {:?}",
        row.findings
    );
}
