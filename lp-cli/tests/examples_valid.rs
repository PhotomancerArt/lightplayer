use anyhow::{Context, Result};
use lpc_engine::{EngineServices, ProjectLoader};
use lpc_model::TreePath;
use lpfs::LpFsStd;
use std::path::{Path, PathBuf};

#[test]
fn checked_in_examples_load_as_core_projects() -> Result<()> {
    let workspace_dir = workspace_dir();
    let examples_dir = workspace_dir.join("examples");
    let mut project_dirs = Vec::new();
    collect_project_dirs(&examples_dir, &mut project_dirs)?;
    project_dirs.sort();

    assert!(
        !project_dirs.is_empty(),
        "expected at least one checked-in example project"
    );

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
            "checked-in example projects failed to load:\n{}",
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

fn collect_project_dirs(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir).with_context(|| format!("read {}", dir.display()))? {
        let entry = entry.with_context(|| format!("read entry in {}", dir.display()))?;
        let path = entry.path();
        if path.is_dir() {
            if path.join("project.toml").is_file() {
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
