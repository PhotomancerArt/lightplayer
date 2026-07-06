//! The local project store: memory-primary `LpFs` over OPFS.
//!
//! All sync `LpFs` operations hit an in-memory filesystem; a driven flusher
//! drains the change log to OPFS (see [`crate::flusher`]). The store is
//! cheaply cloneable and clones share all state — this matters for `chroot`:
//! views are built over a clone of the store itself, so writes through any
//! view land in the *shared* change log the flusher drains. (Delegating
//! `chroot` to the inner memory fs would fork the change log — its chroot
//! clones change-tracking state rather than sharing it.)
//!
//! Borrow discipline: no `RefCell` borrow is ever held across an `await` —
//! flushing snapshots dirty state synchronously, then performs async OPFS IO
//! with no borrows outstanding.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use lpfs::{
    FsError, FsEvent, FsEventKind, FsVersion, LpFs, LpFsMemory, LpFsView, LpPath, LpPathBuf,
};
use web_sys::FileSystemDirectoryHandle;

use crate::opfs_error::OpfsError;
use crate::opfs_read::load_tree;
use crate::opfs_write::{remove_path, write_file};

struct Shared {
    inner: RefCell<LpFsMemory>,
    dir: FileSystemDirectoryHandle,
    /// Everything at or below this version is already on OPFS.
    watermark: Cell<FsVersion>,
}

/// Memory-primary `LpFs` backed by an OPFS directory.
///
/// Clones share all state.
#[derive(Clone)]
pub struct LpFsOpfs {
    shared: Rc<Shared>,
}

/// What a flush accomplished.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlushReport {
    pub files_written: usize,
    pub paths_removed: usize,
}

impl LpFsOpfs {
    /// Load the OPFS directory into memory and wrap it as a store.
    pub async fn mount(dir: FileSystemDirectoryHandle) -> Result<Self, OpfsError> {
        let inner = LpFsMemory::new();
        for (path, bytes) in load_tree(&dir).await? {
            inner.write_file(path.as_path(), &bytes).map_err(|e| {
                OpfsError::new("mount", path.as_str().to_string(), e.to_string().into())
            })?;
        }
        let watermark = inner.current_version();
        Ok(Self {
            shared: Rc::new(Shared {
                inner: RefCell::new(inner),
                dir,
                watermark: Cell::new(watermark),
            }),
        })
    }

    /// Whether unflushed changes exist.
    pub fn has_dirty(&self) -> bool {
        !self
            .shared
            .inner
            .borrow()
            .get_changes_since(self.shared.watermark.get().next())
            .is_empty()
    }

    /// Drain and persist all pending changes to OPFS.
    ///
    /// Snapshots the dirty set and file contents synchronously, then writes
    /// with no borrows held. On any IO failure the watermark does not
    /// advance, so the next flush retries everything still dirty
    /// (idempotent whole-file writes).
    pub async fn flush(&self) -> Result<FlushReport, OpfsError> {
        // synchronous snapshot: events + bytes, borrows dropped before IO
        let (batch, target_version) = {
            let inner = self.shared.inner.borrow();
            let events = inner.get_changes_since(self.shared.watermark.get().next());
            let batch: Vec<(LpPathBuf, Option<Vec<u8>>)> = events
                .into_iter()
                .map(|FsEvent { path, kind }| {
                    let bytes = match kind {
                        FsEventKind::Delete => None,
                        _ => inner.read_file(path.as_path()).ok(),
                    };
                    (path, bytes)
                })
                .collect();
            (batch, inner.current_version())
        };

        let mut report = FlushReport {
            files_written: 0,
            paths_removed: 0,
        };
        for (path, bytes) in &batch {
            match bytes {
                Some(bytes) => {
                    write_file(&self.shared.dir, path.as_path(), bytes).await?;
                    report.files_written += 1;
                }
                None => {
                    // never-flushed or already-removed paths are fine
                    if remove_path(&self.shared.dir, path.as_path()).await.is_ok() {
                        report.paths_removed += 1;
                    }
                }
            }
        }

        self.shared.watermark.set(target_version);
        self.shared
            .inner
            .borrow_mut()
            .clear_changes_before(target_version);
        Ok(report)
    }
}

impl LpFs for LpFsOpfs {
    fn read_file(&self, path: &LpPath) -> Result<Vec<u8>, FsError> {
        self.shared.inner.borrow().read_file(path)
    }

    fn write_file(&self, path: &LpPath, data: &[u8]) -> Result<(), FsError> {
        self.shared.inner.borrow().write_file(path, data)
    }

    fn file_exists(&self, path: &LpPath) -> Result<bool, FsError> {
        self.shared.inner.borrow().file_exists(path)
    }

    fn is_dir(&self, path: &LpPath) -> Result<bool, FsError> {
        self.shared.inner.borrow().is_dir(path)
    }

    fn list_dir(&self, path: &LpPath, recursive: bool) -> Result<Vec<LpPathBuf>, FsError> {
        self.shared.inner.borrow().list_dir(path, recursive)
    }

    fn delete_file(&self, path: &LpPath) -> Result<(), FsError> {
        self.shared.inner.borrow().delete_file(path)
    }

    fn delete_dir(&self, path: &LpPath) -> Result<(), FsError> {
        self.shared.inner.borrow().delete_dir(path)
    }

    fn chroot(&self, subdir: &LpPath) -> Result<Rc<RefCell<dyn LpFs>>, FsError> {
        // view over a CLONE OF THE STORE (shared state), not the inner
        // memory fs — see module docs: this keeps the change log unified.
        let prefix = if subdir.as_str().ends_with('/') {
            subdir.to_path_buf()
        } else {
            LpPathBuf::from(format!("{}/", subdir.as_str()))
        };
        let parent: Rc<RefCell<dyn LpFs>> = Rc::new(RefCell::new(self.clone()));
        Ok(Rc::new(RefCell::new(LpFsView::new(
            parent,
            prefix.as_path(),
        ))))
    }

    fn current_version(&self) -> FsVersion {
        self.shared.inner.borrow().current_version()
    }

    fn get_changes_since(&self, since_version: FsVersion) -> Vec<FsEvent> {
        self.shared.inner.borrow().get_changes_since(since_version)
    }

    fn clear_changes_before(&mut self, before_version: FsVersion) {
        self.shared
            .inner
            .borrow_mut()
            .clear_changes_before(before_version);
    }

    fn record_changes(&mut self, changes: Vec<FsEvent>) {
        self.shared.inner.borrow_mut().record_changes(changes);
    }
}
