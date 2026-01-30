//! In-memory filesystem implementation for testing

use crate::error::FsError;
use crate::fs::{
    LpFs,
    fs_event::ChangeType,
    fs_event::{FsChange, FsVersion},
    lp_fs_view::LpFsView,
};
use alloc::{format, rc::Rc, string::ToString, vec::Vec};
use core::cell::RefCell;
use hashbrown::HashMap;
use lp_model::path::{LpPath, LpPathBuf};

/// In-memory filesystem implementation for testing
pub struct LpFsMemory {
    /// File storage: path -> contents (using Rc<RefCell> so chrooted filesystems can share)
    files: Rc<RefCell<HashMap<LpPathBuf, Vec<u8>>>>,
    /// Version counter (increments on each change)
    current_version: RefCell<FsVersion>,
    /// Map of path -> (version, ChangeType) - only latest change per path
    changes: RefCell<HashMap<LpPathBuf, (FsVersion, ChangeType)>>,
}

impl LpFsMemory {
    /// Create a new empty in-memory filesystem
    pub fn new() -> Self {
        Self {
            files: Rc::new(RefCell::new(HashMap::new())),
            current_version: RefCell::new(FsVersion::default()),
            changes: RefCell::new(HashMap::new()),
        }
    }

    /// Record a filesystem change
    fn record_change(&self, path: &LpPath, change_type: ChangeType) {
        let mut current = self.current_version.borrow_mut();
        *current = current.next();
        let version = *current;
        drop(current);

        self.changes
            .borrow_mut()
            .insert(path.to_path_buf(), (version, change_type));
    }

    /// Write a file (mutable version)
    pub fn write_file_mut(&mut self, path: &LpPath, data: &[u8]) -> Result<(), FsError> {
        // Validate input is absolute (contract: LpFs only accepts absolute paths)
        self.validate_path(path)?;
        // Normalize for internal storage/comparison
        let normalized = path.to_path_buf();
        let mut files = self.files.borrow_mut();
        let existed = files.contains_key(&normalized);
        files.insert(normalized.clone(), data.to_vec());
        drop(files); // Release borrow before recording change

        // Record change
        let change_type = if existed {
            ChangeType::Modify
        } else {
            ChangeType::Create
        };
        self.record_change(normalized.as_path(), change_type);

        Ok(())
    }

    /// Delete a file (mutable version)
    pub fn delete_file_mut(&mut self, path: &LpPath) -> Result<(), FsError> {
        // Validate path is safe to delete (explicitly reject "/")
        Self::validate_path_for_deletion(path)?;
        self.validate_path(path)?;
        let normalized = path.to_path_buf();

        // Check if it's a directory (by checking if any file starts with normalized + "/")
        let dir_prefix_str = format!("{}/", normalized.as_str());
        let mut files = self.files.borrow_mut();
        for file_path in files.keys() {
            if file_path.as_str().starts_with(&dir_prefix_str) {
                return Err(FsError::Filesystem(format!(
                    "Path {path:?} is a directory, use delete_dir_mut() instead"
                )));
            }
        }

        if files.remove(&normalized).is_none() {
            return Err(FsError::NotFound(path.as_str().to_string()));
        }
        drop(files); // Release borrow before recording change

        // Record change
        self.record_change(normalized.as_path(), ChangeType::Delete);

        Ok(())
    }

