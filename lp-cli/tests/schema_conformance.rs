//! Corpus conformance: every authored artifact checked into the repo must
//! validate against the **checked-in** JSON Schemas under `schemas/` (not
//! freshly generated ones — the checked-in files are what users and IDEs
//! consume; `just schema-check` keeps them fresh).
//!
//! Corpus layout (see P1 of the schema/shape-gen hygiene plan):
//!
//! - [`ARTIFACT_ROOTS`] hold authored projects: `project.json` container
//!   manifests validate against `schemas/project.schema.json`, `module.json`
//!   root modules validate against `schemas/module.schema.json`,
//!   `*.map2d.json` files are fixture mapping documents, `*.patch.json`
//!   files are fixture patch documents, and a project-level `editor.json`
//!   is the editor-metadata document — all three validate by parsing with
//!   the real `lpc-mapping` parser (which owns those formats) — and every
//!   other `*.json` is a node artifact and validates against
//!   `schemas/node.schema.json`. Non-JSON
//!   files (`.glsl`, `.svg`, ...) are not artifacts and are ignored by
//!   extension.
//! - [`HARDWARE_ROOTS`] hold board manifests (`HardwareManifestFile`), which
//!   validate against `schemas/hardware.schema.json`. The repo-root
//!   `profiles/` directory is CPU-profiler output (speedscope dumps), not
//!   hardware profiles, and is outside every walked root on purpose.
//!
//! Any `.json` under a walked root that is *not* an artifact must be listed
//! in [`SKIP_JSON`] — there are no silent exclusions, and the walk fails if a
//! skip entry stops matching so the list cannot rot.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde_json::Value;

/// Workspace-relative roots that hold authored project/node artifacts.
const ARTIFACT_ROOTS: &[&str] = &["projects", "examples", "lp-fw/fw-browser/www/smoke-project"];

/// Workspace-relative roots that hold hardware board manifests.
const HARDWARE_ROOTS: &[&str] = &["lp-core/lpc-hardware/boards"];

/// Explicit skip list: workspace-relative paths of `.json` files under the
/// walked roots that are not authored artifacts (e.g. studio fixtures).
/// Currently every `.json` in the corpus is an artifact, so the list is
/// empty; add entries here (never silent glob tweaks) if that changes.
const SKIP_JSON: &[&str] = &[];

