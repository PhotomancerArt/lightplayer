//! Host filesystem implementation using std::fs

use crate::error::FsError;
use crate::fs::{
    LpFs,
    fs_event::{ChangeType, FsChange, FsVersion},
    lp_fs_view::LpFsView,
};
use alloc::{format, rc::Rc, string::ToString, vec::Vec};
use core::cell::RefCell;
use hashbrown::HashMap;
use lp_model::path::{LpPath, LpPathBuf};
use std::fs;
use std::path::PathBuf;
#[cfg(feature = "std")]
use std::sync::Mutex;

/// LP filesystem implementation using std::fs
///
/// All paths are resolved relative to `root_path` and validated to ensure
/// they stay within the root directory for security.
pub struct LpFsStd {
    root_path: PathBuf,
    /// Version counter (increments on each change)
    /// Uses Mutex for thread-safety (required for Send + Sync)
    current_version: Mutex<FsVersion>,
    /// Map of path -> (version, ChangeType) - only latest change per path
    /// Uses Mutex for thread-safety (required for Send + Sync)
    changes: Mutex<HashMap<LpPathBuf, (FsVersion, ChangeType)>>,
}

impl LpFsStd {
    /// Create a new host filesystem with the given root path
    ///
    /// The root path is the project directory. All file operations are
    /// restricted to this directory and its subdirectories.
    pub fn new(root_path: PathBuf) -> Self {
        // Ensure the root directory exists
        if let Err(e) = fs::create_dir_all(&root_path) {
            log::warn!("Failed to create root directory {root_path:?}: {e}");
        }
        Self {
            root_path,
            current_version: Mutex::new(FsVersion::default()),
            changes: Mutex::new(HashMap::new()),
        }
    }

    /// Record a filesystem change
    fn record_change(&self, path: LpPathBuf, change_type: ChangeType) {
        let mut current = self.current_version.lock().unwrap();
        *current = current.next();
        let version = *current;
        drop(current);

        self.changes
            .lock()
            .unwrap()
            .insert(path, (version, change_type));
    }

    /// Resolve a path relative to the root and validate it stays within root
    ///
    /// Returns an error if the path would escape the root directory.
    fn resolve_and_validate(&self, path: &LpPath) -> Result<PathBuf, FsError> {
        // Validate input is absolute (contract: LpFs only accepts absolute paths)
        self.validate_path(path)?;
        // Normalize for internal use
        let normalized = path.to_path_buf();
        let normalized_str = normalized.as_str();
        // Remove leading slash for joining with root_path
        let normalized_path = if normalized_str == "/" {
            ""
        } else {
            &normalized_str[1..]
        };

        // Join with root path
        let full_path = self.root_path.join(normalized_path);

        // Canonicalize to resolve any `..` components
        let canonical_path = full_path.canonicalize().or_else(|_| {
            // If canonicalize fails (path doesn't exist), use the resolved path
            // but we still need to check it's within root
            Ok(full_path)
        })?;

        // Ensure the canonical path is within the root directory
        let canonical_root = self
            .root_path
            .canonicalize()
            .map_err(|e| FsError::Filesystem(format!("Failed to canonicalize root path: {e}")))?;

        if !canonical_path.starts_with(&canonical_root) {
            return Err(FsError::InvalidPath(format!(
                "Path {:?} would escape root directory {:?}",
                path, self.root_path
            )));
        }

        Ok(canonical_path)
    }

    /// Validate that a path is relative to project root (starts with /)
    fn validate_path(&self, path: &LpPath) -> Result<(), FsError> {
        if !path.is_absolute() {
            return Err(FsError::InvalidPath(format!(
                "Path must be relative to project root (start with /): {}",
                path.as_str()
            )));
        }
        Ok(())
    }

    /// Validate that a path is safe to delete
    ///
    /// Returns an error if:
    /// - Path is "/" (root)
    ///
    /// This is a separate function so we can test it without attempting dangerous operations.
    pub fn validate_path_for_deletion(path: &LpPath) -> Result<(), FsError> {
        if path.as_str() == "/" {
            return Err(FsError::InvalidPath(
                "Cannot delete root directory".to_string(),
            ));
        }
        Ok(())
    }

