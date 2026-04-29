//! Flash-backed LpFs implementation using littlefs-rust.
//!
//! Wraps `littlefs_rust::Filesystem<LpFlashStorage>` and implements the
//! `LpFs` trait for use with LpServer.

use alloc::{format, rc::Rc, string::ToString, vec::Vec};
use core::cell::RefCell;
use hashbrown::HashMap;

use lpc_model::path::{LpPath, LpPathBuf};
use lpfs::{ChangeType, FsChange, FsError, FsVersion, LpFs, LpFsMemory, LpFsView};

use crate::flash_storage::{LpFlashStorage, lpfs_config};
use littlefs_rust::{Error as LfsError, FileType as LfsFileType, Filesystem};

/// Flash-backed filesystem implementing LpFs.
///
/// Uses littlefs-rust over the lpfs partition. Supports chroot via LpFsView.
pub struct LpFsFlash {
    inner: Rc<RefCell<LpFsFlashInner>>,
}

struct LpFsFlashInner {
    fs: Filesystem<LpFlashStorage>,
    current_version: FsVersion,
    changes: HashMap<LpPathBuf, (FsVersion, ChangeType)>,
}

impl LpFsFlash {
    /// Initialize flash filesystem by mounting the lpfs partition.
    ///
    /// If the partition is unformatted or corrupted, formats it and retries.
    /// Returns `Err` only if both mount and format-then-mount fail.
    pub fn init(flash: esp_storage::FlashStorage<'static>) -> Result<Self, LfsError> {
        let storage = LpFlashStorage::new(flash);
        let config = lpfs_config();

        let (mut storage, config) = match Filesystem::mount(storage, config) {
            Ok(fs) => return Ok(Self::from_fs(fs)),
            Err((e, storage)) => {
                esp_println::println!("[FS] Mount failed ({e}), formatting partition...");
                (storage, lpfs_config())
            }
        };

        Filesystem::format(&mut storage, &config).map_err(|e| {
            esp_println::println!("[FS] Format failed: {e}");
            e
        })?;

        let fs = Filesystem::mount(storage, config).map_err(|(e, _)| {
            esp_println::println!("[FS] Mount after format failed: {e}");
            e
        })?;

        esp_println::println!("[FS] Formatted and mounted fresh filesystem");
        Ok(Self::from_fs(fs))
    }

    fn from_fs(fs: Filesystem<LpFlashStorage>) -> Self {
        LpFsFlash {
            inner: Rc::new(RefCell::new(LpFsFlashInner {
                fs,
                current_version: FsVersion::default(),
                changes: HashMap::new(),
            })),
        }
    }

    fn record_change(&self, path: &LpPath, change_type: ChangeType) {
        let mut inner = self.inner.borrow_mut();
        inner.current_version = inner.current_version.next();
        let version = inner.current_version;
        inner
            .changes
            .insert(path.to_path_buf(), (version, change_type));
    }

    /// Convert LpPath to littlefs path (strip leading /)
    fn to_lfs_path(path: &LpPath) -> &str {
        let s = path.as_str();
        if s == "/" {
            ""
        } else if let Some(stripped) = s.strip_prefix('/') {
            stripped
        } else {
            s
        }
    }

    /// Ensure parent directories exist for a path (for write_file)
    fn ensure_parent_dirs(&self, path: &str) -> Result<(), FsError> {
        if path.is_empty() {
            return Ok(());
        }
        let mut components = path.split('/').collect::<Vec<_>>();
        components.pop(); // Remove the file name
        if components.is_empty() {
            return Ok(());
        }
        let mut current = alloc::string::String::new();
        for (i, comp) in components.iter().enumerate() {
            if i > 0 {
                current.push('/');
            }
            current.push_str(comp);
            {
                let inner = self.inner.borrow_mut();
                match inner.fs.mkdir(&current) {
                    Ok(()) => {}
                    Err(LfsError::Exists) => {}
                    Err(e) => {
                        return Err(FsError::Filesystem(format!("mkdir {current}: {e}")));
                    }
                }
            }
        }
        Ok(())
    }

