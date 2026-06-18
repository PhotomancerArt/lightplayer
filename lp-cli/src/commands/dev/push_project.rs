//! Push project files to server
//!
//! Provides async function to push local project files to the server.

use anyhow::{Context, Result};
use lpa_client::ProjectDeployFile;
use lpc_model::AsLpPath;
use lpc_wire::WireProjectHandle;
use lpfs::LpFs;

use crate::client::LpClient;

/// Push project files from local filesystem to server
///
/// Recursively reads all files from the local project directory and writes
/// them to the server filesystem.
///
/// # Arguments
///
/// * `client` - Async client for communicating with server
/// * `local_fs` - Local filesystem (project root)
/// * `project_uid` - Project UID (used for server-side project path)
///
/// # Returns
///
/// * `Ok(())` if all files were pushed successfully
/// * `Err` if any file operation failed
#[allow(
    dead_code,
    reason = "Write-only project sync is retained for file-watch and future partial deploy callers"
)]
pub async fn push_project_async(
    client: &LpClient,
    local_fs: &dyn LpFs,
    project_uid: &str,
) -> Result<()> {
    let files = collect_project_deploy_files(local_fs)?;
    client
        .push_project_files(project_uid, files)
        .await
        .with_context(|| format!("Failed to push project files for {project_uid}"))?;
    Ok(())
}

/// Stop any loaded projects, push project files, and load the project.
pub async fn deploy_project_async(
    client: &LpClient,
    local_fs: &dyn LpFs,
    project_uid: &str,
) -> Result<WireProjectHandle> {
    let files = collect_project_deploy_files(local_fs)?;
    client
        .deploy_project_files(project_uid, files)
        .await
        .with_context(|| format!("Failed to deploy project {project_uid}"))
}

fn collect_project_deploy_files(local_fs: &dyn LpFs) -> Result<Vec<ProjectDeployFile>> {
    // List all files recursively in the project directory
    let entries = local_fs
        .list_dir("/".as_path(), true)
        .map_err(|e| anyhow::anyhow!("Failed to list project files: {e}"))?;

    let mut files = Vec::new();

    // Push each file to the server (skip directories)
    for entry_path in entries {
        // Skip directories - check if it's a directory before trying to read
        match local_fs.is_dir(entry_path.as_path()) {
            Ok(true) => {
                // It's a directory, skip it (directories are created implicitly when files are written)
                continue;
            }
            Ok(false) => {
                // It's a file, proceed to read and push
            }
            Err(_) => {
                // If we can't determine, try to read it anyway (might be a file)
            }
        }

        // Read file from local filesystem
        let entry_str = entry_path.as_str();
        let data = match local_fs.read_file(entry_path.as_path()) {
            Ok(data) => data,
            Err(e) => {
                // If read fails and it's because it's a directory, skip it
                if entry_str.ends_with('/')
                    || local_fs.is_dir(entry_path.as_path()).unwrap_or(false)
                {
                    continue;
                }
                return Err(anyhow::anyhow!("Failed to read file {entry_str}: {e}"));
            }
        };

        // Build server path: /projects/{project_uid}/{entry_path}
        // Remove leading '/' from entry_path for server path, then prepend /projects/{project_uid}/
        let relative_path = if let Some(stripped) = entry_str.strip_prefix('/') {
            stripped
        } else {
            entry_str
        };
        files.push(ProjectDeployFile::new(relative_path, data));
    }

    Ok(files)
}