    /// Get the full path for a file (without canonicalization, for non-existent paths)
    ///
    /// This is used when we need to create files that don't exist yet.
    /// Still validates that the path would be within root.
    fn get_path(&self, path: &LpPath) -> Result<PathBuf, FsError> {
        // Validate input is absolute (contract: LpFs only accepts absolute paths)
        self.validate_path(path)?;
        // Normalize for internal use
        let normalized = path.to_path_buf();
        let normalized_str = normalized.as_str();
        // Remove leading slash for joining with root_path
        let normalized_path = if normalized_str == "/" {
            ""
        } else {
            &normalized_str[1..]
        };

        let full_path = self.root_path.join(normalized_path);

        // Check that the normalized path doesn't contain `..` that would escape root
        // We do this by checking if any component is ".."
        let components: Vec<_> = normalized_path.split('/').collect();
        let mut depth = 0;
        for component in components {
            if component == ".." {
                if depth == 0 {
                    return Err(FsError::InvalidPath(format!(
                        "Path {path:?} would escape root directory"
                    )));
                }
                depth -= 1;
            } else if !component.is_empty() && component != "." {
                depth += 1;
            }
        }

        Ok(full_path)
    }

    /// Helper function for recursive directory listing
    fn list_dir_recursive_helper(
        dir_path: &std::path::Path,
        canonical_root: &std::path::Path,
        results: &mut Vec<LpPathBuf>,
    ) -> Result<(), FsError> {
        let entries = fs::read_dir(dir_path).map_err(|e| {
            FsError::Filesystem(format!("Failed to read directory {dir_path:?}: {e}"))
        })?;

        for entry in entries {
            let entry = entry
                .map_err(|e| FsError::Filesystem(format!("Failed to read directory entry: {e}")))?;

            let entry_path = entry.path();

            // Canonicalize the entry path
            let canonical_entry = entry_path.canonicalize().map_err(|e| {
                FsError::Filesystem(format!(
                    "Failed to canonicalize entry path {entry_path:?}: {e}"
                ))
            })?;

            // Build the relative path from canonical root
            let relative_path = canonical_entry.strip_prefix(canonical_root).map_err(|_| {
                FsError::Filesystem(format!(
                    "Failed to compute relative path from root: entry={canonical_entry:?}, root={canonical_root:?}"
                ))
            })?;

            // Convert to string with leading slash
            let path_str = format!("/{}", relative_path.to_string_lossy().replace('\\', "/"));

            results.push(LpPathBuf::from(path_str.as_str()));

            // If it's a directory, recurse
            if canonical_entry.is_dir() {
                Self::list_dir_recursive_helper(&canonical_entry, canonical_root, results)?;
            }
        }

        Ok(())
    }
}

impl LpFs for LpFsStd {
    fn read_file(&self, path: &LpPath) -> Result<Vec<u8>, FsError> {
        let full_path = self.resolve_and_validate(path)?;
        fs::read(&full_path)
            .map_err(|e| FsError::Filesystem(format!("Failed to read file {full_path:?}: {e}")))
    }

    fn write_file(&self, path: &LpPath, data: &[u8]) -> Result<(), FsError> {
        let full_path = self.get_path(path)?;
        // Create parent directory if it doesn't exist
        if let Some(parent) = full_path.parent() {
            if let Err(e) = fs::create_dir_all(parent) {
                return Err(FsError::Filesystem(format!(
                    "Failed to create directory {parent:?}: {e}"
                )));
            }
        }
        fs::write(&full_path, data)
            .map_err(|e| FsError::Filesystem(format!("Failed to write file {full_path:?}: {e}")))
    }

    fn file_exists(&self, path: &LpPath) -> Result<bool, FsError> {
        let full_path = self.get_path(path)?;
        Ok(full_path.exists())
    }

