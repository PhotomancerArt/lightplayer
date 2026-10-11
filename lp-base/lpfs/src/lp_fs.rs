//! Filesystem abstraction trait
//!
//! All paths in this trait are relative to the project root. The project root is the directory
//! containing `project.json`. Leading slashes indicate paths from the project root
//! (e.g., `/project.json`, `/shader.glsl`).
//!
//! Filesystem instances have a root path (especially for real filesystem implementations) to
//! provide security by preventing access outside the project directory.

use crate::error::FsError;
use crate::fs_event::{FsEvent, FsVersion};
use crate::{LpPath, LpPathBuf};

/// The most bytes one [`LpFs::write_deflated_chunk`] may inflate to (the
/// tree store's largest logical chunk; the wire's
/// `lpc_wire::budget::FILE_SYNC_DEFLATED_CHUNK_MAX_LOGICAL`).
pub const MAX_DEFLATED_CHUNK_LOGICAL: usize = 4 * 1024;

/// Platform-agnostic filesystem trait
///
/// All paths are relative to the project root. `/project.json` is always the project
/// configuration file. Filesystem instances have a root path for security.
pub trait LpFs {
    /// Read a file from the filesystem
    ///
    /// Path is relative to project root (e.g., `/project.json`, `/shader.glsl`).
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

    /// Append data to a file, creating it if it doesn't exist.
    ///
    /// Path is relative to project root.
    ///
    /// The default implementation is read-modify-write. Backends with a
    /// native append (in-memory buffers, LittleFS) should override it:
    /// appends are how chunked wire file-writes land (files larger than
    /// one protocol frame), and read-modify-write is O(n²) across a
    /// chunked transfer.
    fn append_file(&self, path: &LpPath, data: &[u8]) -> Result<(), FsError> {
        let mut existing = if self.file_exists(path)? {
            self.read_file(path)?
        } else {
            alloc::vec::Vec::new()
        };
        existing.extend_from_slice(data);
        self.write_file(path, &existing)
    }

    /// Size of a file in bytes.
    ///
    /// Path is relative to project root.
    ///
    /// The default implementation reads the whole file; backends with cheap
    /// metadata (in-memory maps, `std::fs`, LittleFS stat) should override
    /// it — chunked wire writes validate append offsets against the size.
    fn file_size(&self, path: &LpPath) -> Result<u64, FsError> {
        Ok(self.read_file(path)?.len() as u64)
    }

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

    /// Start a batch: until [`Self::commit_batch`], the writes, appends and
    /// deletes made through this filesystem (and its chroot views) commit
    /// together or not at all — a power cut leaves every file as it was
    /// before the batch. Reads see the batch's own writes.
    ///
    /// The default does nothing: a backend without transactions (littlefs,
    /// the host filesystem, memory) commits each call by itself, as it
    /// always has. A batch cannot nest.
    fn begin_batch(&self) -> Result<(), FsError> {
        Ok(())
    }

    /// Commit the batch begun by [`Self::begin_batch`] (nothing to do
    /// outside one). On an error the batch stays open: abort it.
    fn commit_batch(&self) -> Result<(), FsError> {
        Ok(())
    }

    /// Drop the batch begun by [`Self::begin_batch`]: every file is as it
    /// was before it (nothing to do outside one, or on a backend without
    /// transactions).
    fn abort_batch(&self) -> Result<(), FsError> {
        Ok(())
    }

    /// Whether [`Self::begin_batch`] really makes a batch: `true` only on a
    /// backend whose batch commits as one (the tree store). The default
    /// `false` is every backend that commits each call by itself — which is
    /// what the trait's batch defaults do — so a server can tell a client
    /// the truth about *this* filesystem instead of a build fact.
    fn batches_are_atomic(&self) -> bool {
        false
    }

    /// Write one chunk of a file sent as raw deflate (RFC 1951): what
    /// `deflated` inflates to — exactly `logical_len` bytes, at most
    /// [`MAX_DEFLATED_CHUNK_LOGICAL`] — lands at the file's **logical**
    /// `offset`. `offset == 0` creates or truncates; any other offset must
    /// equal the file's current length (an append). Both are checked before
    /// anything is allocated or inflated, and a stream that does not inflate
    /// to exactly `logical_len` bytes writes nothing.
    ///
    /// The default inflates into a heap buffer of `logical_len` (at most
    /// 4 KiB, beside the request that carried it — the one cost a board
    /// without at-rest compression pays) and writes the plain bytes with
    /// [`Self::write_file`] or [`Self::append_file`]. A backend that keeps
    /// the deflated bytes (the tree store) overrides it.
    fn write_deflated_chunk(
        &self,
        path: &LpPath,
        offset: u32,
        logical_len: u32,
        deflated: &[u8],
    ) -> Result<(), FsError> {
        let logical = logical_len as usize;
        if logical > MAX_DEFLATED_CHUNK_LOGICAL {
            return Err(FsError::Filesystem(alloc::format!(
                "deflated chunk too large: {logical} B logical, at most {MAX_DEFLATED_CHUNK_LOGICAL}"
            )));
        }
        if offset != 0 {
            let len = self.file_size(path)?;
            if len != u64::from(offset) {
                return Err(FsError::Filesystem(alloc::format!(
                    "offset mismatch: file is {len} bytes, chunk at {offset}"
                )));
            }
        }
        let mut buf = alloc::vec![0u8; logical];
        match lp_deflate::inflate(deflated, &mut buf, 0) {
            Ok(n) if n == logical => {}
            _ => {
                return Err(FsError::Filesystem(alloc::string::String::from(
                    "corrupt deflated chunk: it does not inflate to its length",
                )));
            }
        }
        if offset == 0 {
            self.write_file(path, &buf)
        } else {
            self.append_file(path, &buf)
        }
    }

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
    ///
    /// This reports changes tracked or recorded by the filesystem implementation;
    /// it is not a guarantee that host-native external edits are watched.
    fn get_changes_since(&self, since_version: FsVersion) -> alloc::vec::Vec<FsEvent>;

    /// Alias for [`Self::get_changes_since`].
    fn get_events_since(&self, since_version: FsVersion) -> alloc::vec::Vec<FsEvent> {
        self.get_changes_since(since_version)
    }

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
    fn record_changes(&mut self, changes: alloc::vec::Vec<FsEvent>);
}
