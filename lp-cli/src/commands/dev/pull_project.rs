//! Pull project files from server
//!
//! Provides async function to pull project files from the server to local filesystem.

use anyhow::{Context, Result};
use lp_model::AsLpPath;
use lp_shared::fs::LpFs;

use crate::client::LpClient;

/// Pull project files from server to local filesystem
///
/// Recursively lists all files in the project on the server and writes
/// them to the local filesystem.
///
/// # Arguments
///
/// * `client` - Async client for communicating with server
/// * `local_fs` - Local filesystem (project root)
/// * `project_uid` - Project UID (used for server-side project path)
///
/// # Returns
///
/// * `Ok(())` if all files were pulled successfully
/// * `Err` if any file operation failed
#[allow(dead_code, reason = "Reserved for future pull functionality")]
pub async fn pull_project_async(
    client: &LpClient,
    local_fs: &dyn LpFs,
    project_uid: &str,
) -> Result<()> {
    // Build server project path
    let server_project_path = format!("/projects/{project_uid}");

    // List all files recursively in the project on server
    let files = client
        .fs_list_dir(server_project_path.as_path(), true)
        .await
        .with_context(|| format!("Failed to list files in project: {server_project_path}"))?;

    // Pull each file from the server
    for file_path in files {
        // Read file from server
        let data = client
            .fs_read(file_path.as_path())
            .await
            .with_context(|| format!("Failed to read file from server: {}", file_path.as_str()))?;

        // Extract local path by removing the "/projects/{project_uid}/" prefix
        let file_path_str = file_path.as_str();
        let prefix_with_slash = format!("/projects/{project_uid}/");
        let prefix_without_slash = format!("projects/{project_uid}/");
        let local_path = if file_path_str.starts_with(&prefix_with_slash) {
            // Remove prefix and ensure it starts with '/'
            let relative = &file_path_str[prefix_with_slash.len()..];
            if relative.starts_with('/') {
                relative.to_string()
            } else {
                format!("/{relative}")
            }
        } else if file_path_str.starts_with(&prefix_without_slash) {
            // Handle paths without leading slash (backward compatibility)
            let relative = &file_path_str[prefix_without_slash.len()..];
            if relative.starts_with('/') {
                relative.to_string()
            } else {
                format!("/{relative}")
            }
        } else if file_path_str == format!("/projects/{project_uid}")
            || file_path_str == format!("projects/{project_uid}")
        {
            // This is the project directory itself, skip
            continue;
        } else {
            // Unexpected path format, skip with warning
            eprintln!("Warning: Unexpected file path format: {file_path_str}");
            continue;
        };

        // Write file to local filesystem
        local_fs
            .write_file(local_path.as_path(), &data)
            .map_err(|e| {
                anyhow::anyhow!("Failed to write file to local filesystem {local_path}: {e}")
            })?;
    }

    Ok(())
}
