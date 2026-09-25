//! Project validation utilities shared by dev and upload commands.

use anyhow::{Context, Result};
use lpc_engine::{EngineServices, ProjectLoader};
use lpc_model::{AsLpPath, TreePath};
use lpfs::{LpFs, LpFsStd};
use std::path::PathBuf;

/// Validate that a local project exists and extract project info.
///
/// The first return value is the remote project directory key. Older projects
/// may still carry `uid`; current project artifacts use `name`, then fall back
/// to the local directory name.
pub fn validate_local_project(project_dir: &PathBuf) -> Result<(String, String)> {
    let fs = LpFsStd::new(project_dir.clone());

    let data = fs.read_file("/project.json".as_path()).map_err(|e| {
        anyhow::anyhow!(
            "Failed to read project.json from: {}\n\
             Error: {}\n\
             Make sure you're in a project directory or specify the project directory",
            project_dir.display(),
            e
        )
    })?;

    let text = std::str::from_utf8(&data).context("project.json is not UTF-8")?;
    let config: serde_json::Value = serde_json::from_str(text).with_context(|| {
        format!(
            "Failed to parse project.json from: {}",
            project_dir.display()
        )
    })?;

    let name = config
        .get("name")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            project_dir
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| String::from("project"));
    let project_key = config
        .get("uid")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| name.clone());
    Ok((project_key, name))
}

/// Check every entry of every playlist (D19), not just the one entry a
/// device loads: a device only finds a broken dormant pattern when it is
/// picked, and the host has every file. Refuses (`Err`) when the project
/// root itself fails to load — the same hard failure `lp-cli upload`
/// already reported, now caught before the files leave the host — or lists
/// every entry whose def failed to load, by display name.
///
/// Used by `upload`'s pre-deploy check, which refuses to upload on either
/// case; a host that only wants to warn (Studio's edit/save path) uses
/// `lpc_registry::ProjectRegistry::entry_issues` directly against its own
/// live registry instead of loading a fresh one from disk.
pub fn check_every_entry(project_dir: &PathBuf) -> Result<Vec<String>> {
    let fs = LpFsStd::new(project_dir.clone());
    // An arbitrary, fixed anchor: this loader instance never ticks or
    // resolves against a real show tree, so the path's content doesn't
    // matter, only that it parses.
    let root_path = TreePath::parse("/upload_check.show")
        .expect("static anchor path is a valid TreePath");
    let services = EngineServices::new(root_path);

    let runtime = ProjectLoader::load_from_root_with_every_entry_resident(&fs, services)
        .with_context(|| {
            format!(
                "project at {} failed to load: every playlist entry is checked before upload",
                project_dir.display()
            )
        })?;

    Ok(runtime
        .registry()
        .entry_issues()
        .iter()
        .map(|issue| issue.display())
        .collect())
}
