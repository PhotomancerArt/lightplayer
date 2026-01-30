//! Filesystem view wrapper for chroot functionality
//!
//! `LpFsView` wraps any `LpFs` implementation with a prefix path, translating
//! all operations between chrooted-relative paths and parent-absolute paths.

use crate::error::FsError;
use crate::fs::LpFs;
use crate::fs::fs_event::{FsChange, FsVersion};
use alloc::{
    format,
    rc::Rc,
    string::{String, ToString},
    vec::Vec,
};
use core::cell::RefCell;
use lp_model::path::{LpPath, LpPathBuf};

/// A filesystem view that translates paths relative to a prefix
///
/// All paths passed to this view are relative to the `prefix` path in the parent filesystem.
/// Operations are delegated to the parent with paths translated accordingly.
pub struct LpFsView {
    /// Parent filesystem to delegate operations to
    parent: Rc<RefCell<dyn LpFs>>,
    /// Prefix path in parent filesystem (e.g., "/projects/my-project/")
    /// Always ends with `/` except for root case
    prefix: LpPathBuf,
}

impl LpFsView {
    /// Create a new filesystem view with the given parent and prefix
    ///
    /// The prefix should be normalized and end with `/` (except for root).
    /// Paths passed to this view will be translated to `prefix + path` in the parent.
    pub fn new(parent: Rc<RefCell<dyn LpFs>>, prefix: &LpPath) -> Self {
        Self {
            parent,
            prefix: prefix.to_path_buf(),
        }
    }

    /// Translate a chrooted-relative path to a parent-absolute path
    ///
    /// Examples:
    /// - `/src/file.txt` with prefix `/projects/my-project/` → `/projects/my-project/src/file.txt`
    /// - `/` with prefix `/projects/my-project/` → `/projects/my-project`
    fn parent_path(&self, chrooted_path: &str) -> String {
        // chrooted_path is already normalized (comes from normalized LpPathBuf)
        let normalized = chrooted_path;
        if normalized == "/" {
            // Root path - use prefix without trailing /
            self.prefix.as_str().trim_end_matches('/').to_string()
        } else {
            // Remove leading / from normalized path and prepend prefix
            // Ensure prefix ends with / for proper joining
            let prefix_str = self.prefix.as_str();
            if prefix_str.ends_with('/') {
                format!("{}{}", prefix_str, &normalized[1..])
            } else {
                // This shouldn't happen if chroot is working correctly, but handle it
                format!("{}/{}", prefix_str, &normalized[1..])
            }
        }
    }

