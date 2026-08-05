//! P03 gate: **every** board in the catalog must generate a first project
//! that is real, not merely well-formed.
//!
//! Three oracles per board, the same ones the checked-in example corpus is
//! held to (`lp-cli/tests/schema_conformance.rs`,
//! `lp-cli/tests/examples_valid.rs`):
//!
//! 1. every authored file validates against the **checked-in** JSON Schemas
//!    (mapping documents through `lpc-mapping`, which owns that format);
//! 2. the package loads through the real `ProjectLoader` — the same call
//!    the editor sim and the device make, so a broken binding, an
//!    unresolvable node ref, or a playlist entry that produces no visual
//!    fails here rather than on hardware;
//! 3. the two board facts hold: the output endpoint names that board's
//!    declared default wire, and the manifest's `target` is that board id.
//!
//! A new board added to the catalog joins this test by existing.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use jsonschema::Validator;
use lpa_studio_core::app::home::{DEFAULT_STRIP_PIXELS, generate_board_project};
use lpc_engine::{EngineServices, ProjectLoader};
use lpc_model::{NodeDef, ProjectManifest, TreePath};
use lpfs::{LpFs, LpFsMemory, LpPath};
use serde_json::Value;

fn workspace_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("lpa-studio-core lives two levels under the workspace root")
        .parent()
        .expect("workspace dir")
        .to_path_buf()
}

fn validator(rel: &str) -> Validator {
    let path = workspace_dir().join(rel);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("reading {rel}: {error} — run `just schema-gen`"));
    let schema: Value = serde_json::from_str(&text).expect("schema is JSON");
    jsonschema::draft202012::new(&schema).expect("schema builds a validator")
}

#[test]
fn every_catalog_board_generates_a_valid_targeted_project() {
    let project_schema = validator("schemas/project.schema.json");
    let module_schema = validator("schemas/module.schema.json");
    let node_schema = validator("schemas/node.schema.json");

    let boards = lpa_boards::all_boards();
    assert!(!boards.is_empty(), "catalog walk is vacuous");

    let mut failures = Vec::new();
    for board in boards {
        let board_id = board.board_id.as_str();
        let wire = board
            .default_led_wire()
            .unwrap_or_else(|| panic!("{board_id}: catalog board declares no default LED wire"));
        let project =
            generate_board_project(board_id).unwrap_or_else(|error| panic!("{board_id}: {error}"));

        // 1. schema conformance, file by file
        for (path, bytes) in &project.files {
            if !path.ends_with(".json") {
                continue;
            }
            let text = std::str::from_utf8(bytes).expect("authored artifacts are utf8");
            if path.ends_with(".map2d.json") {
                let doc = lpc_mapping::Map2dDoc::from_json(text)
                    .unwrap_or_else(|error| panic!("{board_id}/{path}: {error}"));
                lpc_mapping::resolve(&doc)
                    .unwrap_or_else(|error| panic!("{board_id}/{path}: does not resolve: {error}"));
                continue;
            }
            let schema = if path == "project.json" {
                &project_schema
            } else if path.ends_with("module.json") {
                &module_schema
            } else {
                &node_schema
            };
            let instance: Value = serde_json::from_str(text)
                .unwrap_or_else(|error| panic!("{board_id}/{path}: not JSON: {error}"));
            for error in schema.iter_errors(&instance) {
                failures.push(format!(
                    "{board_id}/{path}: at `{}`: {error}",
                    error.instance_path()
                ));
            }
        }

        // 2. the real loader
        let fs = LpFsMemory::new();
        for (path, bytes) in &project.files {
            let absolute = format!("/{path}");
            fs.write_file(LpPath::new(&absolute), bytes)
                .unwrap_or_else(|error| panic!("{board_id}: staging {path}: {error:?}"));
        }
        let services = EngineServices::new(TreePath::parse("/generated.show").expect("root path"));
        if let Err(error) = ProjectLoader::load_from_root(&fs, services) {
            failures.push(format!("{board_id}: does not load: {error}"));
        }

        // 3. the two board facts
        let manifest = ProjectManifest::read_json(&file_text(&project.files, "project.json"))
            .expect("manifest");
        assert_eq!(
            manifest.target.as_deref(),
            Some(board_id),
            "{board_id}: the generated manifest must target the board it was generated for"
        );
        let endpoints = authored_endpoints(&file_text(&project.files, "output.json"));
        assert_eq!(
            endpoints,
            vec![format!("ws281x:local:{wire}")],
            "{board_id}: the output must drive exactly the board's first default wire"
        );
        assert_eq!(
            project.endpoint,
            format!("ws281x:local:{wire}"),
            "{board_id}: the reported endpoint and the authored one are the same string"
        );
    }

    assert!(
        failures.is_empty(),
        "{} generated project(s) failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// The generated package installs into a real library and comes back out
/// with its `target` intact — the catalog op is the wizard's actual seam,
/// and `install_package` rewrites the manifest (uid, name) on the way in.
#[test]
fn generate_for_board_installs_a_targeted_library_package() {
    use lpa_studio_core::app::library::{CatalogOp, LibraryStore, apply_catalog_op};

    let fs: Rc<RefCell<dyn LpFs>> = Rc::new(RefCell::new(LpFsMemory::new()));
    let store = LibraryStore::new(
        fs,
        Rc::new(|| [7u8; 16]),
        Rc::new(|| "2026-08-05-0900".to_string()),
    );
    let outcome = apply_catalog_op(
        &store,
        CatalogOp::GenerateForBoard {
            board_id: "domraem/dom-z-102".to_string(),
        },
        1.0,
    )
    .expect("generation installs");
    let summary = outcome.summary.expect("an installed package");

    let handle = store.open(summary.uid).expect("open the new package");
    let files = handle.read_all_files().expect("read back");
    let manifest_text = files
        .iter()
        .find(|(path, _)| path == "project.json")
        .map(|(_, bytes)| String::from_utf8(bytes.clone()).expect("utf8"))
        .expect("container manifest");
    let manifest = ProjectManifest::read_json(&manifest_text).expect("manifest parses");
    assert_eq!(manifest.target.as_deref(), Some("domraem/dom-z-102"));
    assert!(
        manifest.uid.is_some(),
        "the library mints an identity without dropping the target: {manifest_text}"
    );
    assert!(
        files.iter().any(|(path, _)| path == "effect/render.glsl"),
        "the vendored effect's assets survive installation: {:?}",
        files.iter().map(|(path, _)| path).collect::<Vec<_>>()
    );
}

/// The strip's pixel count is the one the setup flow's compact line
/// promises ("meteor → 256-px strip → <pin>").
#[test]
fn the_generated_strip_is_the_advertised_length() {
    assert_eq!(DEFAULT_STRIP_PIXELS, 256);
}

fn file_text(files: &[(String, Vec<u8>)], path: &str) -> String {
    files
        .iter()
        .find(|(name, _)| name == path)
        .map(|(_, bytes)| String::from_utf8(bytes.clone()).expect("utf8"))
        .unwrap_or_else(|| panic!("generated package has {path}"))
}

/// Authored endpoints of an output artifact, in channel-key order — parsed
/// through the real node model, not by string search.
fn authored_endpoints(text: &str) -> Vec<String> {
    let NodeDef::Output(output) = NodeDef::from_json_str(text).expect("output artifact parses")
    else {
        panic!("the generated output.json is an Output node");
    };
    let mut channels: Vec<_> = output.channels.entries.iter().collect();
    channels.sort_by_key(|(key, _)| **key);
    channels
        .into_iter()
        .map(|(_, channel)| channel.endpoint.value().as_str().to_string())
        .collect()
}
