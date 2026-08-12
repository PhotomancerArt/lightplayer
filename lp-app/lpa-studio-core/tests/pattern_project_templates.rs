//! P4 gate: both pattern templates must produce projects that are real,
//! not merely well-formed.
//!
//! The same three oracles the generated board projects are held to
//! (`generated_board_projects.rs`), plus the one that is specific to a
//! library project:
//!
//! 1. every authored file validates against the **checked-in** JSON Schemas
//!    (mapping documents through `lpc-mapping`, which owns that format);
//! 2. the package loads through the real `ProjectLoader` — the same call
//!    the editor sim and the device make, so an unresolvable node ref, a
//!    dangling shader source, or a binding to a channel nobody writes fails
//!    here rather than in front of the author;
//! 3. it survives a round trip through the library: `install_package`
//!    rewrites the manifest on the way in (uid, name), and the authored
//!    `kind`/`exports` must come back out intact;
//! 4. the export lints clean *from the installed copy* — the export lint's
//!    real input is a library snapshot, not the composition's return value.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use jsonschema::Validator;
use lpa_studio_core::app::home::{ProjectTemplate, template_project_files};
use lpc_engine::{EngineServices, ProjectLoader};
use lpc_model::{ExportFileSet, ProjectKind, ProjectManifest, TreePath, check_exports};
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

fn templates() -> [ProjectTemplate; 2] {
    [ProjectTemplate::Pattern1d, ProjectTemplate::Pattern2d]
}

fn files(template: ProjectTemplate) -> Vec<(String, Vec<u8>)> {
    template_project_files(template)
        .unwrap_or_else(|error| panic!("{template:?}: {error:?}"))
        .unwrap_or_else(|| panic!("{template:?} generates files"))
}

#[test]
fn every_pattern_template_validates_against_the_checked_in_schemas() {
    let project_schema = validator("schemas/project.schema.json");
    let module_schema = validator("schemas/module.schema.json");
    let node_schema = validator("schemas/node.schema.json");

    let mut failures = Vec::new();
    for template in templates() {
        for (path, bytes) in &files(template) {
            if !path.ends_with(".json") {
                continue;
            }
            let text = std::str::from_utf8(bytes).expect("authored artifacts are utf8");
            if path.ends_with(".patch.json") {
                lpc_mapping::PatchDoc::from_json(text)
                    .unwrap_or_else(|error| panic!("{template:?}/{path}: {error}"));
                continue;
            }
            if path.ends_with(".map2d.json") {
                let doc = lpc_mapping::Map2dDoc::from_json(text)
                    .unwrap_or_else(|error| panic!("{template:?}/{path}: {error}"));
                lpc_mapping::resolve(&doc).unwrap_or_else(|error| {
                    panic!("{template:?}/{path}: does not resolve: {error}")
                });
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
                .unwrap_or_else(|error| panic!("{template:?}/{path}: not JSON: {error}"));
            for error in schema.iter_errors(&instance) {
                failures.push(format!(
                    "{template:?}/{path}: at `{}`: {error}",
                    error.instance_path()
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} template file(s) failed schema conformance:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// The oracle that matters most: the template loads through the SAME
/// loader the sim and the device use. A `render(vec2)` shader referencing
/// a missing `shader.glsl`, an `effect/` module whose mirror publishes
/// nothing, or a fixture bound to a control channel with no output would
/// all surface here.
#[test]
fn every_pattern_template_loads_through_the_real_loader() {
    for template in templates() {
        let fs = LpFsMemory::new();
        for (path, bytes) in &files(template) {
            let absolute = format!("/{path}");
            fs.write_file(LpPath::new(&absolute), bytes)
                .unwrap_or_else(|error| panic!("{template:?}: staging {path}: {error:?}"));
        }
        let services = EngineServices::new(TreePath::parse("/pattern.show").expect("root path"));
        ProjectLoader::load_from_root(&fs, services)
            .unwrap_or_else(|error| panic!("{template:?} does not load: {error}"));
    }
}

/// The library round trip: creating from a template installs the authored
/// files verbatim, and the `kind`/`exports` the manifest carries survive
/// the uid/name rewrite `install_package` does on the way in. Without this
/// the New menu would quietly hand back a general project.
#[test]
fn creating_from_a_template_installs_a_pre_designated_pattern_project() {
    use lpa_studio_core::app::library::{CatalogOp, LibraryStore, apply_catalog_op};

    for template in templates() {
        let fs: Rc<RefCell<dyn LpFs>> = Rc::new(RefCell::new(LpFsMemory::new()));
        let store = LibraryStore::new(
            fs,
            Rc::new(|| [7u8; 16]),
            Rc::new(|| "2026-08-07-0900".to_string()),
        );
        let outcome = apply_catalog_op(
            &store,
            CatalogOp::Create {
                name: template.default_project_name().to_string(),
                files: Some(files(template)),
            },
            1.0,
        )
        .unwrap_or_else(|error| panic!("{template:?} installs: {error:?}"));
        let summary = outcome.summary.expect("an installed package");

        let handle = store.open(summary.uid).expect("open the new package");
        let installed = handle.read_all_files().expect("read back");

        let manifest_text = file_text(&installed, "project.json");
        let manifest = ProjectManifest::read_json(&manifest_text).expect("manifest parses");
        assert_eq!(
            manifest.project_kind(),
            ProjectKind::Pattern {
                exports: vec!["effect".to_string()]
            },
            "{template:?}: the designation must survive installation: {manifest_text}"
        );
        assert!(
            manifest.uid.is_some(),
            "{template:?}: the library mints an identity without dropping the kind: \
             {manifest_text}"
        );

        // 4. the export lints clean from the INSTALLED copy — the shape
        // Studio's own lint sees (`read_all_files` paths, no leading slash).
        let set: ExportFileSet<'_> = installed
            .iter()
            .filter(|(path, _)| path.starts_with("effect/"))
            .map(|(path, bytes)| (path.as_str(), bytes.as_slice()))
            .collect();
        let report = check_exports(&["effect".to_string()], &set);
        assert!(
            report.is_empty(),
            "{template:?}: the installed export must lint clean: {:?}",
            report.findings
        );
    }
}

/// A blank create still sends no files, so the store's own scaffold — the
/// minimal manifest plus the one-line root module — is what a blank
/// project has always been.
#[test]
fn the_blank_template_still_takes_the_stores_own_scaffold() {
    assert_eq!(
        template_project_files(ProjectTemplate::Blank).unwrap(),
        None
    );
}

fn file_text(files: &[(String, Vec<u8>)], path: &str) -> String {
    files
        .iter()
        .find(|(name, _)| name == path)
        .map(|(_, bytes)| String::from_utf8(bytes.clone()).expect("utf8"))
        .unwrap_or_else(|| panic!("installed package has {path}"))
}
