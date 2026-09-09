use anyhow::{Context, Result};
use lpc_engine::{EngineServices, ProjectLoader};
use lpc_model::TreePath;
use lpfs::LpFsStd;
use std::path::{Path, PathBuf};

#[test]
fn checked_in_catalog_entries_load_as_core_projects() -> Result<()> {
    let workspace_dir = workspace_dir();
    let project_dirs = checked_in_project_dirs(&workspace_dir, GATE_ROOTS)?;

    let mut failures = Vec::new();
    for project_dir in project_dirs {
        let fs = LpFsStd::new(project_dir.clone());
        let rel = project_dir
            .strip_prefix(&workspace_dir)
            .unwrap_or(&project_dir);
        let root_path = example_root_path(rel)?;
        let services = EngineServices::new(root_path);

        if let Err(err) = ProjectLoader::load_from_root(&fs, services) {
            failures.push(format!("{}: {err}", rel.display()));
        }
    }

    if !failures.is_empty() {
        anyhow::bail!(
            "checked-in projects failed to load:\n{}",
            failures.join("\n")
        );
    }

    Ok(())
}

#[test]
fn checked_in_catalog_entries_rewrite_byte_identically() -> Result<()> {
    // Mitosis invariant: loading and re-writing an unchanged project
    // produces identical bytes for BOTH split files — the container
    // manifest through `ProjectManifest::write_json`, the root module
    // through the canonical slot writer.
    use lpc_model::{NodeDef, ProjectManifest, SlotShapeRegistry};

    let workspace_dir = workspace_dir();
    let project_dirs = checked_in_project_dirs(&workspace_dir, GATE_ROOTS)?;
    let registry = SlotShapeRegistry::default();

    let mut failures = Vec::new();
    for project_dir in project_dirs {
        let rel = project_dir
            .strip_prefix(&workspace_dir)
            .unwrap_or(&project_dir)
            .display()
            .to_string();

        let manifest_text = std::fs::read_to_string(project_dir.join("project.json"))
            .with_context(|| format!("{rel}: read project.json"))?;
        match ProjectManifest::read_json(&manifest_text) {
            Ok(manifest) if manifest.write_json() != manifest_text => {
                failures.push(format!("{rel}: project.json is not canonical"));
            }
            Ok(_) => {}
            Err(err) => failures.push(format!("{rel}: project.json: {err}")),
        }

        let module_text = std::fs::read_to_string(project_dir.join("module.json"))
            .with_context(|| format!("{rel}: read module.json"))?;
        match NodeDef::read_json(&registry, &module_text) {
            Ok(def) => match def.write_json(&registry) {
                Ok(rewritten) if rewritten != module_text => {
                    failures.push(format!("{rel}: module.json is not canonical"));
                }
                Ok(_) => {}
                Err(err) => failures.push(format!("{rel}: module.json rewrite: {err}")),
            },
            Err(err) => failures.push(format!("{rel}: module.json: {err}")),
        }
    }

    if !failures.is_empty() {
        anyhow::bail!(
            "checked-in projects failed the byte-identity rewrite:\n{}",
            failures.join("\n")
        );
    }
    Ok(())
}

/// Every checked-in project under `roots`, recursively (the catalog nests
/// entries one bucket deep: `catalog/<bucket>/<slug>`). Every root must
/// contribute at least one project — a wrong path would otherwise make a
/// gate vacuous.
fn checked_in_project_dirs(workspace_dir: &Path, roots: &[&str]) -> Result<Vec<PathBuf>> {
    let mut project_dirs = Vec::new();
    for root in roots {
        let before = project_dirs.len();
        collect_project_dirs(&workspace_dir.join(root), &mut project_dirs)?;
        assert!(
            project_dirs.len() > before,
            "expected at least one checked-in project under {root}/"
        );
    }
    project_dirs.sort();
    Ok(project_dirs)
}

/// Roots both gates walk: the catalog (the content Studio embeds) and
/// every test rig under `projects/test/` — canonical since PR #543.
const GATE_ROOTS: &[&str] = &["catalog", "projects/test"];

fn workspace_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace dir")
        .to_path_buf()
}

fn collect_project_dirs(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir).with_context(|| format!("read {}", dir.display()))? {
        let entry = entry.with_context(|| format!("read entry in {}", dir.display()))?;
        let path = entry.path();
        if path.is_dir() {
            if path.join("project.json").is_file() {
                out.push(path);
            } else {
                collect_project_dirs(&path, out)?;
            }
        }
    }
    Ok(())
}

fn example_root_path(relative_dir: &Path) -> Result<TreePath> {
    let name = relative_dir
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("_")
        .replace('-', "_");
    TreePath::parse(&format!("/{name}.show"))
        .map_err(|err| anyhow::anyhow!("example root path for {}: {err}", relative_dir.display()))
}
