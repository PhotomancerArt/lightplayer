//! File system watcher
//!
//! Wraps the `notify` crate to provide file change events for the dev command.
//! Converts OS-level file events into `FsChange` events compatible with the sync system.

use anyhow::{Context, Result};
use lp_shared::fs::fs_event::{ChangeType, FsChange};
use notify::Watcher;
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;

/// File system watcher that converts OS events to FsChange events
pub struct FileWatcher {
    /// Receiver for file change events
    event_receiver: mpsc::UnboundedReceiver<FsChange>,
    /// Root path of the project (for path normalization)
    #[allow(dead_code, reason = "Stored for path normalization")]
    root_path: PathBuf,
    /// Watcher handle (kept alive to continue watching)
    _watcher: notify::RecommendedWatcher,
}

impl FileWatcher {
    /// Create a new file watcher for the given directory
    ///
    /// # Arguments
    ///
    /// * `root_path` - Root directory to watch (project directory)
    ///
    /// # Returns
    ///
    /// * `Ok(FileWatcher)` if watcher was created successfully
    /// * `Err` if watcher creation failed
    pub fn new(root_path: PathBuf) -> Result<Self> {
        // Canonicalize root path to handle symlinks (e.g., /private/var on macOS)
        let canonical_root = root_path.canonicalize().with_context(|| {
            format!("Failed to canonicalize root path: {}", root_path.display())
        })?;

        // Create channel for events (use unbounded to avoid blocking in sync callback)
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        // Clone for the event handler
        let root_path_clone = canonical_root.clone();

        // Create watcher with sync event handler
        // Note: notify callbacks run on blocking threads, so we process synchronously
        let mut watcher = notify::RecommendedWatcher::new(
            move |res: Result<notify::Event, notify::Error>| {
                match res {
                    Ok(event) => {
                        // Process event synchronously (we're in a blocking thread)
                        let root = root_path_clone.clone();
                        let tx = event_tx.clone();

                        // Process each path in the event
                        for path in event.paths {
                            // Skip if it's a directory (we only sync files)
                            if path.is_dir() {
                                continue;
                            }

                            // Skip temporary/backup files (editor artifacts)
                            if Self::should_ignore_file(&path) {
                                continue;
                            }

                            // Normalize path (absolute → relative with leading /)
                            let normalized_path = match Self::normalize_path_sync(&path, &root) {
                                Ok(p) => p,
                                Err(e) => {
                                    eprintln!("Error normalizing path {path:?}: {e}");
                                    continue;
                                }
                            };

                            // Map notify event kind to ChangeType
                            let change_type = match event.kind {
                                notify::EventKind::Create(_) => ChangeType::Create,
                                notify::EventKind::Modify(_) => ChangeType::Modify,
                                notify::EventKind::Remove(_) => ChangeType::Delete,
                                notify::EventKind::Any | notify::EventKind::Other => {
                                    // For Any/Other, try to determine from file existence
                                    if path.exists() {
                                        ChangeType::Modify
                                    } else {
                                        ChangeType::Delete
                                    }
                                }
                                _ => {
                                    // Skip unknown event types
                                    continue;
                                }
                            };

                            // Create FsChange and send to channel (non-blocking)
                            let change = FsChange {
                                path: lp_model::LpPathBuf::from(normalized_path),
                                change_type,
                            };

                            // Send event (non-blocking, drop if channel is full)
                            let _ = tx.send(change);
                        }
                    }
                    Err(e) => {
                        eprintln!("File watcher error: {e}");
                    }
                }
            },
            notify::Config::default(),
        )
        .context("Failed to create file watcher")?;

        // Watch the root directory recursively
        watcher
            .watch(&canonical_root, notify::RecursiveMode::Recursive)
            .with_context(|| format!("Failed to watch directory: {}", canonical_root.display()))?;

        Ok(Self {
            event_receiver: event_rx,
            root_path: canonical_root,
            _watcher: watcher,
        })
    }

    /// Check if a file should be ignored (temporary/backup files)
    ///
    /// Returns `true` if the file should be ignored and not synced.
    fn should_ignore_file(path: &Path) -> bool {
        // Get file name
        let file_name = match path.file_name() {
            Some(name) => name.to_string_lossy(),
            None => return true, // No filename, skip
        };

        // Ignore files starting with . (hidden files, except .gitkeep etc.)
        if file_name.starts_with('.') {
            // Allow some common hidden files
            if file_name == ".gitkeep" || file_name == ".gitignore" {
                return false;
            }
            return true;
        }

        // Ignore files ending with ~ (backup files)
        if file_name.ends_with('~') {
            return true;
        }

        // Ignore files ending with .swp, .swo (vim swap files)
        if file_name.ends_with(".swp") || file_name.ends_with(".swo") {
            return true;
        }

        // Ignore files starting with # (emacs backup files)
        if file_name.starts_with('#') && file_name.ends_with('#') {
            return true;
        }

        false
    }

    /// Normalize an absolute path to a relative path with leading /
    ///
    /// Converts `/absolute/path/to/project/src/file.glsl` to `/src/file.glsl`
    /// when root_path is `/absolute/path/to/project`.
    /// This version is safe to call from sync contexts (like notify callbacks).
    fn normalize_path_sync(absolute_path: &Path, root_path: &Path) -> Result<String> {
        // Canonicalize both paths to handle symlinks (e.g., /private/var on macOS)
        let canonical_absolute = absolute_path
            .canonicalize()
            .unwrap_or_else(|_| absolute_path.to_path_buf());
        let canonical_root = root_path
            .canonicalize()
            .unwrap_or_else(|_| root_path.to_path_buf());

        // Get relative path from root
        let relative = canonical_absolute
            .strip_prefix(&canonical_root)
            .with_context(|| {
                format!("Path {canonical_absolute:?} is not within root {canonical_root:?}")
            })?;

        // Convert to string with leading /
        let path_str = relative.to_string_lossy().replace('\\', "/");
        if path_str.starts_with('/') {
            Ok(path_str)
        } else {
            Ok(format!("/{path_str}"))
        }
    }

