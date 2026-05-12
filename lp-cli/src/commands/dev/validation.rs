//! Project validation utilities shared by dev and upload commands.

use anyhow::{Context, Result};
use lpc_model::AsLpPath;
use lpfs::{LpFs, LpFsStd};
use std::path::PathBuf;

/// Validate that a local project exists and extract project info.
///
/// The first return value is the remote project directory key. Older projects
/// may still carry `uid`; current project artifacts use `name`, then fall back
/// to the local directory name.
pub fn validate_local_project(project_dir: &PathBuf) -> Result<(String, String)> {
    let fs = LpFsStd::new(project_dir.clone());

    let data = fs.read_file("/project.toml".as_path()).map_err(|e| {
        anyhow::anyhow!(
            "Failed to read project.toml from: {}\n\
             Error: {}\n\
             Make sure you're in a project directory or specify the project directory",
            project_dir.display(),
            e
        )
    })?;

    let text = std::str::from_utf8(&data).context("project.toml is not UTF-8")?;
    let config: toml::Value = toml::from_str(text).with_context(|| {
        format!(
            "Failed to parse project.toml from: {}",
            project_dir.display()
        )
    })?;

    let name = config
        .get("name")
        .and_then(toml::Value::as_str)
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
        .and_then(toml::Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| name.clone());
    Ok((project_key, name))
}