    /// Delete a directory (mutable version, always recursive)
    pub fn delete_dir_mut(&mut self, path: &LpPath) -> Result<(), FsError> {
        // Validate path is safe to delete (explicitly reject "/")
        Self::validate_path_for_deletion(path)?;
        self.validate_path(path)?;
        let normalized = path.to_path_buf();

        // Check if it's actually a directory (has files with this prefix)
        let prefix_str = if normalized.as_str().ends_with('/') {
            normalized.as_str().to_string()
        } else {
            format!("{}/", normalized.as_str())
        };

        let mut files = self.files.borrow_mut();
        let mut found_any = false;
        let mut files_to_remove = Vec::new();

        for file_path in files.keys() {
            if file_path.as_str().starts_with(&prefix_str) || *file_path == normalized {
                files_to_remove.push(file_path.clone());
                found_any = true;
            }
        }

        if !found_any {
            return Err(FsError::NotFound(path.as_str().to_string()));
        }

        // Remove all files with this prefix (recursive deletion)
        let files_to_remove_clone = files_to_remove.clone();
        for file_path in &files_to_remove {
            files.remove(file_path);
        }
        drop(files); // Release borrow before recording changes

        // Record changes
        for file_path in files_to_remove_clone {
            self.record_change(file_path.as_path(), ChangeType::Delete);
        }

        Ok(())
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
}

impl Default for LpFsMemory {
    fn default() -> Self {
        Self::new()
    }
}

impl LpFs for LpFsMemory {
    fn read_file(&self, path: &LpPath) -> Result<Vec<u8>, FsError> {
        let normalized = path.to_path_buf();
        self.validate_path(normalized.as_path())?;
        self.files
            .borrow()
            .get(&normalized)
            .cloned()
            .ok_or_else(|| FsError::NotFound(normalized.as_str().to_string()))
    }

    fn write_file(&self, path: &LpPath, data: &[u8]) -> Result<(), FsError> {
        // Use interior mutability to allow writes through immutable reference
        // Validate input is absolute (contract: LpFs only accepts absolute paths)
        self.validate_path(path)?;
        // Normalize for internal storage/comparison
        let normalized = path.to_path_buf();
        let mut files = self.files.borrow_mut();
        let existed = files.contains_key(&normalized);
        files.insert(normalized.clone(), data.to_vec());
        drop(files); // Release borrow before recording change

        // Record change
        let change_type = if existed {
            ChangeType::Modify
        } else {
            ChangeType::Create
        };
        self.record_change(normalized.as_path(), change_type);

        Ok(())
    }

    fn file_exists(&self, path: &LpPath) -> Result<bool, FsError> {
        let normalized = path.to_path_buf();
        self.validate_path(normalized.as_path())?;
        Ok(self.files.borrow().contains_key(&normalized))
    }

    fn is_dir(&self, path: &LpPath) -> Result<bool, FsError> {
        let normalized = path.to_path_buf();
        self.validate_path(normalized.as_path())?;
        let files = self.files.borrow();

        // Check if it exists as a file
        if files.contains_key(&normalized) {
            return Ok(false);
        }

        // Check if any file path starts with normalized + "/" (indicating it's a directory)
        let dir_prefix_str = format!("{}/", normalized.as_str());
        for file_path in files.keys() {
            if file_path.as_str().starts_with(&dir_prefix_str) {
                return Ok(true);
            }
        }

        // Path doesn't exist
        Err(FsError::NotFound(normalized.as_str().to_string()))
    }

    fn list_dir(&self, path: &LpPath, recursive: bool) -> Result<Vec<LpPathBuf>, FsError> {
        let normalized = path.to_path_buf();
        self.validate_path(normalized.as_path())?;
        let mut entries = Vec::new();
        let prefix_str = if normalized.as_str().ends_with('/') {
            normalized.as_str().to_string()
        } else {
            alloc::format!("{}/", normalized.as_str())
        };
        let files = self.files.borrow();

        if recursive {
            // Recursive: return all files/directories with this prefix
            for file_path in files.keys() {
                if file_path.as_str().starts_with(&prefix_str) {
                    entries.push(file_path.clone());
                }
            }
            // Also include directories (paths that are prefixes of files)
            let mut dirs = hashbrown::HashSet::new();
            for file_path in files.keys() {
                let file_path_str = file_path.as_str();
                if file_path_str.starts_with(&prefix_str) {
                    let remainder = &file_path_str[prefix_str.len()..];
                    if let Some(slash_pos) = remainder.find('/') {
                        let dir_path = format!("{}{}", prefix_str, &remainder[..slash_pos]);
                        dirs.insert(LpPathBuf::from(dir_path.as_str()));
                    }
                }
            }
            // Add directories that aren't already in entries
            for dir_path in dirs {
                if !entries.iter().any(|e| *e == dir_path) {
                    entries.push(dir_path);
                }
            }
        } else {
            // Non-recursive: only immediate children
            for file_path in files.keys() {
                let file_path_str = file_path.as_str();
                if file_path_str.starts_with(&prefix_str) {
                    // Extract the entry name (file or subdirectory)
                    let remainder = &file_path_str[prefix_str.len()..];
                    if let Some(slash_pos) = remainder.find('/') {
                        // It's a subdirectory - add the directory path
                        let dir_name = &remainder[..slash_pos];
                        let full_dir_path = format!("{prefix_str}{dir_name}");
                        let full_dir_path_buf = LpPathBuf::from(full_dir_path.as_str());
                        if !entries.iter().any(|e| *e == full_dir_path_buf) {
                            entries.push(full_dir_path_buf);
                        }
                    } else {
                        // It's a file directly in this directory
                        entries.push(file_path.clone());
                    }
                }
            }
        }

        Ok(entries)
    }

