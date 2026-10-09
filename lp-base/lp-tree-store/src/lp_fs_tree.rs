//! `lpfs::LpFs` over the tree store (feature `lpfs`): what the firmware
//! will call. Semantics follow `LpFsMemory` (directories are implicit; a
//! recursive listing names every directory at every depth, as the trait
//! says), the change log follows the littlefs adapter (RAM only, latest
//! change per path), and `chroot` is `LpFsView` over a handle sharing this
//! one's store and change log. The batch methods are the store's
//! transaction.

use alloc::format;
use alloc::rc::Rc;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::cell::RefCell;
use core::fmt::Debug;

use lpfs::{FsError, FsEvent, FsEventKind, FsVersion, LpFs, LpFsView, LpPath, LpPathBuf};

use crate::flash::Flash;
use crate::heap_sort::sort_strings;
use crate::object_hasher::ObjectHasher;
use crate::store_error::StoreError;
use crate::tree_store::TreeStore;

/// The tree store as an `LpFs`.
pub struct LpFsTree<F: Flash, H: ObjectHasher> {
    inner: Rc<RefCell<Inner<F, H>>>,
}

struct Inner<F: Flash, H: ObjectHasher> {
    store: TreeStore<F, H>,
    version: FsVersion,
    changes: Vec<(LpPathBuf, FsVersion, FsEventKind)>,
}

impl<F: Flash, H: ObjectHasher> LpFsTree<F, H> {
    pub fn new(store: TreeStore<F, H>) -> Self {
        Self {
            inner: Rc::new(RefCell::new(Inner {
                store,
                version: FsVersion::default(),
                changes: Vec::new(),
            })),
        }
    }

    /// The store underneath (stats, a harness's flash access).
    pub fn with_store<T>(&self, f: impl FnOnce(&mut TreeStore<F, H>) -> T) -> T {
        f(&mut self.inner.borrow_mut().store)
    }

    fn record(&self, path: &str, kind: FsEventKind) {
        let mut i = self.inner.borrow_mut();
        i.version = i.version.next();
        let v = i.version;
        let p = LpPathBuf::from(path);
        match i.changes.iter_mut().find(|c| c.0 == p) {
            Some(c) => {
                c.1 = v;
                c.2 = kind;
            }
            None => i.changes.push((p, v, kind)),
        }
    }

    fn files_under(&self, dir: &str) -> Result<Vec<String>, FsError> {
        let prefix = if dir == "/" {
            String::from("/")
        } else {
            format!("{dir}/")
        };
        self.inner
            .borrow_mut()
            .store
            .list(&prefix)
            .map_err(store_err)
    }
}

/// An absolute, normalized path string ("/" for the root).
fn norm(path: &LpPath) -> Result<String, FsError> {
    if !path.is_absolute() {
        return Err(FsError::InvalidPath(format!(
            "Path must be absolute: {}",
            path.as_str()
        )));
    }
    let s = path.to_path_buf().as_str().to_string();
    let t = s.trim_end_matches('/');
    Ok(if t.is_empty() {
        String::from("/")
    } else {
        String::from(t)
    })
}

fn store_err<E: Debug>(e: StoreError<E>) -> FsError {
    FsError::Filesystem(format!("tree store: {e:?}"))
}

fn not_found(p: &str) -> FsError {
    FsError::NotFound(p.to_string())
}

impl<F: Flash + 'static, H: ObjectHasher + 'static> LpFs for LpFsTree<F, H> {
    fn read_file(&self, path: &LpPath) -> Result<Vec<u8>, FsError> {
        let p = norm(path)?;
        let got = self.inner.borrow_mut().store.get(&p).map_err(store_err)?;
        got.ok_or_else(|| not_found(&p))
    }

    fn write_file(&self, path: &LpPath, data: &[u8]) -> Result<(), FsError> {
        let p = norm(path)?;
        if p == "/" {
            return Err(FsError::InvalidPath("Cannot write to root".to_string()));
        }
        let existed = {
            let st = &mut self.inner.borrow_mut().store;
            let existed = st.exists(&p).map_err(store_err)?;
            st.put(&p, data).map_err(store_err)?;
            existed
        };
        self.record(
            &p,
            if existed {
                FsEventKind::Modify
            } else {
                FsEventKind::Create
            },
        );
        Ok(())
    }

    fn append_file(&self, path: &LpPath, data: &[u8]) -> Result<(), FsError> {
        let p = norm(path)?;
        if p == "/" {
            return Err(FsError::InvalidPath("Cannot write to root".to_string()));
        }
        let existed = {
            let st = &mut self.inner.borrow_mut().store;
            let existed = st.exists(&p).map_err(store_err)?;
            st.append(&p, data).map_err(store_err)?;
            existed
        };
        self.record(
            &p,
            if existed {
                FsEventKind::Modify
            } else {
                FsEventKind::Create
            },
        );
        Ok(())
    }