    fn is_dir(&self, path: &LpPath) -> Result<bool, FsError> {
        // Validate input is absolute (contract: LpFs only accepts absolute paths)
        self.validate_path(path)?;
        // Normalize for internal use
        let normalized = path.to_path_buf();
        let normalized_str = normalized.as_str();
        let full_path = self.get_path(path)?;
        if !full_path.exists() {
            return Err(FsError::NotFound(normalized_str.to_string()));
        }
        Ok(full_path.is_dir())
    }

    fn list_dir(&self, path: &LpPath, recursive: bool) -> Result<Vec<LpPathBuf>, FsError> {
        // Validate input is absolute (contract: LpFs only accepts absolute paths)
        self.validate_path(path)?;
        // Normalize for internal use
        let normalized = path.to_path_buf();
        let normalized_str = normalized.as_str();
        let full_path = self.resolve_and_validate(path)?;

        // Check if it's actually a directory
        if !full_path.is_dir() {
            return Err(FsError::Filesystem(format!(
                "Path {normalized_str:?} is not a directory"
            )));
        }

        // Get canonical root for comparison
        let canonical_root = self
            .root_path
            .canonicalize()
            .map_err(|e| FsError::Filesystem(format!("Failed to canonicalize root path: {e}")))?;

        let mut results = Vec::new();

        if recursive {
            // Recursive listing: walk the directory tree
            Self::list_dir_recursive_helper(&full_path, &canonical_root, &mut results)?;
        } else {
            // Non-recursive: only immediate children
            let entries = fs::read_dir(&full_path).map_err(|e| {
                FsError::Filesystem(format!("Failed to read directory {full_path:?}: {e}"))
            })?;

            for entry in entries {
                let entry = entry.map_err(|e| {
                    FsError::Filesystem(format!("Failed to read directory entry: {e}"))
                })?;

                let entry_path = entry.path();

                // Canonicalize the entry path
                let canonical_entry = entry_path.canonicalize().map_err(|e| {
                    FsError::Filesystem(format!(
                        "Failed to canonicalize entry path {entry_path:?}: {e}"
                    ))
                })?;

                // Build the relative path from canonical root
                let relative_path =
                    canonical_entry.strip_prefix(&canonical_root).map_err(|_| {
                        FsError::Filesystem(format!(
                            "Failed to compute relative path from root: entry={canonical_entry:?}, root={canonical_root:?}"
                        ))
                    })?;

                // Convert to string with leading slash
                let path_str = format!("/{}", relative_path.to_string_lossy().replace('\\', "/"));

                results.push(LpPathBuf::from(path_str.as_str()));
            }
        }

        Ok(results)
    }

    fn delete_file(&self, path: &LpPath) -> Result<(), FsError> {
        // Validate input is absolute (contract: LpFs only accepts absolute paths)
        self.validate_path(path)?;
        // Validate path is safe to delete (explicitly reject "/")
        Self::validate_path_for_deletion(path)?;
        // Normalize for internal use
        let normalized = path.to_path_buf();
        let normalized_str = normalized.as_str();

        let full_path = self.resolve_and_validate(path)?;

        // Check if it's a file (not a directory)
        if full_path.is_dir() {
            return Err(FsError::Filesystem(format!(
                "Path {normalized_str:?} is a directory, use delete_dir() instead"
            )));
        }

        fs::remove_file(&full_path)
            .map_err(|e| FsError::Filesystem(format!("Failed to delete file {full_path:?}: {e}")))
    }

    fn delete_dir(&self, path: &LpPath) -> Result<(), FsError> {
        // Validate input is absolute (contract: LpFs only accepts absolute paths)
        self.validate_path(path)?;
        // Validate path is safe to delete (explicitly reject "/")
        Self::validate_path_for_deletion(path)?;
        // Normalize for internal use
        let normalized = path.to_path_buf();
        let normalized_str = normalized.as_str();

        let full_path = self.resolve_and_validate(path)?;

        // Check if it's a directory
        if !full_path.is_dir() {
            return Err(FsError::Filesystem(format!(
                "Path {normalized_str:?} is not a directory, use delete_file() instead"
            )));
        }

        // Delete recursively
        fs::remove_dir_all(&full_path).map_err(|e| {
            FsError::Filesystem(format!("Failed to delete directory {full_path:?}: {e}"))
        })
    }