    /// Recursively delete a directory (littlefs remove only works on empty dirs)
    fn delete_dir_recursive(&self, path: &str) -> Result<(), FsError> {
        let entries = {
            let inner = self.inner.borrow();
            inner
                .fs
                .list_dir(path)
                .map_err(|e| FsError::Filesystem(format!("list_dir {path}: {e}")))?
        };
        for entry in entries {
            let child_path = if path.is_empty() {
                entry.name.clone()
            } else {
                format!("{}/{}", path, entry.name)
            };
            match entry.file_type {
                LfsFileType::Dir => self.delete_dir_recursive(&child_path)?,
                LfsFileType::File => {
                    let inner = self.inner.borrow_mut();
                    inner.fs.remove(&child_path).map_err(|e| {
                        FsError::Filesystem(format!("remove file {child_path}: {e}"))
                    })?;
                }
            }
        }
        let inner = self.inner.borrow_mut();
        inner
            .fs
            .remove(path)
            .map_err(|e| FsError::Filesystem(format!("remove dir {path}: {e}")))?;
        Ok(())
    }
}

impl LpFs for LpFsFlash {
    fn read_file(&self, path: &LpPath) -> Result<Vec<u8>, FsError> {
        if !path.is_absolute() {
            return Err(FsError::InvalidPath(format!(
                "Path must be absolute: {}",
                path.as_str()
            )));
        }
        let lfs_path = Self::to_lfs_path(path);
        let inner = self.inner.borrow();
        inner
            .fs
            .read_to_vec(lfs_path)
            .map_err(|e| FsError::NotFound(format!("{}: {}", path.as_str(), e)))
    }

    fn write_file(&self, path: &LpPath, data: &[u8]) -> Result<(), FsError> {
        if !path.is_absolute() {
            return Err(FsError::InvalidPath(format!(
                "Path must be absolute: {}",
                path.as_str()
            )));
        }
        let lfs_path = Self::to_lfs_path(path);
        if lfs_path.is_empty() {
            return Err(FsError::InvalidPath("Cannot write to root".to_string()));
        }
        self.ensure_parent_dirs(lfs_path)?;
        let existed = {
            let inner = self.inner.borrow();
            inner.fs.exists(lfs_path)
        };
        let inner = self.inner.borrow_mut();
        inner
            .fs
            .write_file(lfs_path, data)
            .map_err(|e| FsError::Filesystem(format!("write {}: {e}", path.as_str())))?;
        drop(inner);
        let change_type = if existed {
            ChangeType::Modify
        } else {
            ChangeType::Create
        };
        self.record_change(path, change_type);
        Ok(())
    }

    fn file_exists(&self, path: &LpPath) -> Result<bool, FsError> {
        if !path.is_absolute() {
            return Err(FsError::InvalidPath(format!(
                "Path must be absolute: {}",
                path.as_str()
            )));
        }
        let lfs_path = Self::to_lfs_path(path);
        let inner = self.inner.borrow();
        Ok(inner.fs.exists(lfs_path))
    }

    fn is_dir(&self, path: &LpPath) -> Result<bool, FsError> {
        if !path.is_absolute() {
            return Err(FsError::InvalidPath(format!(
                "Path must be absolute: {}",
                path.as_str()
            )));
        }
        let lfs_path = Self::to_lfs_path(path);
        let inner = self.inner.borrow();
        match inner.fs.stat(lfs_path) {
            Ok(meta) => Ok(meta.file_type == LfsFileType::Dir),
            Err(LfsError::NoEntry) => Err(FsError::NotFound(path.as_str().to_string())),
            Err(e) => Err(FsError::Filesystem(format!("stat {}: {e}", path.as_str()))),
        }
    }

