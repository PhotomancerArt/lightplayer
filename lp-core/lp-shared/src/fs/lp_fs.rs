//! Filesystem abstraction trait
//!
//! All paths in this trait are relative to the project root. The project root is the directory
//! containing `project.json`. Leading slashes indicate paths from the project root
//! (e.g., `/project.json`, `/src/my-shader.shader/main.glsl`).
//!
//! Filesystem instances have a root path (especially for real filesystem implementations) to
//! provide security by preventing access outside the project directory.

use crate::error::FsError;
use crate::fs::fs_event::{FsChange, FsVersion};
use lp_model::path::{LpPath, LpPathBuf};

/// Platform-agnostic filesystem trait
///
/// All paths are relative to the project root. `/project.json` is always the project
/// configuration file. Filesystem instances have a root path for security.
pub trait LpFs {
    /// Read a file from the filesystem
    ///
    /// Path is relative to project root (e.g., `/project.json`, `/src/my-shader.shader/main.glsl`).
    ///
    /// Returns the file contents as a byte vector, or an error if the file doesn't exist
    /// or cannot be read.
    fn read_file(&self, path: &LpPath) -> Result<alloc::vec::Vec<u8>, FsError>;

    /// Write data to a file in the filesystem
    ///
    /// Path is relative to project root.
    ///
    /// Creates the file if it doesn't exist, overwrites if it does.
    fn write_file(&self, path: &LpPath, data: &[u8]) -> Result<(), FsError>;

    /// Check if a file exists in the filesystem
    ///
    /// Path is relative to project root.
    fn file_exists(&self, path: &LpPath) -> Result<bool, FsError>;

    /// Check if a path is a directory
    ///
    /// Path is relative to project root.
    /// Returns `true` if the path exists and is a directory, `false` if it exists and is a file,
    /// or an error if the path doesn't exist or cannot be accessed.
    fn is_dir(&self, path: &LpPath) -> Result<bool, FsError>;

    /// List directory contents (files and subdirectories)
    ///
    /// Path is relative to project root (e.g., `/src` or `/src/nested`).
    ///
    /// Returns paths relative to project root. The returned paths include the directory
    /// path prefix (e.g., listing `/src` might return `["/src/my-shader.shader", "/src/my-texture.texture"]`).
    ///
    /// If `recursive` is `true`, lists all files and directories recursively. If `false`, only lists
    /// immediate children.
    fn list_dir(
        &self,
        path: &LpPath,
        recursive: bool,
    ) -> Result<alloc::vec::Vec<LpPathBuf>, FsError>;

    /// Delete a file from the filesystem
    ///
    /// Path is relative to project root.
    ///
    /// Returns an error if the path is "/" (root), would escape the root directory, or the file doesn't exist.
    fn delete_file(&self, path: &LpPath) -> Result<(), FsError>;

    /// Delete a directory from the filesystem
    ///
    /// Path is relative to project root.
    ///
    /// Always deletes recursively (removes directory and all contents).
    /// Returns an error if the path is "/" (root), would escape the root directory, or the directory doesn't exist.
    fn delete_dir(&self, path: &LpPath) -> Result<(), FsError>;

    /// Create a new filesystem view rooted at a subdirectory
    ///
    /// Returns a new `LpFs` instance where all paths are relative to the specified subdirectory.
    /// The subdirectory path is relative to the current root.
    ///
    /// For example, if the current root is `/projects` and you chroot to `my-project`,
    /// then paths like `/project.json` in the new view will resolve to `/projects/my-project/project.json`
    /// in the original filesystem.
    ///
    /// Returns `Rc<RefCell<dyn LpFs>>` to allow sharing and mutation of the filesystem view.
    fn chroot(
        &self,
        subdir: &LpPath,
    ) -> Result<alloc::rc::Rc<core::cell::RefCell<dyn LpFs>>, FsError>;

    /// Get the current filesystem version
    ///
    /// Returns the version number that will be assigned to the next change.
    /// If no changes have occurred, returns the initial version (typically 0).
    fn current_version(&self) -> FsVersion;

    /// Get all changes since a specific version
    ///
    /// Returns changes for paths that were modified at or after `since_version`.
    /// Changes are returned with paths relative to the filesystem root.
    /// Only the latest change per path is returned (if a file was modified
    /// multiple times, only the most recent change is included).
    fn get_changes_since(&self, since_version: FsVersion) -> alloc::vec::Vec<FsChange>;

    /// Clear changes older than the specified version
    ///
    /// Removes change tracking for versions older than `before_version`.
    /// This is useful for memory management when no consumers need old versions.
    fn clear_changes_before(&mut self, before_version: FsVersion);

    /// Record externally detected changes
    ///
    /// Used by filesystem implementations that don't directly track changes
    /// (e.g., `LpFsStd` receiving changes from `FileWatcher`).
    /// Each change is assigned the next version number.
    fn record_changes(&mut self, changes: alloc::vec::Vec<FsChange>);
}
