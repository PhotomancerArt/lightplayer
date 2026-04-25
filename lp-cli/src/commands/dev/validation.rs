//! Project validation utilities shared by dev and upload commands.

use anyhow::{Context, Result};
use lp_model::{AsLpPath, project::ProjectConfig};
use lpfs::{LpFs, LpFsStd};
use std::path::PathBuf;

/// Validate that a local project exists and extract project info
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

    let config: ProjectConfig = serde_json::from_slice(&data).with_context(|| {
        format!(
            "Failed to parse project.json from: {}",
            project_dir.display()
        )
    })?;

    Ok((config.uid.clone(), config.name.clone()))
}