    fn delete_file(&self, path: &LpPath) -> Result<(), FsError> {
        // Use interior mutability to allow deletes through immutable reference
        let normalized = path.to_path_buf();
        let normalized_str = normalized.as_str();
        Self::validate_path_for_deletion(normalized.as_path())?;
        self.validate_path(normalized.as_path())?;

        // Check if it's a directory (by checking if any file starts with normalized + "/")
        let dir_prefix_str = format!("{normalized_str}/");
        let mut files = self.files.borrow_mut();
        for file_path in files.keys() {
            if file_path.as_str().starts_with(&dir_prefix_str) {
                return Err(FsError::Filesystem(format!(
                    "Path {normalized_str:?} is a directory, use delete_dir() instead"
                )));
            }
        }

        if files.remove(&normalized).is_none() {
            return Err(FsError::NotFound(normalized.as_str().to_string()));
        }
        drop(files); // Release borrow before recording change

        // Record change
        self.record_change(normalized.as_path(), ChangeType::Delete);

        Ok(())
    }

    fn delete_dir(&self, path: &LpPath) -> Result<(), FsError> {
        // Use interior mutability to allow deletes through immutable reference
        let normalized = path.to_path_buf();
        let normalized_str = normalized.as_str();
        Self::validate_path_for_deletion(normalized.as_path())?;
        self.validate_path(normalized.as_path())?;

        // Check if it's actually a directory (has files with this prefix)
        let prefix = if normalized_str.ends_with('/') {
            normalized_str.to_string()
        } else {
            format!("{normalized_str}/")
        };

        let mut files = self.files.borrow_mut();
        let mut found_any = false;
        let mut files_to_remove = Vec::new();

        for file_path in files.keys() {
            if file_path.as_str().starts_with(&prefix) || *file_path == normalized {
                files_to_remove.push(file_path.clone());
                found_any = true;
            }
        }

        if !found_any {
            return Err(FsError::NotFound(normalized.as_str().to_string()));
        }

        // Remove all files with this prefix (recursive deletion)
        for file_path in &files_to_remove {
            files.remove(file_path);
        }
        drop(files); // Release borrow before recording changes

        // Record changes
        for file_path in files_to_remove {
            self.record_change(file_path.as_path(), ChangeType::Delete);
        }

        Ok(())
    }

    fn chroot(
        &self,
        subdir: &LpPath,
    ) -> Result<alloc::rc::Rc<core::cell::RefCell<dyn LpFs>>, FsError> {
        // Validate input is absolute (contract: LpFs only accepts absolute paths)
        self.validate_path(subdir)?;
        // Normalize the subdirectory path for internal use
        let normalized = subdir.to_path_buf();
        let normalized_subdir = normalized.as_str();

        // Ensure it ends with / for prefix matching
        let prefix = if normalized_subdir.ends_with('/') {
            normalized.clone()
        } else {
            // Append "/" to make it a directory prefix
            let mut prefix_str = normalized.as_str().to_string();
            prefix_str.push('/');
            LpPathBuf::from(prefix_str)
        };

        // Wrap self in Rc<RefCell<>> for LpFsView
        // Create a new LpFsMemory instance that shares the same files storage
        let parent_rc = Rc::new(RefCell::new(LpFsMemory {
            files: Rc::clone(&self.files),
            current_version: RefCell::new(*self.current_version.borrow()),
            changes: RefCell::new(self.changes.borrow().clone()),
        }));

        Ok(Rc::new(RefCell::new(LpFsView::new(
            parent_rc,
            prefix.as_path(),
        ))))
    }

