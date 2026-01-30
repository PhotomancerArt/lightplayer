//! Filesystem watching and syncing loop
//!
//! Monitors file changes in the project directory and syncs them to the server.

use anyhow::{Context, Result};
use lp_shared::fs::{LpFs, fs_event::FsChange};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::sleep;

use crate::client::LpClient;
use crate::commands::dev::{sync::sync_file_change, watcher::FileWatcher};

/// Debounce duration for file changes (500ms)
pub const DEBOUNCE_DURATION: Duration = Duration::from_millis(500);

/// Filesystem watching and syncing loop
///
/// Monitors file changes in the project directory and syncs them to the server.
/// Uses debouncing to batch multiple rapid changes together.
///
/// # Arguments
///
/// * `transport` - Shared client transport (Arc<Mutex<Box<dyn ClientTransport>>>)
/// * `project_dir` - Local project directory path
/// * `project_uid` - Project UID for server-side path
/// * `local_fs` - Local filesystem for reading files (wrapped in Arc for sharing)
///
/// # Returns
///
/// * `Ok(())` if the loop completed successfully
/// * `Err` if an unrecoverable error occurred
pub async fn fs_loop(
    transport: Arc<tokio::sync::Mutex<Box<dyn crate::client::transport::ClientTransport>>>,
    project_dir: PathBuf,
    project_uid: String,
    local_fs: Arc<dyn LpFs + Send + Sync>,
) -> Result<()> {
    // Create LpClient with shared transport
    let client = Arc::new(LpClient::new_shared(transport));

    // Create file watcher
    let mut watcher =
        FileWatcher::new(project_dir.clone()).context("Failed to create file watcher")?;

    // Debouncing state
    let mut pending_changes: HashMap<String, FsChange> = HashMap::new();
    let mut last_change_time: Option<Instant> = None;

    // Main loop
    loop {
        // Wait for file change or timeout (for debounce checking)
        tokio::select! {
            // Receive file change event
            change = watcher.next_change() => {
                if let Some(change) = change {
                    add_pending_change(&mut pending_changes, &mut last_change_time, change);
                } else {
                    // Channel closed, exit loop
                    eprintln!("File watcher channel closed, exiting fs_loop");
                    break;
                }
            }
            // Timeout for debounce checking
            _ = sleep(Duration::from_millis(50)) => {
                // Check if debounce period has passed
            }
        }

        // Check if debounce period has passed
        let should_sync = if let Some(last_time) = last_change_time {
            last_time.elapsed() >= DEBOUNCE_DURATION && !pending_changes.is_empty()
        } else {
            false
        };

        if should_sync {
            // Sync all pending changes
            let changes: Vec<FsChange> = pending_changes.values().cloned().collect();
            pending_changes.clear();
            last_change_time = None;

            // Note: Changes are tracked on the server side when files are synced via fs_write.
            // Client-side LpFsStd change tracking would require thread-safe access (Mutex instead of RefCell),
            // which is a larger refactoring. For now, we rely on server-side tracking.

            for change in &changes {
                // Sync each change to server
                if let Err(e) =
                    sync_file_change(&client, change, &project_uid, &project_dir, &local_fs).await
                {
                    eprintln!("Failed to sync file change {}: {}", change.path.as_str(), e);
                    // Continue with other changes even if one fails
                }
            }
        }
    }

    Ok(())
}

/// Add a file change to the pending changes list
///
/// Deduplicates changes by path (later changes override earlier ones).
pub fn add_pending_change(
    pending_changes: &mut HashMap<String, FsChange>,
    last_change_time: &mut Option<Instant>,
    change: FsChange,
) {
    pending_changes.insert(change.path.as_str().to_string(), change);
    *last_change_time = Some(Instant::now());
}
