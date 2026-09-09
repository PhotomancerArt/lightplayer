//! The four oracles a pattern project is held to, shared by the template
//! gate (`pattern_project_templates.rs`) and the catalog gate
//! (`catalog_pattern_oracles.rs`):
//!
//! 1. every authored file validates against the **checked-in** JSON
//!    Schemas (mapping documents through `lpc-mapping`, which owns that
//!    format);
//! 2. the package loads through the real `ProjectLoader` — the same call
//!    the editor sim and the device make;
//! 3. it survives a round trip through the library: `install_package`
//!    rewrites the manifest on the way in (uid, name), and the authored
//!    `kind`/`exports` must come back out intact;
//! 4. the export lints clean *from the installed copy* — the export lint's
//!    real input is a library snapshot, not the composition's return value.

#![allow(dead_code, reason = "each test binary uses the subset it needs")]

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use jsonschema::Validator;
use lpa_studio_core::app::library::{CatalogOp, LibraryStore, apply_catalog_op};
use lpc_engine::{EngineServices, ProjectLoader};
use lpc_model::{ExportFileSet, ProjectKind, ProjectManifest, TreePath, check_exports};
use lpfs::{LpFs, LpFsMemory, LpPath};
use serde_json::Value;

pub type Files = Vec<(String, Vec<u8>)>;

pub fn workspace_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("lpa-studio-core lives two levels under the workspace root")
        .parent()
        .expect("workspace dir")
        .to_path_buf()
}

pub fn validator(rel: &str) -> Validator {
    let path = workspace_dir().join(rel);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("reading {rel}: {error} — run `just schema-gen`"));
    let schema: Value = serde_json::from_str(&text).expect("schema is JSON");
    jsonschema::draft202012::new(&schema).expect("schema builds a validator")
}

/// The three checked-in schemas a project's files validate against.
pub struct Schemas {
    project: Validator,
    module: Validator,
    node: Validator,
}

impl Schemas {
    pub fn checked_in() -> Self {
        Self {
            project: validator("schemas/project.schema.json"),
            module: validator("schemas/module.schema.json"),
            node: validator("schemas/node.schema.json"),
        }
    }
}

/// Oracle 1: every `.json` file conforms (mapping and patch documents by
/// parsing with `lpc-mapping`). Returns the failures, one line each.
pub fn schema_failures(schemas: &Schemas, label: &str, files: &Files) -> Vec<String> {
    let mut failures = Vec::new();
    for (path, bytes) in files {
        if !path.ends_with(".json") {
            continue;
        }
        let text = std::str::from_utf8(bytes).expect("authored artifacts are utf8");
        if path.ends_with(".patch.json") {
            lpc_mapping::PatchDoc::from_json(text)
                .unwrap_or_else(|error| panic!("{label}/{path}: {error}"));
            continue;
        }
        if path.ends_with(".map2d.json") {
            let doc = lpc_mapping::Map2dDoc::from_json(text)
                .unwrap_or_else(|error| panic!("{label}/{path}: {error}"));
            lpc_mapping::resolve(&doc)
                .unwrap_or_else(|error| panic!("{label}/{path}: does not resolve: {error}"));
            continue;
        }
        if path.ends_with("editor.json") {
            lpc_mapping::EditorMetaDoc::from_json(text)
                .unwrap_or_else(|error| panic!("{label}/{path}: {error}"));
            continue;
        }
        let schema = if path == "project.json" {
            &schemas.project
        } else if path.ends_with("module.json") {
            &schemas.module
        } else {
            &schemas.node
        };
        let instance: Value = serde_json::from_str(text)
            .unwrap_or_else(|error| panic!("{label}/{path}: not JSON: {error}"));
        for error in schema.iter_errors(&instance) {
            failures.push(format!(
                "{label}/{path}: at `{}`: {error}",
                error.instance_path()
            ));
        }
    }
    failures
}

/// Oracle 2: the files load through the SAME loader the sim and the device
/// use. A `render(vec2)` shader referencing a missing `shader.glsl`, an
/// `effect/` module whose mirror publishes nothing, or a fixture bound to a
/// control channel with no output would all surface here.
pub fn assert_loads_through_the_real_loader(label: &str, files: &Files) {
    let fs = LpFsMemory::new();
    for (path, bytes) in files {
        let absolute = format!("/{path}");
        fs.write_file(LpPath::new(&absolute), bytes)
            .unwrap_or_else(|error| panic!("{label}: staging {path}: {error:?}"));
    }
    let services = EngineServices::new(TreePath::parse("/pattern.show").expect("root path"));
    ProjectLoader::load_from_root(&fs, services)
        .unwrap_or_else(|error| panic!("{label} does not load: {error}"));
}

/// Oracle 3, first half: install the files as a new library package and
/// read the installed copy back.
pub fn install_and_read_back(label: &str, name: &str, files: Files) -> Files {
    let fs: Rc<RefCell<dyn LpFs>> = Rc::new(RefCell::new(LpFsMemory::new()));
    let store = LibraryStore::new(
        fs,
        Rc::new(|| [7u8; 16]),
        Rc::new(|| "2026-08-07-0900".to_string()),
    );
    let outcome = apply_catalog_op(
        &store,
        CatalogOp::Create {
            name: name.to_string(),
            files: Some(files),
        },
        1.0,
    )
    .unwrap_or_else(|error| panic!("{label} installs: {error:?}"));
    let summary = outcome.summary.expect("an installed package");
    let handle = store.open(summary.uid).expect("open the new package");
    handle.read_all_files().expect("read back")
}

/// Oracle 3, second half, and oracle 4: the installed copy is still a
/// pattern project exporting `effect`, with a minted uid, and its export
/// lints clean in the shape Studio's own lint sees (`read_all_files`
/// paths, no leading slash).
pub fn assert_installed_copy_is_a_lint_clean_pattern(label: &str, installed: &Files) {
    let manifest_text = file_text(installed, "project.json");
    let manifest = ProjectManifest::read_json(&manifest_text).expect("manifest parses");
    let ProjectKind::Pattern { exports } = manifest.project_kind() else {
        panic!("{label}: the designation must survive installation: {manifest_text}");
    };
    assert_eq!(
        exports,
        vec!["effect".to_string()],
        "{label}: the installed copy exports `effect`: {manifest_text}"
    );
    assert!(
        manifest.uid.is_some(),
        "{label}: the library mints an identity without dropping the kind: {manifest_text}"
    );

    let set: ExportFileSet<'_> = installed
        .iter()
        .filter(|(path, _)| path.starts_with("effect/"))
        .map(|(path, bytes)| (path.as_str(), bytes.as_slice()))
        .collect();
    let report = check_exports(&exports, &set);
    assert!(
        report.is_empty(),
        "{label}: the installed export must lint clean: {:?}",
        report.findings
    );
}

pub fn file_text(files: &Files, path: &str) -> String {
    files
        .iter()
        .find(|(name, _)| name == path)
        .map(|(_, bytes)| String::from_utf8(bytes.clone()).expect("utf8"))
        .unwrap_or_else(|| panic!("installed package has {path}"))
}
