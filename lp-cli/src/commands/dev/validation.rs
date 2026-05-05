//! Project validation utilities shared by dev and upload commands.

use anyhow::{Context, Result};
use lpc_model::AsLpPath;
use lpfs::{LpFs, LpFsStd};
use std::path::PathBuf;

/// Validate that a local project exists and extract project info
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

    let uid = config
        .get("uid")
        .and_then(toml::Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    let name = config
        .get("name")
        .and_then(toml::Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    Ok((uid, name))
}
