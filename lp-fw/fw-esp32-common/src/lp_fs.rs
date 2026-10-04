//! Flash-backed LpFs implementation using littlefs-rust.
//!
//! Wraps `littlefs_rust::Filesystem<S>` over a chip-supplied
//! `littlefs_rust::Storage` implementation and implements the `LpFs` trait
//! for use with LpServer. The chip crate owns the flash adapter and the
//! partition-derived littlefs `Config`.

use alloc::{format, rc::Rc, string::ToString, vec, vec::Vec};
use core::cell::RefCell;
use hashbrown::HashMap;

use lpfs::lp_path::{LpPath, LpPathBuf};
use lpfs::{FsError, FsEvent, FsEventKind, FsVersion, LpFs, LpFsMemory, LpFsView};

use littlefs_rust::{
    Config, Error as LfsError, FileType as LfsFileType, Filesystem, OpenFlags, Storage,
};

/// Flash-backed filesystem implementing LpFs.
///
/// Uses littlefs-rust over the lpfs partition. Supports chroot via LpFsView.
pub struct LpFsFlash<S: Storage> {
    inner: Rc<RefCell<LpFsFlashInner<S>>>,
}

struct LpFsFlashInner<S: Storage> {
    fs: Filesystem<S>,
    current_version: FsVersion,
    changes: HashMap<LpPathBuf, (FsVersion, FsEventKind)>,
}

/// What [`LpFsFlash::init_guarded`] is told before it formats a partition
/// that would not mount.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatVerdict {
    /// Nothing worth keeping is there: format it.
    Format,
    /// Something worth keeping is there (on the C6, a pre-repartition
    /// filesystem the new partition overlaps): do NOT format.
    Hold,
}

/// How [`LpFsFlash::init_guarded`] came out.
pub enum FlashFsInit<S: Storage> {
    /// An existing filesystem mounted.
    Mounted(LpFsFlash<S>),
    /// None mounted; the partition was formatted and mounted fresh.
    Formatted(LpFsFlash<S>),
    /// None mounted, and the verdict said [`FormatVerdict::Hold`]: nothing was
    /// written. The caller serves a RAM filesystem.
    Held,
}

impl<S: Storage> LpFsFlash<S> {
    /// Initialize flash filesystem by mounting the lpfs partition.
    ///
    /// If the partition is unformatted or corrupted, formats it and retries.
    /// Returns `Err` only if both mount and format-then-mount fail. The bool
    /// is `true` when it had to format (the hello's `formatted`).
    pub fn init(storage: S, make_config: fn() -> Config) -> Result<(Self, bool), LfsError> {
        match Self::init_guarded(storage, make_config, |_| FormatVerdict::Format)? {
            FlashFsInit::Mounted(fs) => Ok((fs, false)),
            FlashFsInit::Formatted(fs) => Ok((fs, true)),
            // The verdict above never holds.
            FlashFsInit::Held => Err(LfsError::Io),
        }
    }

    /// [`Self::init`], asking `may_format` before it formats.
    ///
    /// `may_format` gets the storage the failed mount handed back, so a chip
    /// can look elsewhere on the same flash (the C6 probes its pre-repartition
    /// filesystem read-only — see `lpfs_mounts_read_only`). This crate stays
    /// layout-agnostic: the S3 and the classic call [`Self::init`], which
    /// always formats.
    pub fn init_guarded(
        storage: S,
        make_config: fn() -> Config,
        may_format: impl FnOnce(&mut S) -> FormatVerdict,
    ) -> Result<FlashFsInit<S>, LfsError> {
        let config = make_config();

        let (mut storage, config, mount_error) = match Filesystem::mount(storage, config) {
            Ok(fs) => return Ok(FlashFsInit::Mounted(Self::from_fs(fs))),
            Err((e, storage)) => (storage, make_config(), e),
        };

        if may_format(&mut storage) == FormatVerdict::Hold {
            log::warn!("[FS] Mount failed ({mount_error}), holding: not formatting");
            return Ok(FlashFsInit::Held);
        }
        // The exact line the validation payloads parse (`lp-emu-validate`'s
        // `[FS] Mount failed …` pattern): keep its words.
        log::warn!("[FS] Mount failed ({mount_error}), formatting partition...");

        Filesystem::format(&mut storage, &config).map_err(|e| {
            log::warn!("[FS] Format failed: {e}");
            e
        })?;

        let fs = Filesystem::mount(storage, config).map_err(|(e, _)| {
            log::warn!("[FS] Mount after format failed: {e}");
            e
        })?;

        log::info!("[FS] Formatted and mounted fresh filesystem");
        Ok(FlashFsInit::Formatted(Self::from_fs(fs)))
    }