    /// Get the next file change event
    ///
    /// This is a non-blocking operation that returns `None` if no events are available.
    ///
    /// # Returns
    ///
    /// * `Some(FsChange)` if an event is available
    /// * `None` if no events are available or the channel is closed
    pub async fn next_change(&mut self) -> Option<FsChange> {
        self.event_receiver.recv().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_filewatcher_creation() {
        let temp_dir = TempDir::new().unwrap();
        let watcher = FileWatcher::new(temp_dir.path().to_path_buf());
        assert!(watcher.is_ok());
    }

    #[tokio::test]
    async fn test_filewatcher_detects_create() {
        let temp_dir = TempDir::new().unwrap();
        let mut watcher = FileWatcher::new(temp_dir.path().to_path_buf()).unwrap();

        // Give watcher time to start
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Create a file
        let test_file = temp_dir.path().join("test.txt");
        std::fs::write(&test_file, b"content").unwrap();

        // Wait for event (with timeout)
        let change = tokio::time::timeout(Duration::from_secs(2), watcher.next_change())
            .await
            .unwrap();

        assert!(change.is_some(), "Expected file create event");
        let change = change.unwrap();
        assert_eq!(change.path.as_str(), "/test.txt");
        assert_eq!(change.change_type, ChangeType::Create);
    }

    #[tokio::test]
    async fn test_filewatcher_detects_modify() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.txt");

        // Create file first
        std::fs::write(&test_file, b"initial").unwrap();

        // Create watcher after file exists
        let mut watcher = FileWatcher::new(temp_dir.path().to_path_buf()).unwrap();

        // Give watcher time to start
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Modify the file
        std::fs::write(&test_file, b"modified").unwrap();

        // Wait for event (may get Create or Modify depending on OS)
        let change = tokio::time::timeout(Duration::from_secs(2), watcher.next_change())
            .await
            .unwrap();

        assert!(change.is_some(), "Expected file modify event");
        let change = change.unwrap();
        assert_eq!(change.path.as_str(), "/test.txt");
        // Some OSes report modify as Create, so accept either
        assert!(
            change.change_type == ChangeType::Modify || change.change_type == ChangeType::Create,
            "Expected Modify or Create, got {:?}",
            change.change_type
        );
    }

    #[tokio::test]
    async fn test_filewatcher_detects_delete() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.txt");

        // Create file first
        std::fs::write(&test_file, b"content").unwrap();

        // Create watcher after file exists
        let mut watcher = FileWatcher::new(temp_dir.path().to_path_buf()).unwrap();

        // Give watcher time to start
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Delete the file
        std::fs::remove_file(&test_file).unwrap();

        // Wait for event (with longer timeout for delete events)
        // Note: Some OSes may report delete events differently, so we may need to wait for multiple events
        let mut found_delete = false;
        for _ in 0..5 {
            if let Ok(Some(change)) =
                tokio::time::timeout(Duration::from_secs(1), watcher.next_change()).await
            {
                if change.path.as_str() == "/test.txt" {
                    if change.change_type == ChangeType::Delete {
                        found_delete = true;
                        break;
                    }
                    // Some OSes report delete as Modify when file doesn't exist
                    if change.change_type == ChangeType::Modify && !test_file.exists() {
                        found_delete = true;
                        break;
                    }
                }
            }
        }
        assert!(found_delete, "Expected file delete event for /test.txt");
    }

    #[test]
    fn test_normalize_path() {
        let root = PathBuf::from("/project");
        let absolute = PathBuf::from("/project/src/file.glsl");
        let normalized = FileWatcher::normalize_path_sync(&absolute, &root).unwrap();
        assert_eq!(normalized, "/src/file.glsl");
    }

    #[test]
    fn test_normalize_path_nested() {
        let root = PathBuf::from("/project");
        let absolute = PathBuf::from("/project/src/nested/file.glsl");
        let normalized = FileWatcher::normalize_path_sync(&absolute, &root).unwrap();
        assert_eq!(normalized, "/src/nested/file.glsl");
    }

    #[test]
    fn test_should_ignore_file_backup() {
        let path = PathBuf::from("/project/src/file.glsl~");
        assert!(FileWatcher::should_ignore_file(&path));
    }

    #[test]
    fn test_should_ignore_file_swap() {
        let path = PathBuf::from("/project/src/file.glsl.swp");
        assert!(FileWatcher::should_ignore_file(&path));
    }

    #[test]
    fn test_should_ignore_file_hidden() {
        let path = PathBuf::from("/project/src/.hidden");
        assert!(FileWatcher::should_ignore_file(&path));
    }

    #[test]
    fn test_should_not_ignore_gitkeep() {
        let path = PathBuf::from("/project/src/.gitkeep");
        assert!(!FileWatcher::should_ignore_file(&path));
    }

    #[test]
    fn test_should_not_ignore_normal_file() {
        let path = PathBuf::from("/project/src/file.glsl");
        assert!(!FileWatcher::should_ignore_file(&path));
    }
}