    fn list_dir(&self, path: &LpPath, recursive: bool) -> Result<Vec<LpPathBuf>, FsError> {
        if !path.is_absolute() {
            return Err(FsError::InvalidPath(format!(
                "Path must be absolute: {}",
                path.as_str()
            )));
        }
        let lfs_path = Self::to_lfs_path(path);
        let prefix = if path.as_str() == "/" {
            "/"
        } else if path.as_str().ends_with('/') {
            path.as_str()
        } else {
            &format!("{}/", path.as_str())
        };

        let mut entries = Vec::new();
        let inner = self.inner.borrow();

        if recursive {
            fn list_recursive<S: littlefs_rust::Storage>(
                fs: &littlefs_rust::Filesystem<S>,
                path: &str,
                prefix: &str,
                entries: &mut Vec<LpPathBuf>,
            ) -> Result<(), FsError> {
                let items = fs
                    .list_dir(path)
                    .map_err(|e| FsError::Filesystem(format!("list_dir {path}: {e}")))?;
                for item in items {
                    let full_lfs = if path.is_empty() {
                        item.name.clone()
                    } else {
                        format!("{}/{}", path, item.name)
                    };
                    let full_lp = if prefix == "/" {
                        format!("/{full_lfs}")
                    } else {
                        format!("{}/{full_lfs}", prefix.trim_end_matches('/'))
                    };
                    let full_lp = if !full_lp.starts_with('/') {
                        format!("/{full_lp}")
                    } else {
                        full_lp
                    };
                    entries.push(LpPathBuf::from(full_lp.as_str()));
                    if item.file_type == LfsFileType::Dir {
                        list_recursive(fs, &full_lfs, prefix, entries)?;
                    }
                }
                Ok(())
            }
            list_recursive(&inner.fs, lfs_path, prefix, &mut entries)?;
        } else {
            let items = inner
                .fs
                .list_dir(lfs_path)
                .map_err(|e| FsError::Filesystem(format!("list_dir {}: {e}", path.as_str())))?;
            for item in items {
                let full_lp = if prefix == "/" {
                    format!("/{}", item.name)
                } else if prefix.ends_with('/') {
                    format!("{}{}", prefix, item.name)
                } else {
                    format!("{}/{}", prefix, item.name)
                };
                let full_lp = if !full_lp.starts_with('/') {
                    format!("/{full_lp}")
                } else {
                    full_lp
                };
                entries.push(LpPathBuf::from(full_lp.as_str()));
            }
        }

        Ok(entries)
    }

    fn delete_file(&self, path: &LpPath) -> Result<(), FsError> {
        LpFsMemory::validate_path_for_deletion(path)?;
        if !path.is_absolute() {
            return Err(FsError::InvalidPath(format!(
                "Path must be absolute: {}",
                path.as_str()
            )));
        }
        let lfs_path = Self::to_lfs_path(path);
        let inner = self.inner.borrow_mut();
        inner.fs.remove(lfs_path).map_err(|e| {
            if e == LfsError::IsDir {
                FsError::Filesystem(format!(
                    "Path {} is a directory, use delete_dir() instead",
                    path.as_str()
                ))
            } else if e == LfsError::NoEntry {
                FsError::NotFound(path.as_str().to_string())
            } else {
                FsError::Filesystem(format!("remove {}: {e}", path.as_str()))
            }
        })?;
        drop(inner);
        self.record_change(path, ChangeType::Delete);
        Ok(())
    }

    fn delete_dir(&self, path: &LpPath) -> Result<(), FsError> {
        LpFsMemory::validate_path_for_deletion(path)?;
        if !path.is_absolute() {
            return Err(FsError::InvalidPath(format!(
                "Path must be absolute: {}",
                path.as_str()
            )));
        }
        let lfs_path = Self::to_lfs_path(path);
        self.delete_dir_recursive(lfs_path)?;
        self.record_change(path, ChangeType::Delete);
        Ok(())
    }

    fn chroot(&self, subdir: &LpPath) -> Result<Rc<RefCell<dyn LpFs>>, FsError> {
        if !subdir.is_absolute() {
            return Err(FsError::InvalidPath(format!(
                "Path must be absolute: {}",
                subdir.as_str()
            )));
        }
        let prefix = if subdir.as_str().ends_with('/') {
            subdir.to_path_buf()
        } else {
            LpPathBuf::from(format!("{}/", subdir.as_str()).as_str())
        };
        let parent = Rc::new(RefCell::new(LpFsFlash {
            inner: Rc::clone(&self.inner),
        }));
        Ok(Rc::new(RefCell::new(LpFsView::new(
            parent,
            prefix.as_path(),
        ))))
    }

    fn current_version(&self) -> FsVersion {
        self.inner.borrow().current_version
    }

    fn get_changes_since(&self, since_version: FsVersion) -> Vec<FsChange> {
        self.inner
            .borrow()
            .changes
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
        self.inner
            .borrow_mut()
            .changes
            .retain(|_, (version, _)| *version >= before_version);
    }

    fn record_changes(&mut self, changes: Vec<FsChange>) {
        for change in changes {
            self.record_change(change.path.as_path(), change.change_type);
        }
    }
}