    fn from_fs(fs: Filesystem<S>) -> Self {
        LpFsFlash {
            inner: Rc::new(RefCell::new(LpFsFlashInner {
                fs,
                current_version: FsVersion::default(),
                changes: HashMap::new(),
            })),
        }
    }

    fn record_change(&self, path: &LpPath, kind: FsEventKind) {
        let mut inner = self.inner.borrow_mut();
        inner.current_version = inner.current_version.next();
        let version = inner.current_version;
        inner.changes.insert(path.to_path_buf(), (version, kind));
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

fn map_lfs_read_error(path: &LpPath, error: LfsError) -> FsError {
    match error {
        LfsError::NoEntry => FsError::NotFound(path.as_str().to_string()),
        other => FsError::Filesystem(format!("read {}: {other}", path.as_str())),
    }
}

impl<S: Storage + 'static> LpFs for LpFsFlash<S> {
    fn read_file(&self, path: &LpPath) -> Result<Vec<u8>, FsError> {
        if !path.is_absolute() {
            return Err(FsError::InvalidPath(format!(
                "Path must be absolute: {}",
                path.as_str()
            )));
        }
        let lfs_path = Self::to_lfs_path(path);
        let size = {
            let inner = self.inner.borrow();
            let meta = inner
                .fs
                .stat(lfs_path)
                .map_err(|e| map_lfs_read_error(path, e))?;
            if meta.file_type != LfsFileType::File {
                return Err(FsError::Filesystem(format!(
                    "{} is not a file",
                    path.as_str()
                )));
            }
            meta.size as usize
        };

        let mut buf = vec![0u8; size];
        if size == 0 {
            return Ok(buf);
        }

        let inner = self.inner.borrow();
        let file = inner
            .fs
            .open(lfs_path, OpenFlags::READ)
            .map_err(|e| map_lfs_read_error(path, e))?;
        let n = file
            .read(&mut buf)
            .map_err(|e| FsError::Filesystem(format!("read {}: {e}", path.as_str())))?;
        file.close()
            .map_err(|e| FsError::Filesystem(format!("close {}: {e}", path.as_str())))?;
        buf.truncate(n as usize);
        Ok(buf)
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
        let kind = if existed {
            FsEventKind::Modify
        } else {
            FsEventKind::Create
        };
        self.record_change(path, kind);
        Ok(())
    }

    /// Native append (M2b follow-up, landed in M5): open with APPEND and
    /// write the chunk — no whole-file RAM residency, O(chunk) instead of
    /// the trait default's read-modify-write. The device push path
    /// (WriteChunk) appends per chunk, so this is the hot path.
    fn append_file(&self, path: &LpPath, data: &[u8]) -> Result<(), FsError> {
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
        {
            let inner = self.inner.borrow_mut();
            let file = inner
                .fs
                .open(
                    lfs_path,
                    OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::APPEND,
                )
                .map_err(|e| FsError::Filesystem(format!("append {}: {e}", path.as_str())))?;
            file.write(data)
                .map_err(|e| FsError::Filesystem(format!("append {}: {e}", path.as_str())))?;
            file.close()
                .map_err(|e| FsError::Filesystem(format!("close {}: {e}", path.as_str())))?;
        }
        let kind = if existed {
            FsEventKind::Modify
        } else {
            FsEventKind::Create
        };
        self.record_change(path, kind);
        Ok(())
    }

    /// Native size via stat — the trait default reads the whole file.
    fn file_size(&self, path: &LpPath) -> Result<u64, FsError> {
        if !path.is_absolute() {
            return Err(FsError::InvalidPath(format!(
                "Path must be absolute: {}",
                path.as_str()
            )));
        }
        let lfs_path = Self::to_lfs_path(path);
        let inner = self.inner.borrow();
        let meta = inner
            .fs
            .stat(lfs_path)
            .map_err(|e| map_lfs_read_error(path, e))?;
        if meta.file_type != LfsFileType::File {
            return Err(FsError::Filesystem(format!(
                "{} is not a file",
                path.as_str()
            )));
        }
        Ok(meta.size as u64)
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
                    // `full_lfs` is already the ROOT-relative lfs path
                    // (`to_lfs_path` strips the leading slash), so the
                    // canonical LpPath is just "/" + it. Prepending the
                    // listing prefix here DOUBLED the base for every
                    // non-root recursive listing ("/projects/studio/
                    // projects/studio/…"), which broke the chroot view's
                    // strip and with it on-device hash_package (push
                    // verification).
                    let full_lp = format!("/{full_lfs}");
                    entries.push(LpPathBuf::from(full_lp.as_str()));
                    if item.file_type == LfsFileType::Dir {
                        list_recursive(fs, &full_lfs, entries)?;
                    }
                }
                Ok(())
            }
            list_recursive(&inner.fs, lfs_path, &mut entries)?;
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
        self.record_change(path, FsEventKind::Delete);
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
        self.record_change(path, FsEventKind::Delete);
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