    fn current_version(&self) -> FsVersion {
        *self.current_version.borrow()
    }

    fn get_changes_since(&self, since_version: FsVersion) -> Vec<FsChange> {
        self.changes
            .borrow()
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
            .borrow_mut()
            .retain(|_, (version, _)| *version >= before_version);
    }

    fn record_changes(&mut self, changes: Vec<FsChange>) {
        for change in changes {
            self.record_change(change.path.as_path(), change.change_type);
        }
    }
}

impl LpFsMemory {
    /// Get all changes (convenience method)
    pub fn get_changes(&self) -> Vec<FsChange> {
        self.get_changes_since(FsVersion::default())
    }

    /// Reset all changes (clear the change history)
    pub fn reset_changes(&mut self) {
        self.changes.borrow_mut().clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_model::AsLpPath;

    #[test]
    fn test_create_and_read_file() {
        let mut fs = LpFsMemory::new();
        fs.write_file_mut("/test.txt".as_path(), b"hello").unwrap();
        assert_eq!(fs.read_file("/test.txt".as_path()).unwrap(), b"hello");
    }

    #[test]
    fn test_file_exists() {
        let mut fs = LpFsMemory::new();
        assert!(!fs.file_exists("/test.txt".as_path()).unwrap());
        fs.write_file_mut("/test.txt".as_path(), b"hello").unwrap();
        assert!(fs.file_exists("/test.txt".as_path()).unwrap());
    }

    #[test]
    fn test_list_dir() {
        let mut fs = LpFsMemory::new();
        fs.write_file_mut("/src/file1.txt".as_path(), b"content1")
            .unwrap();
        fs.write_file_mut("/src/file2.txt".as_path(), b"content2")
            .unwrap();
        fs.write_file_mut("/src/nested/file3.txt".as_path(), b"content3")
            .unwrap();
        fs.write_file_mut("/other.txt".as_path(), b"content")
            .unwrap();

        let entries = fs.list_dir("/src".as_path(), false).unwrap();
        assert!(entries.contains(&LpPathBuf::from("/src/file1.txt")));
        assert!(entries.contains(&LpPathBuf::from("/src/file2.txt")));
        // list_dir("/src") should show "/src/nested" as a directory, not the file inside it
        assert!(entries.contains(&LpPathBuf::from("/src/nested")));
        assert!(!entries.contains(&LpPathBuf::from("/src/nested/file3.txt")));
        assert!(!entries.contains(&LpPathBuf::from("/other.txt")));
    }

    #[test]
    fn test_list_dir_recursive() {
        let mut fs = LpFsMemory::new();
        fs.write_file_mut("/src/file1.txt".as_path(), b"content1")
            .unwrap();
        fs.write_file_mut("/src/nested/file2.txt".as_path(), b"content2")
            .unwrap();
        fs.write_file_mut("/src/nested/deep/file3.txt".as_path(), b"content3")
            .unwrap();

        let entries = fs.list_dir("/src".as_path(), true).unwrap();
        assert!(entries.contains(&LpPathBuf::from("/src/file1.txt")));
        assert!(entries.contains(&LpPathBuf::from("/src/nested/file2.txt")));
        assert!(entries.contains(&LpPathBuf::from("/src/nested/deep/file3.txt")));
    }

    #[test]
    fn test_delete_file() {
        let mut fs = LpFsMemory::new();
        fs.write_file_mut("/test.txt".as_path(), b"content")
            .unwrap();
        assert!(fs.file_exists("/test.txt".as_path()).unwrap());

        fs.delete_file_mut("/test.txt".as_path()).unwrap();
        assert!(!fs.file_exists("/test.txt".as_path()).unwrap());
    }

    #[test]
    fn test_delete_dir() {
        let mut fs = LpFsMemory::new();
        fs.write_file_mut("/dir/file1.txt".as_path(), b"content1")
            .unwrap();
        fs.write_file_mut("/dir/nested/file2.txt".as_path(), b"content2")
            .unwrap();
        assert!(fs.file_exists("/dir/file1.txt".as_path()).unwrap());
        assert!(fs.file_exists("/dir/nested/file2.txt".as_path()).unwrap());

        fs.delete_dir_mut("/dir".as_path()).unwrap();
        assert!(!fs.file_exists("/dir/file1.txt".as_path()).unwrap());
        assert!(!fs.file_exists("/dir/nested/file2.txt".as_path()).unwrap());
    }

    #[test]
    fn test_delete_root_rejected() {
        let mut fs = LpFsMemory::new();
        assert!(fs.delete_file_mut("/".as_path()).is_err());
        assert!(fs.delete_dir_mut("/".as_path()).is_err());
    }

    #[test]
    fn test_validate_path_for_deletion() {
        assert!(LpFsMemory::validate_path_for_deletion("/".as_path()).is_err());
        assert!(LpFsMemory::validate_path_for_deletion("/file.txt".as_path()).is_ok());
        assert!(LpFsMemory::validate_path_for_deletion("/dir".as_path()).is_ok());
    }

    #[test]
    fn test_path_validation() {
        let mut fs = LpFsMemory::new();
        // Caller must normalize relative paths to absolute before passing to LpFs
        // Prepend "/" to make relative paths absolute, then normalize
        let relative_path = LpPathBuf::from("/relative");
        fs.write_file_mut(relative_path.as_path(), b"data").unwrap();
        assert!(fs.file_exists("/relative".as_path()).unwrap());
        assert!(fs.write_file_mut("/valid".as_path(), b"data").is_ok());

        // Test that normalization works correctly - caller normalizes before calling
        let normalized_path = LpPathBuf::from("/normalized");
        fs.write_file_mut(normalized_path.as_path(), b"data2")
            .unwrap();
        assert!(fs.file_exists("/normalized".as_path()).unwrap());
    }

    #[test]
    fn test_chroot_basic() {
        let mut fs = LpFsMemory::new();
        fs.write_file_mut("/projects/test/project.json".as_path(), b"{}")
            .unwrap();
        fs.write_file_mut("/projects/test/src/file.txt".as_path(), b"content")
            .unwrap();
        fs.write_file_mut("/projects/other/file.txt".as_path(), b"other")
            .unwrap();

        let chrooted = fs.chroot("/projects/test".as_path()).unwrap();
        assert!(
            chrooted
                .borrow()
                .file_exists("/project.json".as_path())
                .unwrap()
        );
        assert!(
            chrooted
                .borrow()
                .file_exists("/src/file.txt".as_path())
                .unwrap()
        );
        assert!(
            !chrooted
                .borrow()
                .file_exists("/projects/other/file.txt".as_path())
                .unwrap()
        );
    }

    #[test]
    fn test_chroot_with_relative_path() {
        let mut fs = LpFsMemory::new();
        fs.write_file_mut("/test-projects/test/project.json".as_path(), b"{}")
            .unwrap();
        fs.write_file_mut("/test-projects/test/src/file.txt".as_path(), b"content")
            .unwrap();

        // Test with "./test-projects/test" - caller must normalize to absolute first
        let chroot_path = LpPathBuf::from("/test-projects/test");
        let chrooted = fs.chroot(chroot_path.as_path()).unwrap();
        assert!(
            chrooted
                .borrow()
                .file_exists("/project.json".as_path())
                .unwrap()
        );
        assert!(
            chrooted
                .borrow()
                .file_exists("/src/file.txt".as_path())
                .unwrap()
        );
    }

    #[test]
    fn test_chroot_path_normalization() {
        let mut fs = LpFsMemory::new();
        fs.write_file_mut("/a/b/c/file.txt".as_path(), b"content")
            .unwrap();

        // All these should work and point to the same directory
        // Caller must normalize relative paths to absolute before passing to chroot
        let chroot1 = fs.chroot("/a/b".as_path()).unwrap();
        let chroot2_path = LpPathBuf::from("/a/b");
        let chroot2 = fs.chroot(chroot2_path.as_path()).unwrap();
        let chroot3_path = LpPathBuf::from("/a/b");
        let chroot3 = fs.chroot(chroot3_path.as_path()).unwrap();

        assert!(
            chroot1
                .borrow()
                .file_exists("/c/file.txt".as_path())
                .unwrap()
        );
        assert!(
            chroot2
                .borrow()
                .file_exists("/c/file.txt".as_path())
                .unwrap()
        );
        assert!(
            chroot3
                .borrow()
                .file_exists("/c/file.txt".as_path())
                .unwrap()
        );
    }

    #[test]
    fn test_chroot_relative_paths() {
        let mut fs = LpFsMemory::new();
        fs.write_file_mut("/src/test.shader/main.glsl".as_path(), b"shader code")
            .unwrap();
        fs.write_file_mut("/src/test.shader/node.json".as_path(), b"{}")
            .unwrap();

        // Chroot to node directory
        let chrooted = fs.chroot("/src/test.shader".as_path()).unwrap();

        // Paths must be absolute - caller normalizes relative paths to absolute before calling
        assert!(
            chrooted
                .borrow()
                .file_exists("/main.glsl".as_path())
                .unwrap()
        );

        // Caller converts relative paths to absolute before calling LpFs
        let relative_path1 = LpPathBuf::from("/main.glsl");
        assert!(
            chrooted
                .borrow()
                .file_exists(relative_path1.as_path())
                .unwrap()
        );
        let relative_path2 = LpPathBuf::from("/main.glsl");
        assert!(
            chrooted
                .borrow()
                .file_exists(relative_path2.as_path())
                .unwrap()
        );

        // Read file with absolute path
        let content = chrooted.borrow().read_file("/main.glsl".as_path()).unwrap();
        assert_eq!(content, b"shader code");

        // Read file with absolute path (normalized from relative)
        let content2 = chrooted.borrow().read_file("/main.glsl".as_path()).unwrap();
        assert_eq!(content2, b"shader code");

        // Read file with absolute path
        let content3 = chrooted.borrow().read_file("/main.glsl".as_path()).unwrap();
        assert_eq!(content3, b"shader code");
    }

    #[test]
    fn test_chroot_path_normalization_relative() {
        let mut fs = LpFsMemory::new();
        fs.write_file_mut("/a/b/c/file.txt".as_path(), b"content")
            .unwrap();
        fs.write_file_mut("/a/b/other.txt".as_path(), b"other")
            .unwrap();

        let chrooted = fs.chroot("/a/b".as_path()).unwrap();

        // Test various path formats - caller must convert relative paths to absolute first
        // All paths passed to LpFs must be absolute
        assert!(
            chrooted
                .borrow()
                .file_exists("/c/file.txt".as_path())
                .unwrap()
        );
        assert!(
            chrooted
                .borrow()
                .file_exists("/c/file.txt".as_path())
                .unwrap()
        );
        assert!(
            chrooted
                .borrow()
                .file_exists("/c/file.txt".as_path())
                .unwrap()
        );

        assert!(
            chrooted
                .borrow()
                .file_exists("/other.txt".as_path())
                .unwrap()
        );
        assert!(
            chrooted
                .borrow()
                .file_exists("/other.txt".as_path())
                .unwrap()
        );
        assert!(
            chrooted
                .borrow()
                .file_exists("/other.txt".as_path())
                .unwrap()
        );

        // Read with absolute path
        let content = chrooted
            .borrow()
            .read_file("/c/file.txt".as_path())
            .unwrap();
        assert_eq!(content, b"content");

        // Read with absolute path
        let content2 = chrooted
            .borrow()
            .read_file("/c/file.txt".as_path())
            .unwrap();
        assert_eq!(content2, b"content");
    }

    #[test]
    fn test_chroot_list_dir_relative_paths() {
        let mut fs = LpFsMemory::new();
        fs.write_file_mut("/src/shader/main.glsl".as_path(), b"code")
            .unwrap();
        fs.write_file_mut("/src/shader/node.json".as_path(), b"{}")
            .unwrap();
        fs.write_file_mut("/src/shader/util.glsl".as_path(), b"util")
            .unwrap();

        let chrooted = fs.chroot("/src/shader".as_path()).unwrap();

        // List root directory
        let entries = chrooted.borrow().list_dir("/".as_path(), false).unwrap();
        assert!(entries.contains(&LpPathBuf::from("/main.glsl")));
        assert!(entries.contains(&LpPathBuf::from("/node.json")));
        assert!(entries.contains(&LpPathBuf::from("/util.glsl")));

        // List with relative path - caller must convert to absolute first
        let relative_path = LpPathBuf::from("/");
        let entries2 = chrooted
            .borrow()
            .list_dir(relative_path.as_path(), false)
            .unwrap();
        assert!(entries2.contains(&LpPathBuf::from("/main.glsl")));
    }

    #[test]
    fn test_chroot_nested() {
        let mut fs = LpFsMemory::new();
        fs.write_file_mut("/a/b/c/file.txt".as_path(), b"content")
            .unwrap();

        let chroot1 = fs.chroot("/a".as_path()).unwrap();
        // Caller must convert relative paths to absolute before passing to chroot
        let chroot2_path = LpPathBuf::from("/b");
        let chroot2 = chroot1.borrow().chroot(chroot2_path.as_path()).unwrap();
        assert!(
            chroot2
                .borrow()
                .file_exists("/c/file.txt".as_path())
                .unwrap()
        );
    }

    #[test]
    fn test_chroot_read_file() {
        let mut fs = LpFsMemory::new();
        fs.write_file_mut(
            "/projects/test/project.json".as_path(),
            b"{\"name\":\"test\"}",
        )
        .unwrap();

        let chrooted = fs.chroot("/projects/test".as_path()).unwrap();
        let content = chrooted
            .borrow()
            .read_file("/project.json".as_path())
            .unwrap();
        assert_eq!(content, b"{\"name\":\"test\"}");
    }

    #[test]
    fn test_chroot_list_dir() {
        let mut fs = LpFsMemory::new();
        fs.write_file_mut("/projects/test/src/file1.txt".as_path(), b"1")
            .unwrap();
        fs.write_file_mut("/projects/test/src/file2.txt".as_path(), b"2")
            .unwrap();
        fs.write_file_mut("/projects/test/other.txt".as_path(), b"other")
            .unwrap();

        let chrooted = fs.chroot("/projects/test".as_path()).unwrap();
        let entries = chrooted.borrow().list_dir("/src".as_path(), false).unwrap();
        assert!(entries.contains(&LpPathBuf::from("/src/file1.txt")));
        assert!(entries.contains(&LpPathBuf::from("/src/file2.txt")));
        assert!(!entries.contains(&LpPathBuf::from("/projects/test/src/file1.txt")));
    }

    #[test]
    fn test_chroot_sees_parent_changes() {
        let mut fs = LpFsMemory::new();
        // Create initial file
        fs.write_file_mut("/projects/test/src/file.txt".as_path(), b"initial")
            .unwrap();

        // Chroot to the project
        let chrooted = fs.chroot("/projects/test".as_path()).unwrap();

        // Verify initial content
        let content = chrooted
            .borrow()
            .read_file("/src/file.txt".as_path())
            .unwrap();
        assert_eq!(content, b"initial");

        // Modify file in parent filesystem
        fs.write_file_mut("/projects/test/src/file.txt".as_path(), b"updated")
            .unwrap();

        // Chrooted filesystem should see the updated content
        let updated_content = chrooted
            .borrow()
            .read_file("/src/file.txt".as_path())
            .unwrap();
        assert_eq!(updated_content, b"updated");

        // Create a new file in parent
        fs.write_file_mut("/projects/test/src/newfile.txt".as_path(), b"new")
            .unwrap();

        // Chrooted filesystem should see the new file
        assert!(
            chrooted
                .borrow()
                .file_exists("/src/newfile.txt".as_path())
                .unwrap()
        );
        let new_content = chrooted
            .borrow()
            .read_file("/src/newfile.txt".as_path())
            .unwrap();
        assert_eq!(new_content, b"new");
    }
}