    fn chroot(
        &self,
        subdir: &LpPath,
    ) -> Result<alloc::rc::Rc<core::cell::RefCell<dyn LpFs>>, FsError> {
        // Validate input is absolute (contract: LpFs only accepts absolute paths)
        self.validate_path(subdir)?;
        // Normalize the subdirectory path for internal use
        let normalized = subdir.to_path_buf();
        let normalized_str = normalized.as_str();
        // Remove leading slash for joining with root_path
        let normalized_subdir = if normalized_str == "/" {
            ""
        } else {
            &normalized_str[1..]
        };

        // Join with root path
        let new_root = self.root_path.join(normalized_subdir);

        // Validate that the new root doesn't escape the current root
        // by checking if it's within the current root
        let canonical_new_root = new_root.canonicalize().or_else(|_| {
            // If canonicalize fails (path doesn't exist), use the resolved path
            // but we still need to check it's within root
            Ok(new_root.clone())
        })?;

        let canonical_current_root = self
            .root_path
            .canonicalize()
            .map_err(|e| FsError::Filesystem(format!("Failed to canonicalize root path: {e}")))?;

        if !canonical_new_root.starts_with(&canonical_current_root) {
            return Err(FsError::InvalidPath(format!(
                "Chroot path {:?} would escape root directory {:?}",
                normalized_str, self.root_path
            )));
        }

        // Construct prefix path for LpFsView
        // The prefix is the normalized subdir path
        let prefix = if normalized_str.ends_with('/') {
            normalized.clone()
        } else {
            normalized.join("")
        };

        // Wrap self in Rc<RefCell<>> for LpFsView
        // Create a new LpFsStd instance that shares the same root_path
        // (though LpFsView will handle path translation)
        let parent_rc = Rc::new(RefCell::new(LpFsStd {
            root_path: self.root_path.clone(),
            current_version: Mutex::new(*self.current_version.lock().unwrap()),
            changes: Mutex::new(self.changes.lock().unwrap().clone()),
        }));

        Ok(Rc::new(RefCell::new(LpFsView::new(
            parent_rc,
            prefix.as_path(),
        ))))
    }

    fn current_version(&self) -> FsVersion {
        *self.current_version.lock().unwrap()
    }

