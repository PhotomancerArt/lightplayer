//! End-to-end **import** tests (module authoring unit, P5): vendoring a
//! library pattern's export into the open project, and starting a whole new
//! project from one.
//!
//! Both gestures copy an export folder out of one package and into another,
//! so both stand or fall on the same property: the folder's INTERNAL
//! references are relative, and re-rooting the folder must preserve them
//! byte for byte. These tests pin that where it is actually observable —
//! after a round trip through a real `LpServer`, where a broken ref means a
//! module with no children rather than a diff nobody reads.
//!
//! What they cover:
//!
//! - the picker's import source lists the library's pattern exports (one
//!   row per export for a family) and never the project you are standing in;
//! - picking one writes `modules/<key>/**` on the runtime AND in the
//!   library, attaches it at the project root, and the vendored module
//!   loads with its own child — proof the relative refs survived;
//! - an export with no provenance of its own inherits the source project's
//!   attribution on the way out (R14);
//! - importing the same export twice dedupes the key (`fire`, `fire_2`)
//!   instead of rejecting;
//! - "New project from this…" composes the pattern rig around the vendored
//!   export, designates it, and opens — lint-clean.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use lpc_model::AsLpPath;
use lpfs::LpFsMemory;

use crate::app::library::{LibraryStore, MemoryLibraryHost, PackageProvenance};
use crate::app::studio::studio_edit_e2e_tests::{
    InProcessServerIo, device_e2e_server, drive, project_editor,
};
use crate::{
    ControllerId, HOME_NODE_ID, HomeOp, StudioActor, StudioCommand, StudioController,
    StudioServerClient, UiAction, UiStudioView,
};

/// Where a library-opened project lives on the runtime.
const PROJECT_DIR: &str = "/projects/studio";

/// The SOURCE package: a pattern project designating two exports, so the
/// picker has a family to expand and the key dedupe has a sibling to
/// collide with. `fire` deliberately carries NO provenance (the R14 stamp
/// is what this fixture is for); `ice` carries its own, which must survive
/// untouched.
fn sparkle_pack_files() -> Vec<(String, Vec<u8>)> {
    files(&[
        (
            "project.json",
            r#"{
  "format": 5,
  "name": "Sparkle pack",
  "author": "Yona",
  "version": "2",
  "license": "CC0-1.0",
  "created": "2026-08-01",
  "kind": "pattern",
  "exports": [
    "fire",
    "ice"
  ]
}
"#,
        ),
        (
            "module.json",
            r#"{
  "kind": "Module",
  "nodes": {
    "fire": { "ref": "./fire/module.json" },
    "ice": { "ref": "./ice/module.json" }
  }
}"#,
        ),
        ("fire/module.json", FIRE_MODULE),
        ("fire/pixels.json", FOLDER_FIXTURE),
        (
            "ice/module.json",
            r#"{
  "kind": "Module",
  "nodes": {
    "pixels": { "ref": "./pixels.json" }
  },
  "provenance": {
    "author": "Someone Else"
  }
}"#,
        ),
        ("ice/pixels.json", FOLDER_FIXTURE),
    ])
}

/// The export with no attribution of its own — and one relative ref, which
/// is the thing re-rooting must not break.
const FIRE_MODULE: &str = r#"{
  "kind": "Module",
  "nodes": {
    "pixels": { "ref": "./pixels.json" }
  }
}"#;

/// A self-contained fixture node: no shader, so the e2e server has no GLSL
/// to compile and a failure here can only mean the vendoring.
const FOLDER_FIXTURE: &str = r#"{
  "kind": "Fixture",
  "render_size": { "width": 8, "height": 8 },
  "bindings": {
    "input": { "source": "bus:visual.out" },
    "output": { "target": "bus:control.out" }
  }
}"#;