#[test]
fn authored_artifacts_conform_to_checked_in_schemas() -> Result<()> {
    let workspace = workspace_dir();
    let project_validator = load_validator(&workspace, "schemas/project.schema.json")?;
    let module_validator = load_validator(&workspace, "schemas/module.schema.json")?;
    let node_validator = load_validator(&workspace, "schemas/node.schema.json")?;
    assert_skip_list_matches(&workspace)?;

    let mut failures = Vec::new();
    for root in ARTIFACT_ROOTS {
        let files = walk_json_files(&workspace, root)?;
        // Vacuity guard: a root with zero artifacts means the walk (or the
        // corpus layout assumption) is silently wrong, not that all is well.
        assert!(
            !files.is_empty(),
            "no artifact JSON found under {root}/ — walk is vacuous"
        );
        let mut projects = 0usize;
        let mut modules = 0usize;
        let mut nodes = 0usize;
        let mut mappings = 0usize;
        let mut patches = 0usize;
        let mut editor_docs = 0usize;
        for file in &files {
            let rel = relative(&workspace, file);
            if SKIP_JSON.contains(&rel.as_str()) {
                continue;
            }
            if rel.ends_with(".map2d.json") {
                mappings += 1;
                validate_map2d_file(file, &rel, &mut failures)?;
                continue;
            }
            if rel.ends_with(".patch.json") {
                patches += 1;
                validate_patch_file(file, &rel, &mut failures)?;
                continue;
            }
            if file.file_name().is_some_and(|name| name == "editor.json") {
                editor_docs += 1;
                validate_editor_meta_file(file, &rel, &mut failures)?;
                continue;
            }
            let validator = if file.file_name().is_some_and(|name| name == "project.json") {
                projects += 1;
                &project_validator
            } else if file.file_name().is_some_and(|name| name == "module.json") {
                modules += 1;
                &module_validator
            } else {
                nodes += 1;
                &node_validator
            };
            validate_file(validator, file, &rel, &mut failures)?;
        }
        println!(
            "{root}: validated {projects} container manifests + {modules} module roots + {nodes} node artifacts + {mappings} mapping documents + {patches} patch documents + {editor_docs} editor documents"
        );
    }

    if !failures.is_empty() {
        bail!(
            "{} artifact(s) failed schema conformance:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }
    Ok(())
}

#[test]
fn hardware_manifests_conform_to_checked_in_schema() -> Result<()> {
    let workspace = workspace_dir();
    let manifest_validator = load_validator(&workspace, "schemas/hardware.schema.json")?;
    let display_validator = load_validator(&workspace, "schemas/board-display.schema.json")?;

    let mut failures = Vec::new();
    let mut display_count = 0usize;
    for root in HARDWARE_ROOTS {
        let files = walk_json_files(&workspace, root)?;
        assert!(
            !files.is_empty(),
            "no hardware manifest JSON found under {root}/ — walk is vacuous"
        );
        for file in &files {
            let rel = relative(&workspace, file);
            if SKIP_JSON.contains(&rel.as_str()) {
                continue;
            }
            // `<product>.display.json` sidecars are catalog metadata
            // (lpa-boards) with their own schema.
            if rel.ends_with(".display.json") {
                display_count += 1;
                validate_file(&display_validator, file, &rel, &mut failures)?;
            } else {
                validate_file(&manifest_validator, file, &rel, &mut failures)?;
            }
        }
        println!("{root}: validated {} hardware manifests", files.len());
    }
    // Vacuity guard for the sidecar branch too.
    assert!(
        display_count > 0,
        "no display sidecars found under the hardware roots — walk is vacuous"
    );

    if !failures.is_empty() {
        bail!(
            "{} hardware manifest(s) failed schema conformance:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }
    Ok(())
}

fn workspace_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace dir")
        .to_path_buf()
}

/// Build a validator from a checked-in schema file.
fn load_validator(workspace: &Path, rel: &str) -> Result<jsonschema::Validator> {
    let path = workspace.join(rel);
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("reading {rel} — run `just schema-gen` if missing"))?;
    let schema: Value = serde_json::from_str(&text).with_context(|| format!("parsing {rel}"))?;
    jsonschema::draft202012::new(&schema)
        .map_err(|error| anyhow::anyhow!("building validator for {rel}: {error}"))
}

/// Every `.json` file under `workspace/root`, recursively, sorted.
fn walk_json_files(workspace: &Path, root: &str) -> Result<Vec<PathBuf>> {
    let dir = workspace.join(root);
    assert!(dir.is_dir(), "corpus root {root}/ does not exist");
    let mut files = Vec::new();
    collect_json_files(&dir, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_json_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir).with_context(|| format!("read {}", dir.display()))? {
        let path = entry
            .with_context(|| format!("read entry in {}", dir.display()))?
            .path();
        if path.is_dir() {
            collect_json_files(&path, out)?;
        } else if path.extension().is_some_and(|ext| ext == "json") {
            out.push(path);
        }
    }
    Ok(())
}

/// Validate one file, pushing `path: instance-path: error` lines on failure.
fn validate_file(
    validator: &jsonschema::Validator,
    file: &Path,
    rel: &str,
    failures: &mut Vec<String>,
) -> Result<()> {
    let text = std::fs::read_to_string(file).with_context(|| format!("reading {rel}"))?;
    let instance: Value = match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(error) => {
            failures.push(format!("{rel}: not valid JSON: {error}"));
            return Ok(());
        }
    };
    for error in validator.iter_errors(&instance) {
        failures.push(format!("{rel}: at `{}`: {error}", error.instance_path()));
    }
    Ok(())
}

/// Validate a `*.map2d.json` mapping document with the owning parser —
/// stricter than a JSON Schema: it enforces the format gate and resolves.
fn validate_map2d_file(file: &Path, rel: &str, failures: &mut Vec<String>) -> Result<()> {
    let text = std::fs::read_to_string(file).with_context(|| format!("reading {rel}"))?;
    match lpc_mapping::Map2dDoc::from_json(&text) {
        Ok(doc) => {
            if let Err(error) = lpc_mapping::resolve(&doc) {
                failures.push(format!("{rel}: does not resolve: {error}"));
            }
        }
        Err(error) => failures.push(format!("{rel}: {error}")),
    }
    Ok(())
}

/// Patch documents validate by PARSING with the real `lpc-mapping` parser,
/// which owns that format. They are not resolved here: resolution is
/// fixture-relative (it needs the lamp count the fixture's mapping produces),
/// and the document alone does not carry it — the loader does that, and
/// reports it on the fixture.
fn validate_patch_file(file: &Path, rel: &str, failures: &mut Vec<String>) -> Result<()> {
    let text = std::fs::read_to_string(file).with_context(|| format!("reading {rel}"))?;
    if let Err(error) = lpc_mapping::PatchDoc::from_json(&text) {
        failures.push(format!("{rel}: {error}"));
    }
    Ok(())
}

/// The project-level `editor.json` validates by PARSING with the real
/// `lpc-mapping` parser, which owns that format — the same rule mapping and
/// patch documents follow, and for the same reason: the parser is the
/// format's definition, not a generated schema.
///
/// It is NOT a node artifact: it carries per-node editor presentation state
/// (Arrange placements) and is deliberately absent from the node schema, so
/// the node validator refuses it. An example may ship one — `examples/small-dome`
/// arranges its dome and door in one plan — which is what surfaced this
/// classification gap.
fn validate_editor_meta_file(file: &Path, rel: &str, failures: &mut Vec<String>) -> Result<()> {
    let text = std::fs::read_to_string(file).with_context(|| format!("reading {rel}"))?;
    if let Err(error) = lpc_mapping::EditorMetaDoc::from_json(&text) {
        failures.push(format!("{rel}: {error}"));
    }
    Ok(())
}

fn relative(workspace: &Path, file: &Path) -> String {
    file.strip_prefix(workspace)
        .unwrap_or(file)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Every [`SKIP_JSON`] entry must still exist under a walked root; a stale
/// entry means the skip list has rotted.
fn assert_skip_list_matches(workspace: &Path) -> Result<()> {
    for rel in SKIP_JSON {
        let path = workspace.join(rel);
        assert!(path.is_file(), "SKIP_JSON entry {rel} does not exist");
        assert!(
            ARTIFACT_ROOTS
                .iter()
                .chain(HARDWARE_ROOTS)
                .any(|root| Path::new(rel).starts_with(root)),
            "SKIP_JSON entry {rel} is not under any walked root"
        );
    }
    Ok(())
}
