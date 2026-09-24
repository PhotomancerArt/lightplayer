//! The project runtime's filesystem, with the access files taken out of it.
//!
//! The fs *wire* path already refuses an access file's bytes on every link
//! (`handlers::handle_fs_request`, `file_sync`). That leaves every other
//! reader of a loaded project's files: the engine loading a node's resources,
//! the registry parsing artifacts, the overlay's base-value parse. A project
//! — shared, catalog or authored — that names `.lp/access.json` as a shader
//! source or any other resource would otherwise have its sidecar read into
//! the runtime, and from there a compile error, a node status or an
//! inventory entry can carry its bytes to a play-tier `ProjectRead`.
//!
//! So a loaded project never sees one. [`AccessGuardedFs`] wraps the
//! project's chrooted view and answers a read of any path
//! [`lpc_access::is_access_file_path`] holds for with an error — the
//! resource fails to load and says so, and nothing downstream ever has the
//! bytes to leak. Everything else passes straight through, writes and
//! deletes included (edit may write an access file; nobody reads one).
//!
//! The server's own reads of the sidecar and the device store (login's
//! installed-secrets assembly, `access_store`) go through the **base** fs,
//! not a project's, so this wrapper is never in their way.

extern crate alloc;

use alloc::format;
use alloc::rc::Rc;
use alloc::vec::Vec;
use core::cell::RefCell;

use lpfs::{FsError, FsEvent, FsVersion, LpFs, LpPath, LpPathBuf};

/// A project filesystem that refuses to read access files.
pub struct AccessGuardedFs {
    inner: Rc<RefCell<dyn LpFs>>,
}

impl AccessGuardedFs {
    /// Wrap `inner`, a project's filesystem view.
    #[must_use]
    pub fn new(inner: Rc<RefCell<dyn LpFs>>) -> Self {
        Self { inner }
    }

    /// `inner`, wrapped and shared the way a project holds its fs.
    #[must_use]
    pub fn guard(inner: Rc<RefCell<dyn LpFs>>) -> Rc<RefCell<dyn LpFs>> {
        Rc::new(RefCell::new(Self::new(inner)))
    }
}

impl LpFs for AccessGuardedFs {
    fn read_file(&self, path: &LpPath) -> Result<Vec<u8>, FsError> {
        if lpc_access::is_access_file_path(path.as_str()) {
            return Err(FsError::Filesystem(format!(
                "{}: access files are not readable by a project",
                path.as_str()
            )));
        }
        self.inner.borrow().read_file(path)
    }

    fn write_file(&self, path: &LpPath, data: &[u8]) -> Result<(), FsError> {
        self.inner.borrow().write_file(path, data)
    }

    // Delegated rather than defaulted: the trait's default appends by
    // reading the file first, which this wrapper would refuse.
    fn append_file(&self, path: &LpPath, data: &[u8]) -> Result<(), FsError> {
        self.inner.borrow().append_file(path, data)
    }

    fn file_size(&self, path: &LpPath) -> Result<u64, FsError> {
        self.inner.borrow().file_size(path)
    }

    fn file_exists(&self, path: &LpPath) -> Result<bool, FsError> {
        self.inner.borrow().file_exists(path)
    }

    fn is_dir(&self, path: &LpPath) -> Result<bool, FsError> {
        self.inner.borrow().is_dir(path)
    }

    fn list_dir(&self, path: &LpPath, recursive: bool) -> Result<Vec<LpPathBuf>, FsError> {
        self.inner.borrow().list_dir(path, recursive)
    }

    fn delete_file(&self, path: &LpPath) -> Result<(), FsError> {
        self.inner.borrow().delete_file(path)
    }

    fn delete_dir(&self, path: &LpPath) -> Result<(), FsError> {
        self.inner.borrow().delete_dir(path)
    }

    // A view of a view is still a project's view: it stays guarded.
    fn chroot(&self, subdir: &LpPath) -> Result<Rc<RefCell<dyn LpFs>>, FsError> {
        Ok(Self::guard(self.inner.borrow().chroot(subdir)?))
    }

    fn current_version(&self) -> FsVersion {
        self.inner.borrow().current_version()
    }

    fn get_changes_since(&self, since_version: FsVersion) -> Vec<FsEvent> {
        self.inner.borrow().get_changes_since(since_version)
    }

    fn get_events_since(&self, since_version: FsVersion) -> Vec<FsEvent> {
        self.inner.borrow().get_events_since(since_version)
    }

    fn clear_changes_before(&mut self, before_version: FsVersion) {
        self.inner.borrow_mut().clear_changes_before(before_version);
    }

    fn record_changes(&mut self, changes: Vec<FsEvent>) {
        self.inner.borrow_mut().record_changes(changes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpfs::LpFsMemory;
    use lpfs::lp_path::AsLpPath;

    #[test]
    fn access_files_are_not_read_and_everything_else_is() {
        let base = LpFsMemory::new();
        base.write_file("/.lp/access.json".as_path(), b"secret")
            .unwrap();
        base.write_file("/.lp/state.json".as_path(), b"state")
            .unwrap();
        base.write_file("/shader.glsl".as_path(), b"glsl").unwrap();
        let fs = AccessGuardedFs::new(Rc::new(RefCell::new(base)));

        for path in ["/.lp/access.json", ".lp/access.json", "/.LP/Access.json"] {
            assert!(fs.read_file(path.as_path()).is_err(), "{path} is refused");
        }
        assert_eq!(fs.read_file("/.lp/state.json".as_path()).unwrap(), b"state");
        assert_eq!(fs.read_file("/shader.glsl".as_path()).unwrap(), b"glsl");
        // The file is still there to write, name and delete.
        assert!(fs.file_exists("/.lp/access.json".as_path()).unwrap());
        fs.write_file("/.lp/access.json".as_path(), b"rotated")
            .unwrap();
        fs.delete_file("/.lp/access.json".as_path()).unwrap();
    }

    #[test]
    fn a_chroot_of_the_guard_is_guarded_too() {
        let base = LpFsMemory::new();
        base.write_file("/sub/.lp/access.json".as_path(), b"secret")
            .unwrap();
        let fs = AccessGuardedFs::new(Rc::new(RefCell::new(base)));
        let view = fs.chroot("/sub".as_path()).unwrap();
        assert!(
            view.borrow()
                .read_file("/.lp/access.json".as_path())
                .is_err()
        );
    }
}