    fn get_changes_since(&self, since_version: FsVersion) -> Vec<FsChange> {
        self.changes
            .lock()
            .unwrap()
            .iter()
            .filter_map(|(path, (version, change_type))| {
                if *version >= since_version {
                    Some(FsChange {
                        path: path.clone(),
                        change_type: *change_type,
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    fn clear_changes_before(&mut self, before_version: FsVersion) {
        self.changes
            .lock()
            .unwrap()
            .retain(|_, (version, _)| *version >= before_version);
    }

    fn record_changes(&mut self, changes: Vec<FsChange>) {
        for change in changes {
            // Path is already LpPathBuf, just record it
            self.record_change(change.path, change.change_type);
        }
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;
    use lp_model::AsLpPath;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_path_validation_within_root() {
        let temp_dir = TempDir::new().unwrap();
        let fs = LpFsStd::new(temp_dir.path().to_path_buf());

        // Valid paths should work
        assert!(fs.get_path("/project.json".as_path()).is_ok());
        assert!(fs.get_path("/src/test.txt".as_path()).is_ok());
        // Relative paths must be converted to absolute before passing to LpFs
        assert!(fs.get_path("/project.json".as_path()).is_ok());
    }

    #[test]
    fn test_path_validation_prevents_escape() {
        let temp_dir = TempDir::new().unwrap();
        let fs = LpFsStd::new(temp_dir.path().to_path_buf());

        // Paths with .. should be rejected
        assert!(fs.get_path("/../outside.txt".as_path()).is_err());
        assert!(fs.get_path("/src/../../outside.txt".as_path()).is_err());
        assert!(
            fs.resolve_and_validate("/../outside.txt".as_path())
                .is_err()
        );
    }

    #[test]
    fn test_list_dir() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        // Create test directory structure
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("project.json"), b"{}").unwrap();
        fs::write(root.join("src/file1.txt"), b"content1").unwrap();
        fs::write(root.join("src/file2.txt"), b"content2").unwrap();
        fs::create_dir_all(root.join("src/subdir")).unwrap();

        let fs = LpFsStd::new(root.to_path_buf());
        let entries = fs.list_dir("/src".as_path(), false).unwrap();

        // Should contain the files and subdirectory
        assert!(entries.contains(&LpPathBuf::from("/src/file1.txt")));
        assert!(entries.contains(&LpPathBuf::from("/src/file2.txt")));
        assert!(entries.contains(&LpPathBuf::from("/src/subdir")));
    }

    #[test]
    fn test_list_dir_security() {
        let temp_dir = TempDir::new().unwrap();
        let fs = LpFsStd::new(temp_dir.path().to_path_buf());

        // Should not be able to list outside root
        assert!(fs.list_dir("/../".as_path(), false).is_err());
    }

    #[test]
    fn test_validate_path_for_deletion() {
        // Test the validation helper function (without attempting deletion)
        use lp_model::AsLpPath;
        assert!(LpFsStd::validate_path_for_deletion("/".as_path()).is_err());
        assert!(LpFsStd::validate_path_for_deletion("/file.txt".as_path()).is_ok());
        assert!(LpFsStd::validate_path_for_deletion("/dir".as_path()).is_ok());
    }

    #[test]
    fn test_delete_file() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();
        let fs = LpFsStd::new(root.to_path_buf());

        // Create a file
        fs::write(root.join("test.txt"), b"content").unwrap();
        assert!(fs.file_exists("/test.txt".as_path()).unwrap());

        // Delete it
        fs.delete_file("/test.txt".as_path()).unwrap();
        assert!(!fs.file_exists("/test.txt".as_path()).unwrap());
    }

    #[test]
    fn test_delete_dir() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();
        let fs = LpFsStd::new(root.to_path_buf());

        // Create directory with files
        fs::create_dir_all(root.join("dir/nested")).unwrap();
        fs::write(root.join("dir/file1.txt"), b"content1").unwrap();
        fs::write(root.join("dir/nested/file2.txt"), b"content2").unwrap();

        // Delete directory (recursive)
        fs.delete_dir("/dir".as_path()).unwrap();
        assert!(!root.join("dir").exists());
    }

    #[test]
    fn test_delete_root_rejected() {
        let temp_dir = TempDir::new().unwrap();
        let fs = LpFsStd::new(temp_dir.path().to_path_buf());

        // Should reject deleting root
        assert!(fs.delete_file("/".as_path()).is_err());
        assert!(fs.delete_dir("/".as_path()).is_err());
    }

    #[test]
    fn test_list_dir_recursive() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();
        let fs = LpFsStd::new(root.to_path_buf());

        // Create nested structure
        fs::create_dir_all(root.join("src/nested")).unwrap();
        fs::write(root.join("src/file1.txt"), b"content1").unwrap();
        fs::write(root.join("src/nested/file2.txt"), b"content2").unwrap();

        // List non-recursive
        let entries = fs.list_dir("/src".as_path(), false).unwrap();
        assert!(entries.contains(&LpPathBuf::from("/src/file1.txt")));
        assert!(entries.contains(&LpPathBuf::from("/src/nested")));
        assert!(!entries.contains(&LpPathBuf::from("/src/nested/file2.txt")));

        // List recursive
        let entries = fs.list_dir("/src".as_path(), true).unwrap();
        assert!(entries.contains(&LpPathBuf::from("/src/file1.txt")));
        assert!(entries.contains(&LpPathBuf::from("/src/nested/file2.txt")));
    }
}