/// The DESTINATION package: an ordinary general project with a clock.
fn workbench_files() -> Vec<(String, Vec<u8>)> {
    files(&[
        ("project.json", "{\n  \"format\": 5\n}\n"),
        (
            "module.json",
            r#"{
  "kind": "Module",
  "nodes": {
    "clock": { "ref": "./clock.json" }
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
    ])
}

fn files(entries: &[(&str, &str)]) -> Vec<(String, Vec<u8>)> {
    entries
        .iter()
        .map(|(name, body)| ((*name).to_string(), body.as_bytes().to_vec()))
        .collect()
}

/// A live studio with both packages installed in a real library store.
///
/// Returns the driving parts by value for the same reason the export e2e
/// does: the actor is generic over its timer factory.
macro_rules! studio_with_library {
    () => {{
        let server = Rc::new(RefCell::new(device_e2e_server()));
        let io = InProcessServerIo {
            server: Rc::clone(&server),
            inbox: Rc::new(RefCell::new(VecDeque::new())),
            sent: Rc::new(RefCell::new(Vec::new())),
        };
        let client = StudioServerClient::from_io_for_test("in-process", Box::new(io));
        let mut controller = StudioController::connected_with_client_for_test(client);
        // Distinct randomness per install: two packages sharing a minted
        // uid would resolve to one another through the uid index, and this
        // file's whole point is TWO packages.
        let seed = std::cell::Cell::new(9u8);
        let store = LibraryStore::new(
            Rc::new(RefCell::new(LpFsMemory::new())),
            Rc::new(move || {
                seed.set(seed.get().wrapping_add(1));
                [seed.get(); 16]
            }),
            Rc::new(|| "2026-08-07-1017".to_string()),
        );
        controller.attach_library(Rc::new(MemoryLibraryHost::new(
            store.clone(),
            Rc::new(|| 2.0),
        )));
        let (actor, handle) = StudioActor::new(controller, |_| core::future::ready(()));
        (actor, handle, store, server)
    }};
}

fn install(store: &LibraryStore, name: &str, files: Vec<(String, Vec<u8>)>) -> String {
    store
        .install_package(name, &files, PackageProvenance::Created, 1.0)
        .unwrap_or_else(|error| panic!("install {name}: {error}"))
        .uid
        .to_string()
}

/// One runtime file under the open project's storage dir, as text.
fn runtime_file(server: &Rc<RefCell<lpa_server::LpServer>>, name: &str) -> String {
    let bytes = server
        .borrow()
        .base_fs()
        .read_file(format!("{PROJECT_DIR}/{name}").as_path())
        .unwrap_or_else(|error| panic!("runtime file {name}: {error}"));
    String::from_utf8(bytes).expect("utf8 project file")
}

fn runtime_has(server: &Rc<RefCell<lpa_server::LpServer>>, name: &str) -> bool {
    server
        .borrow()
        .base_fs()
        .file_exists(format!("{PROJECT_DIR}/{name}").as_path())
        .unwrap_or(false)
}

/// Every root-child card label on the canvas.
fn child_labels(view: &UiStudioView) -> Vec<String> {
    project_editor(view).nodes[0]
        .children
        .iter()
        .map(|child| child.label.clone())
        .collect()
}

#[test]
fn importing_a_pattern_vendors_the_folder_stamps_it_and_dedupes_a_second_copy() {
    let (mut actor, handle, store, server) = studio_with_library!();
    let mut view = handle.view;
    let source_uid = install(&store, "Sparkle pack", sparkle_pack_files());
    let workbench_uid = install(&store, "Workbench", workbench_files());

    handle.tx.send(StudioCommand::Action(UiAction::from_op(
        ControllerId::new(HOME_NODE_ID),
        HomeOp::OpenPackage {
            key: workbench_uid.clone(),
        },
    )));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("open emits a snapshot");

    // -- the picker's import source ---------------------------------------
    let menu = project_editor(&snapshot)
        .add_node_menu
        .clone()
        .expect("the project root carries the add-node picker");
    assert_eq!(menu.imports_empty, None, "the library has a pattern in it");
    let labels: Vec<&str> = menu.imports.iter().map(|e| e.label.as_str()).collect();
    assert_eq!(
        labels.len(),
        2,
        "a two-export family expands to a row each: {labels:?}"
    );
    assert!(
        labels.iter().all(|label| label.contains("sparkle-pack"))
            && labels.iter().any(|label| label.ends_with("· fire"))
            && labels.iter().any(|label| label.ends_with("· ice")),
        "family rows name the package AND the export: {labels:?}"
    );
    assert!(
        menu.imports.iter().all(|entry| {
            entry
                .action
                .op_as::<crate::NodeImportOp>()
                .is_some_and(|op| op.package_uid == source_uid)
        }),
        "the open project is never offered as an import source"
    );
    let fire = menu
        .imports
        .iter()
        .find(|entry| entry.label.ends_with("· fire"))
        .expect("the fire row")
        .action
        .clone();

    // -- import ------------------------------------------------------------
    handle.tx.send(StudioCommand::Action(fire.clone()));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("import emits a snapshot");

    // Files landed under `modules/<key>/`, whole folder.
    assert!(runtime_has(&server, "modules/fire/module.json"));
    assert!(
        runtime_has(&server, "modules/fire/pixels.json"),
        "the folder's other files came too"
    );
    // …and the root module attached it by a ref that resolves.
    let root = runtime_file(&server, "module.json");
    assert!(
        root.contains("modules/fire/module.json"),
        "the vendored module is attached at the project root: {root}"
    );

    // R14: `fire` had no provenance, so it inherited the SOURCE project's.
    let vendored = runtime_file(&server, "modules/fire/module.json");
    let def = lpc_model::NodeDef::from_json_str(&vendored).expect("vendored module parses");
    let provenance = def
        .as_module()
        .expect("module def")
        .provenance
        .data
        .clone()
        .expect("an unprovenanced export inherits the source project's attribution");
    assert_eq!(provenance.author.data.clone().unwrap().value(), "Yona");
    assert_eq!(provenance.license.data.clone().unwrap().value(), "CC0-1.0");
    // The folder's own relative ref was NOT rewritten by the re-rooting.
    assert!(
        vendored.contains("\"ref\": \"pixels.json\"") || vendored.contains("\"./pixels.json\""),
        "the module's internal ref stayed relative: {vendored}"
    );

    // The module is HEALTHY: the engine loaded it and its child mounted —
    // which is only possible if that relative ref resolved after the move.
    let labels = child_labels(&snapshot);
    assert!(
        labels
            .iter()
            .any(|label| label.eq_ignore_ascii_case("fire")),
        "the vendored module is a card on the canvas: {labels:?}"
    );
    let fire_card = project_editor(&snapshot).nodes[0]
        .children
        .iter()
        .find(|child| child.label.eq_ignore_ascii_case("fire"))
        .expect("fire card");
    assert!(
        fire_card
            .children
            .iter()
            .any(|child| child.label.eq_ignore_ascii_case("pixels")),
        "the vendored module's own child loaded — the relative ref resolved: {:?}",
        fire_card
            .children
            .iter()
            .map(|child| child.label.clone())
            .collect::<Vec<_>>()
    );

    // Creation commits, and the save-pull put the same bytes in the library.
    let library_files = store
        .open(workbench_uid.parse().expect("uid"))
        .expect("library reopens")
        .read_all_files()
        .expect("library files");
    let library_vendored = library_files
        .iter()
        .find(|(path, _)| path == "modules/fire/module.json")
        .map(|(_, bytes)| String::from_utf8(bytes.clone()).expect("utf8"))
        .expect("the vendored module reached the library");
    assert_eq!(
        library_vendored, vendored,
        "library and runtime copies must stay byte-identical"
    );

    // -- import the same export again: deduped, not rejected ---------------
    handle.tx.send(StudioCommand::Action(fire));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("second import emits a snapshot");

    assert!(
        runtime_has(&server, "modules/fire_2/module.json")
            && runtime_has(&server, "modules/fire_2/pixels.json"),
        "the second copy takes the deduped key"
    );
    let labels = child_labels(&snapshot);
    assert!(
        labels
            .iter()
            .any(|label| label.eq_ignore_ascii_case("fire"))
            && labels
                .iter()
                .any(|label| label.to_ascii_lowercase().replace(' ', "_") == "fire_2"),
        "both copies are on the canvas, independent of one another: {labels:?}"
    );
    assert_eq!(
        runtime_file(&server, "modules/fire_2/module.json"),
        vendored,
        "the second copy is the same bytes, at a different key"
    );
}

/// A library with nothing to import says so on a disabled row rather than
/// dropping the source — a hole where an affordance was is worse than a
/// sentence explaining it.
#[test]
fn an_empty_library_shows_the_import_sources_empty_state() {
    let (mut actor, handle, store, _server) = studio_with_library!();
    let mut view = handle.view;
    let workbench_uid = install(&store, "Workbench", workbench_files());

    handle.tx.send(StudioCommand::Action(UiAction::from_op(
        ControllerId::new(HOME_NODE_ID),
        HomeOp::OpenPackage { key: workbench_uid },
    )));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("open emits a snapshot");

    let menu = project_editor(&snapshot)
        .add_node_menu
        .clone()
        .expect("the picker is there");
    assert!(
        menu.imports.is_empty(),
        "a library holding only general projects offers nothing to import"
    );
    assert_eq!(
        menu.imports_empty.as_deref(),
        Some("No patterns in your library")
    );
}

/// "New project from this…": the card gesture lands you in a running
/// workbench built around somebody else's pattern — rig at the root, your
/// own copy of their export designated, and the export lint clean.
#[test]
fn new_project_from_a_pattern_opens_a_rig_around_the_vendored_export() {
    let (mut actor, handle, store, server) = studio_with_library!();
    let mut view = handle.view;
    let source_uid = install(&store, "Sparkle pack", sparkle_pack_files());

    handle.tx.send(StudioCommand::Action(UiAction::from_op(
        ControllerId::new(HOME_NODE_ID),
        HomeOp::CreateFromPattern {
            uid: source_uid,
            export: "fire".to_string(),
            name: "fire-project".to_string(),
        },
    )));
    drive(actor.run_one_batch_for_test());
    let snapshot = view.try_recv().expect("create-and-open emits a snapshot");

    assert!(snapshot.home.is_none(), "the composed project opened");
    let labels = child_labels(&snapshot);
    for expected in ["Clock", "Effect", "Strip 300", "Matrix 32x16"] {
        assert!(
            labels.iter().any(|label| label == expected),
            "the 1D rig's {expected} card is on the canvas: {labels:?}"
        );
    }

    // The export is the SOURCE's module (with its inherited provenance),
    // not the template's own.
    let effect = runtime_file(&server, "effect/module.json");
    let def = lpc_model::NodeDef::from_json_str(&effect).expect("effect module parses");
    let module = def.as_module().expect("module def");
    assert!(
        module.nodes.entries.contains_key("pixels"),
        "the vendored export replaced the template's: {effect}"
    );
    assert_eq!(
        module
            .provenance
            .data
            .clone()
            .expect("stamped on the way in")
            .author
            .data
            .clone()
            .unwrap()
            .value(),
        "Yona"
    );
    assert!(
        runtime_has(&server, "effect/pixels.json"),
        "the folder's other files came too"
    );
    assert!(
        !runtime_has(&server, "effect/shader.glsl"),
        "nothing of the template's OWN export survives beside the vendored one"
    );

    // The manifest designates it, so P3's exports rail is up with no
    // further gesture — and the lint on it reads clean.
    let manifest = lpc_model::ProjectManifest::read_json(&runtime_file(&server, "project.json"))
        .expect("manifest parses");
    assert_eq!(
        manifest.project_kind(),
        lpc_model::ProjectKind::Pattern {
            exports: vec!["effect".to_string()]
        }
    );
    let crate::UiNodeFace::Module(root_face) = project_editor(&snapshot).nodes[0]
        .face
        .clone()
        .expect("the root card wears a face")
    else {
        panic!("the root card wears a module face");
    };
    let exports = root_face
        .exports
        .expect("the composed project arrives with its exports rail");
    assert_eq!(exports.rows.len(), 1);
    assert_eq!(exports.rows[0].name, "effect");
    assert_eq!(
        exports.worst(),
        None,
        "a vendored, licensed, self-contained export reads clean: {:?}",
        exports.findings
    );
}
