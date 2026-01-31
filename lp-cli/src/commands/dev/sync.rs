//! File sync utilities
//!
//! Provides functions for syncing individual file changes to the server.

use anyhow::{Context, Result};
use lp_model::AsLpPath;
use lp_shared::fs::{LpFs, fs_event::ChangeType, fs_event::FsChange};
use std::sync::Arc;

use crate::client::LpClient;

/// Sync a file change to the server
///
/// Reads the file from local filesystem (if needed) and writes/deletes it on the server.
///
/// # Arguments
///
/// * `client` - Async client for communicating with server
/// * `change` - File change event to sync
/// * `project_uid` - Project UID for server-side path
/// * `local_fs` - Local filesystem for reading files (wrapped in Arc for Send + Sync)
///
/// # Returns
///
/// * `Ok(())` if the change was synced successfully
/// * `Err` if syncing failed
pub async fn sync_file_change(
    client: &Arc<LpClient>,
    change: &FsChange,
    project_uid: &str,
    _project_dir: &std::path::Path,
    local_fs: &Arc<dyn LpFs + Send + Sync>,
) -> Result<()> {
    // Build server path: /projects/{project_uid}/{file_path}
    // Remove leading '/' from change.path for server path, then prepend /projects/{project_uid}/
    let path_str = change.path.as_str();
    let relative_path = if let Some(stripped) = path_str.strip_prefix('/') {
        stripped
    } else {
        path_str
    };
    let server_path = format!("/projects/{project_uid}/{relative_path}");

    match change.change_type {
        ChangeType::Create | ChangeType::Modify => {
            // Check if file still exists (it might have been deleted by the time we sync)
            if !local_fs.file_exists(change.path.as_path()).unwrap_or(false) {
                // File doesn't exist anymore, skip sync (likely a temporary file)
                return Ok(());
            }

            // Read file from local filesystem
            let data = local_fs.read_file(change.path.as_path()).map_err(|e| {
                anyhow::anyhow!("Failed to read file {}: {}", change.path.as_str(), e)
            })?;

            // Write file to server
            client
                .fs_write(server_path.as_path(), data)
                .await
                .with_context(|| format!("Failed to write file to server: {server_path}"))?;
        }
        ChangeType::Delete => {
            // Delete file from server
            client
                .fs_delete_file(server_path.as_path())
                .await
                .with_context(|| format!("Failed to delete file on server: {server_path}"))?;
        }
    }

    Ok(())
}