    fn get_changes_since(&self, since_version: FsVersion) -> Vec<FsEvent> {
        self.inner
            .borrow()
            .changes
            .iter()
            .filter_map(|(path, (version, kind))| {
                if *version >= since_version {
                    Some(FsEvent {
                        path: path.clone(),
                        kind: *kind,
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

    fn record_changes(&mut self, changes: Vec<FsEvent>) {
        for change in changes {
            self.record_change(change.path.as_path(), change.kind);
        }
    }
}

/// Does a littlefs filesystem mount from `storage` with `config` — without
/// ever writing to it?
///
/// The pure half of the C6's legacy guard (plan
/// `lp2025/2026-10-01-1843-c6-repartition`, MQ2): before formatting an `lpfs`
/// that would not mount, the C6 asks whether a LightPlayer filesystem in the
/// pre-repartition layout is still at the old offset. littlefs's mount only
/// reads, and [`ReadOnlyStorage`] makes sure of it: any write or erase it
/// attempted would fail with `Io` rather than touch the flash. Mounted
/// filesystems are unmounted again (a read-only unmount writes nothing).
pub fn lpfs_mounts_read_only<S: Storage>(storage: S, config: Config) -> bool {
    match Filesystem::mount(ReadOnlyStorage(storage), config) {
        Ok(fs) => {
            let _ = fs.unmount();
            true
        }
        Err(_) => false,
    }
}

/// A littlefs `Storage` that forwards reads and refuses every write and
/// erase with `Io` — what [`lpfs_mounts_read_only`] probes through.
pub struct ReadOnlyStorage<S: Storage>(pub S);

impl<S: Storage> Storage for ReadOnlyStorage<S> {
    fn read(&mut self, block: u32, offset: u32, buf: &mut [u8]) -> Result<(), LfsError> {
        self.0.read(block, offset, buf)
    }

    fn write(&mut self, _block: u32, _offset: u32, _data: &[u8]) -> Result<(), LfsError> {
        Err(LfsError::Io)
    }

    fn erase(&mut self, _block: u32) -> Result<(), LfsError> {
        Err(LfsError::Io)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use littlefs_rust::RamStorage;

    const BLOCK: u32 = 4096;

    fn config(blocks: u32) -> Config {
        let mut c = Config::new(BLOCK, blocks);
        c.cache_size = 512;
        c.lookahead_size = 64;
        c
    }

    /// A 240-block filesystem holding files, the way a pre-repartition C6's
    /// `lpfs` does.
    fn legacy_image() -> RamStorage {
        let mut storage = RamStorage::new(BLOCK, 240);
        Filesystem::format(&mut storage, &config(240)).unwrap();
        let fs = Filesystem::mount(storage, config(240))
            .map_err(|(e, _)| e)
            .unwrap();
        fs.mkdir("projects").unwrap();
        fs.mkdir("projects/basic").unwrap();
        fs.write_file("projects/basic/project.json", b"{\"name\":\"basic\"}")
            .unwrap();
        fs.write_file("hardware.json", &[7u8; 5000]).unwrap();
        fs.unmount().unwrap()
    }

    #[test]
    fn a_filesystem_with_files_mounts_read_only_and_is_not_written() {
        let storage = legacy_image();
        let before = storage.data().to_vec();
        // Probe through a borrowed view so the bytes can be compared after.
        struct View<'a>(&'a mut RamStorage);
        impl Storage for View<'_> {
            fn read(&mut self, b: u32, o: u32, buf: &mut [u8]) -> Result<(), LfsError> {
                self.0.read(b, o, buf)
            }
            fn write(&mut self, b: u32, o: u32, d: &[u8]) -> Result<(), LfsError> {
                self.0.write(b, o, d)
            }
            fn erase(&mut self, b: u32) -> Result<(), LfsError> {
                self.0.erase(b)
            }
        }
        let mut storage = storage;
        assert!(lpfs_mounts_read_only(View(&mut storage), config(240)));
        assert_eq!(
            storage.data(),
            &before[..],
            "the probe wrote to the filesystem"
        );
    }

    #[test]
    fn blank_flash_does_not_mount() {
        assert!(!lpfs_mounts_read_only(
            RamStorage::new(BLOCK, 240),
            config(240)
        ));
    }

    /// The overlap the guard exists for: the post-repartition `lpfs` starts
    /// 64 blocks into the old one, so mounting at the new offset fails (no
    /// superblock there) while the old filesystem is still whole.
    #[test]
    fn the_new_region_inside_an_old_filesystem_does_not_mount_but_the_old_one_does() {
        let legacy = legacy_image();
        let tail = legacy.data()[64 * BLOCK as usize..].to_vec();
        let mut new_region = RamStorage::new(BLOCK, 176);
        for (block, chunk) in tail.chunks(BLOCK as usize).enumerate() {
            new_region.write(block as u32, 0, chunk).unwrap();
        }
        assert!(!lpfs_mounts_read_only(new_region, config(176)));
        assert!(lpfs_mounts_read_only(legacy, config(240)));
    }

    #[test]
    fn a_held_verdict_never_formats() {
        let storage = RamStorage::new(BLOCK, 176);
        fn cfg() -> Config {
            config(176)
        }
        let outcome = LpFsFlash::init_guarded(storage, cfg, |_| FormatVerdict::Hold).unwrap();
        assert!(matches!(outcome, FlashFsInit::Held));
    }

    #[test]
    fn a_format_verdict_formats_and_an_existing_filesystem_just_mounts() {
        fn cfg() -> Config {
            config(240)
        }
        let blank = RamStorage::new(BLOCK, 240);
        let outcome = LpFsFlash::init_guarded(blank, cfg, |_| FormatVerdict::Format).unwrap();
        assert!(matches!(outcome, FlashFsInit::Formatted(_)));

        let mut asked = false;
        let outcome = LpFsFlash::init_guarded(legacy_image(), cfg, |_| {
            asked = true;
            FormatVerdict::Hold
        })
        .unwrap();
        assert!(matches!(outcome, FlashFsInit::Mounted(_)));
        assert!(!asked, "a filesystem that mounts never asks for a verdict");
    }
}