    /// Translate a parent-absolute path to a chrooted-relative path
    ///
    /// Used for translating results from parent operations (e.g., `list_dir`).
    /// Returns `None` if the path doesn't start with the prefix.
    fn chrooted_path(&self, parent_path: &LpPathBuf) -> Option<LpPathBuf> {
        if parent_path.as_str().starts_with(self.prefix.as_str()) {
            if let Some(stripped) = parent_path.strip_prefix(self.prefix.as_str()) {
                let relative_str = stripped.as_str();
                let normalized = if relative_str.is_empty() {
                    "/"
                } else if relative_str.starts_with('/') {
                    relative_str
                } else {
                    return Some(LpPathBuf::from(format!("/{relative_str}")));
                };
                Some(LpPathBuf::from(normalized))
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Validate that a path is valid for this view
    ///
    /// Paths must start with `/` (absolute from chroot root).
    fn validate_path(&self, path: &LpPath) -> Result<(), FsError> {
        if !path.is_absolute() {
            return Err(FsError::InvalidPath(format!(
                "Path must be relative to view root (start with /): {}",
                path.as_str()
            )));
        }
        Ok(())
    }
}

impl LpFs for LpFsView {
    fn read_file(&self, path: &LpPath) -> Result<Vec<u8>, FsError> {
        // Validate input is absolute (contract: LpFs only accepts absolute paths)
        self.validate_path(path)?;
        // Normalize for internal use
        let normalized = path.to_path_buf();
        let normalized_str = normalized.as_str();
        let parent_path = self.parent_path(normalized_str);
        let parent_lp_path = LpPath::new(parent_path.as_str());
        self.parent.borrow().read_file(parent_lp_path)
    }

    fn write_file(&self, path: &LpPath, data: &[u8]) -> Result<(), FsError> {
        // Validate input is absolute (contract: LpFs only accepts absolute paths)
        self.validate_path(path)?;
        // Normalize for internal use
        let normalized = path.to_path_buf();
        let normalized_str = normalized.as_str();
        let parent_path = self.parent_path(normalized_str);
        let parent_lp_path = LpPath::new(parent_path.as_str());
        self.parent.borrow().write_file(parent_lp_path, data)
    }

    fn file_exists(&self, path: &LpPath) -> Result<bool, FsError> {
        // Validate input is absolute (contract: LpFs only accepts absolute paths)
        self.validate_path(path)?;
        // Normalize for internal use
        let normalized = path.to_path_buf();
        let normalized_str = normalized.as_str();
        let parent_path = self.parent_path(normalized_str);
        let parent_lp_path = LpPath::new(parent_path.as_str());
        self.parent.borrow().file_exists(parent_lp_path)
    }

    fn is_dir(&self, path: &LpPath) -> Result<bool, FsError> {
        // Validate input is absolute (contract: LpFs only accepts absolute paths)
        self.validate_path(path)?;
        // Normalize for internal use
        let normalized = path.to_path_buf();
        let normalized_str = normalized.as_str();
        let parent_path = self.parent_path(normalized_str);
        let parent_lp_path = LpPath::new(parent_path.as_str());
        self.parent.borrow().is_dir(parent_lp_path)
    }

    fn list_dir(&self, path: &LpPath, recursive: bool) -> Result<Vec<LpPathBuf>, FsError> {
        // Validate input is absolute (contract: LpFs only accepts absolute paths)
        self.validate_path(path)?;
        // Normalize for internal use
        let normalized = LpPathBuf::from(path.as_str());
        let normalized_str = normalized.as_str();
        let parent_path = self.parent_path(normalized_str);
        let parent_lp_path = LpPath::new(parent_path.as_str());
        let parent_entries = self.parent.borrow().list_dir(parent_lp_path, recursive)?;

        // Translate parent paths to chrooted-relative paths
        let mut entries = Vec::new();
        for parent_entry in parent_entries {
            if let Some(chrooted_path) = self.chrooted_path(&parent_entry) {
                entries.push(chrooted_path);
            }
        }

        Ok(entries)
    }

    fn delete_file(&self, path: &LpPath) -> Result<(), FsError> {
        // Validate input is absolute (contract: LpFs only accepts absolute paths)
        self.validate_path(path)?;
        // Normalize for internal use
        let normalized = path.to_path_buf();
        let normalized_str = normalized.as_str();

        if normalized_str == "/" {
            return Err(FsError::InvalidPath(
                "Cannot delete root directory".to_string(),
            ));
        }

        let parent_path = self.parent_path(normalized_str);
        let parent_lp_path = LpPath::new(parent_path.as_str());
        self.parent.borrow().delete_file(parent_lp_path)
    }

    fn delete_dir(&self, path: &LpPath) -> Result<(), FsError> {
        // Validate input is absolute (contract: LpFs only accepts absolute paths)
        self.validate_path(path)?;
        // Normalize for internal use
        let normalized = LpPathBuf::from(path.as_str());
        let normalized_str = normalized.as_str();

        if normalized_str == "/" {
            return Err(FsError::InvalidPath(
                "Cannot delete root directory".to_string(),
            ));
        }

        let parent_path = self.parent_path(normalized_str);
        let parent_lp_path = LpPath::new(parent_path.as_str());
        self.parent.borrow().delete_dir(parent_lp_path)
    }

    fn chroot(&self, subdir: &LpPath) -> Result<Rc<RefCell<dyn LpFs>>, FsError> {
        // Validate input is absolute (contract: LpFs only accepts absolute paths)
        self.validate_path(subdir)?;
        // Normalize the subdirectory path for internal use
        let normalized = subdir.to_path_buf();
        let normalized_subdir = normalized.as_str();

        // Construct prefix relative to current chroot
        let relative_prefix = if normalized_subdir.ends_with('/') {
            normalized_subdir.to_string()
        } else {
            format!("{normalized_subdir}/")
        };

        // Construct full prefix in parent filesystem
        // Remove leading / from relative_prefix since self.prefix already ends with /
        let new_prefix = if relative_prefix == "/" {
            self.prefix.clone()
        } else {
            self.prefix.join(&relative_prefix[1..])
        };

        Ok(Rc::new(RefCell::new(LpFsView {
            parent: Rc::clone(&self.parent),
            prefix: new_prefix,
        })))
    }

    fn current_version(&self) -> FsVersion {
        self.parent.borrow().current_version()
    }

    fn get_changes_since(&self, since_version: FsVersion) -> Vec<FsChange> {
        let parent_changes = self.parent.borrow().get_changes_since(since_version);
        let prefix = &self.prefix;

        parent_changes
            .into_iter()
            .filter_map(|change| {
                if change.path.as_str().starts_with(prefix.as_str()) {
                    // Translate to chrooted-relative path
                    if let Some(chrooted_path) = self.chrooted_path(&change.path) {
                        Some(FsChange {
                            path: chrooted_path,
                            change_type: change.change_type,
                        })
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect()
    }

    fn clear_changes_before(&mut self, _before_version: FsVersion) {
        // No-op for views (parent manages versions)
    }

    fn record_changes(&mut self, _changes: Vec<FsChange>) {
        // No-op for views (parent manages versions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::LpFsMemory;
    use lp_model::AsLpPath;

    #[test]
    fn test_lp_fs_view_basic() {
        let mut fs = LpFsMemory::new();
        fs.write_file_mut("/projects/test/src/file.txt".as_path(), b"content")
            .unwrap();

        let parent_rc: Rc<RefCell<dyn LpFs>> = Rc::new(RefCell::new(fs));
        let view = LpFsView::new(Rc::clone(&parent_rc), "/projects/test/".as_path());

        assert!(view.file_exists("/src/file.txt".as_path()).unwrap());
        let content = view.read_file("/src/file.txt".as_path()).unwrap();
        assert_eq!(content, b"content");
    }

    #[test]
    fn test_lp_fs_view_sees_parent_changes() {
        let mut fs = LpFsMemory::new();
        fs.write_file_mut("/projects/test/src/file.txt".as_path(), b"initial")
            .unwrap();

        let parent_rc: Rc<RefCell<dyn LpFs>> = Rc::new(RefCell::new(fs));
        let view = LpFsView::new(Rc::clone(&parent_rc), "/projects/test/".as_path());

        // Verify initial content
        let content = view.read_file("/src/file.txt".as_path()).unwrap();
        assert_eq!(content, b"initial");

        // Modify file in parent filesystem
        parent_rc
            .borrow_mut()
            .write_file("/projects/test/src/file.txt".as_path(), b"updated")
            .unwrap();

        // View should see the updated content
        let updated_content = view.read_file("/src/file.txt".as_path()).unwrap();
        assert_eq!(updated_content, b"updated");

        // Create a new file in parent
        parent_rc
            .borrow_mut()
            .write_file("/projects/test/src/newfile.txt".as_path(), b"new")
            .unwrap();

        // View should see the new file
        assert!(view.file_exists("/src/newfile.txt".as_path()).unwrap());
        let new_content = view.read_file("/src/newfile.txt".as_path()).unwrap();
        assert_eq!(new_content, b"new");
    }

    #[test]
    fn test_lp_fs_view_nested() {
        let mut fs = LpFsMemory::new();
        fs.write_file_mut("/a/b/c/file.txt".as_path(), b"content")
            .unwrap();

        let parent_rc: Rc<RefCell<dyn LpFs>> = Rc::new(RefCell::new(fs));
        let view1 = LpFsView::new(Rc::clone(&parent_rc), "/a/".as_path());
        // Caller must convert relative paths to absolute before passing to chroot
        let chroot_path = LpPathBuf::from("/b");
        let view2 = view1.chroot(chroot_path.as_path()).unwrap();

        assert!(view2.borrow().file_exists("/c/file.txt".as_path()).unwrap());
        let content = view2.borrow().read_file("/c/file.txt".as_path()).unwrap();
        assert_eq!(content, b"content");
    }

    #[test]
    fn test_lp_fs_view_list_dir() {
        let mut fs = LpFsMemory::new();
        fs.write_file_mut("/projects/test/src/file1.txt".as_path(), b"1")
            .unwrap();
        fs.write_file_mut("/projects/test/src/file2.txt".as_path(), b"2")
            .unwrap();
        fs.write_file_mut("/projects/test/other.txt".as_path(), b"other")
            .unwrap();

        let parent_rc: Rc<RefCell<dyn LpFs>> = Rc::new(RefCell::new(fs));
        let view = LpFsView::new(Rc::clone(&parent_rc), "/projects/test/".as_path());

        let entries = view.list_dir("/src".as_path(), false).unwrap();
        assert!(entries.contains(&LpPathBuf::from("/src/file1.txt")));
        assert!(entries.contains(&LpPathBuf::from("/src/file2.txt")));
        assert!(!entries.contains(&LpPathBuf::from("/projects/test/src/file1.txt")));
    }
}