    fn file_size(&self, path: &LpPath) -> Result<u64, FsError> {
        let p = norm(path)?;
        let size = self
            .inner
            .borrow_mut()
            .store
            .file_size(&p)
            .map_err(store_err)?;
        size.map(u64::from).ok_or_else(|| not_found(&p))
    }

    fn file_exists(&self, path: &LpPath) -> Result<bool, FsError> {
        let p = norm(path)?;
        self.inner.borrow_mut().store.exists(&p).map_err(store_err)
    }

    fn is_dir(&self, path: &LpPath) -> Result<bool, FsError> {
        let p = norm(path)?;
        if p == "/" {
            return Ok(true);
        }
        if self.file_exists(LpPath::new(&p))? {
            return Ok(false);
        }
        if self.files_under(&p)?.is_empty() {
            return Err(not_found(&p));
        }
        Ok(true)
    }

    fn list_dir(&self, path: &LpPath, recursive: bool) -> Result<Vec<LpPathBuf>, FsError> {
        let p = norm(path)?;
        let base = if p == "/" { 0 } else { p.len() };
        let mut out: Vec<String> = Vec::new();
        for f in self.files_under(&p)? {
            let rel = &f[base + 1..];
            // Every directory between `p` and the file (recursive), or the
            // first one only.
            let mut end = 0;
            while let Some(k) = rel[end..].find('/') {
                out.push(format!("{}/{}", &f[..base], &rel[..end + k]));
                end += k + 1;
                if !recursive {
                    break;
                }
            }
            if recursive || !rel.contains('/') {
                out.push(f.clone());
            }
        }
        sort_strings(&mut out);
        out.dedup();
        Ok(out.into_iter().map(LpPathBuf::from).collect())
    }

    fn delete_file(&self, path: &LpPath) -> Result<(), FsError> {
        let p = norm(path)?;
        if p == "/" {
            return Err(FsError::InvalidPath(
                "Cannot delete root directory".to_string(),
            ));
        }
        if !self.files_under(&p)?.is_empty() {
            return Err(FsError::Filesystem(format!(
                "Path {p:?} is a directory, use delete_dir() instead"
            )));
        }
        let gone = self
            .inner
            .borrow_mut()
            .store
            .delete(&p)
            .map_err(store_err)?;
        if !gone {
            return Err(not_found(&p));
        }
        self.record(&p, FsEventKind::Delete);
        Ok(())
    }

    fn delete_dir(&self, path: &LpPath) -> Result<(), FsError> {
        let p = norm(path)?;
        if p == "/" {
            return Err(FsError::InvalidPath(
                "Cannot delete root directory".to_string(),
            ));
        }
        let mut gone = self.files_under(&p)?;
        let is_file = self.file_exists(LpPath::new(&p))?;
        if gone.is_empty() && !is_file {
            return Err(not_found(&p));
        }
        self.inner
            .borrow_mut()
            .store
            .delete_file_and_tree(&p)
            .map_err(store_err)?;
        if is_file {
            gone.push(p);
        }
        for f in gone {
            self.record(&f, FsEventKind::Delete);
        }
        Ok(())
    }

    fn chroot(&self, subdir: &LpPath) -> Result<Rc<RefCell<dyn LpFs>>, FsError> {
        let p = norm(subdir)?;
        let prefix = if p == "/" {
            String::from("/")
        } else {
            format!("{p}/")
        };
        let parent = Rc::new(RefCell::new(LpFsTree {
            inner: Rc::clone(&self.inner),
        }));
        Ok(Rc::new(RefCell::new(LpFsView::new(
            parent,
            LpPath::new(&prefix),
        ))))
    }

    fn begin_batch(&self) -> Result<(), FsError> {
        self.inner.borrow_mut().store.begin().map_err(store_err)
    }

    fn commit_batch(&self) -> Result<(), FsError> {
        self.inner.borrow_mut().store.commit().map_err(store_err)
    }

    fn abort_batch(&self) -> Result<(), FsError> {
        self.inner.borrow_mut().store.abort().map_err(store_err)
    }

    fn current_version(&self) -> FsVersion {
        self.inner.borrow().version
    }

    fn get_changes_since(&self, since_version: FsVersion) -> Vec<FsEvent> {
        self.inner
            .borrow()
            .changes
            .iter()
            .filter(|c| c.1 >= since_version)
            .map(|c| FsEvent {
                path: c.0.clone(),
                kind: c.2,
            })
            .collect()
    }

    fn clear_changes_before(&mut self, before_version: FsVersion) {
        self.inner
            .borrow_mut()
            .changes
            .retain(|c| c.1 >= before_version);
    }

    fn record_changes(&mut self, changes: Vec<FsEvent>) {
        for c in changes {
            self.record(c.path.as_str(), c.kind);
        }
    }
}
