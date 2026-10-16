//! Flash-backed LpFs implementation using littlefs-rust.
//!
//! Wraps `littlefs_rust::Filesystem<S>` over a chip-supplied
//! `littlefs_rust::Storage` implementation and implements the `LpFs` trait
//! for use with LpServer. The chip crate owns the flash adapter and the
//! partition-derived littlefs `Config`.
//!
//! **What a power cut leaves.** An overwrite is one littlefs metadata commit
//! (at close), so a cut leaves the old bytes or the new. A create is two (the
//! empty entry at open, the data at close), so [`LpFs::write_file`] creates a
//! file under a hidden name in the same directory and renames it into place
//! — one commit — and a cut leaves no file or the whole one
//! (`docs/defects/2026-10-10-a-cut-during-a-littlefs-file-create-leaves-it-empty.md`).
//! Only ever a rename within one directory: littlefs-rust loses an unrelated
//! entry when a rename across directories is cut
//! (`docs/defects/2026-10-08-littlefs-rust-cross-directory-rename-cut-loses-the-next-entry.md`).

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
    /// The partition holds a filesystem this build does not read (on the C6,
    /// the tree store: [`holds_tree_store`]): do NOT format, and refuse it.
    Refuse,
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
    /// None mounted, and the verdict said [`FormatVerdict::Refuse`]: nothing
    /// was written. The caller serves a RAM filesystem with access locked
    /// (`FsBootState::Refused`).
    Refused,
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
            // The verdict above never holds or refuses.
            FlashFsInit::Held | FlashFsInit::Refused => Err(LfsError::Io),
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

        match may_format(&mut storage) {
            FormatVerdict::Hold => {
                log::warn!("[FS] Mount failed ({mount_error}), holding: not formatting");
                return Ok(FlashFsInit::Held);
            }
            FormatVerdict::Refuse => {
                log::warn!("[FS] Mount failed ({mount_error}), refusing: not formatting");
                return Ok(FlashFsInit::Refused);
            }
            FormatVerdict::Format => {}
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

/// The suffix of the hidden name a new file is written under before it takes
/// its own (`clock.json#lp-new`). A cut can leave one behind; it is never
/// listed, a later create of the same file overwrites it, and removing the
/// directory removes it. Not a format: an older firmware just sees one more
/// file. `#` sorts below letters, digits, `.`, `-` and `_`, so nothing falls
/// between a name and its temporary and the two share a metadata pair (a
/// rename between pairs takes littlefs's move path).
pub const CREATE_TEMP_SUFFIX: &str = "#lp-new";

fn is_create_temp(name: &str) -> bool {
    name.ends_with(CREATE_TEMP_SUFFIX)
}

/// A listing never shows a create's temporary (see [`CREATE_TEMP_SUFFIX`]).
fn is_hidden(entry: &littlefs_rust::DirEntry) -> bool {
    entry.file_type == LfsFileType::File && is_create_temp(&entry.name)
}

/// Open `path` created or truncated, write all of `data`, and close it,
/// reporting the close: the commit happens there, and littlefs-rust's own
/// `write_file` drops the file and with it any error the close returns.
fn write_whole<S: Storage>(fs: &Filesystem<S>, path: &str, data: &[u8]) -> Result<(), LfsError> {
    let file = fs.open(
        path,
        OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::TRUNC,
    )?;
    let mut offset = 0;
    while offset < data.len() {
        match file.write(&data[offset..])? {
            0 => return Err(LfsError::Io),
            n => offset += n as usize,
        }
    }
    file.close()
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
        if is_create_temp(lfs_path) {
            return Err(FsError::InvalidPath(format!(
                "{}: names ending in `{CREATE_TEMP_SUFFIX}` are reserved",
                path.as_str()
            )));
        }
        self.ensure_parent_dirs(lfs_path)?;
        let existed = {
            let inner = self.inner.borrow();
            inner.fs.exists(lfs_path)
        };
        let write_error =
            |e: LfsError| FsError::Filesystem(format!("write {}: {e}", path.as_str()));
        {
            let inner = self.inner.borrow_mut();
            if existed {
                // An overwrite is one metadata commit at close: a cut leaves
                // the old bytes or the new.
                write_whole(&inner.fs, lfs_path, data).map_err(write_error)?;
            } else {
                // A create is two commits (the empty entry at open, the data
                // at close), so a cut between them would leave the file on
                // flash, empty. Write it whole under a hidden name beside it,
                // then give it its name: a same-directory rename is one
                // commit, so a cut leaves no file or the whole one (this
                // module's docs).
                let temp = format!("{lfs_path}{CREATE_TEMP_SUFFIX}");
                write_whole(&inner.fs, &temp, data).map_err(write_error)?;
                if let Err(e) = inner.fs.rename(&temp, lfs_path) {
                    let _ = inner.fs.remove(&temp);
                    return Err(write_error(e));
                }
            }
        }
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
                    if is_hidden(&item) {
                        continue;
                    }
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
                if is_hidden(&item) {
                    continue;
                }
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

/// The tree store's sector magic, `"LTS1"`: bytes 0..4 of every sector
/// header it writes, in every format version, past and future
/// (`lp-base/lp-tree-store/FORMAT.md`, "Sector"). Spelled here, not
/// imported, so a littlefs image recognises a tree store without linking
/// one; a test pins it to `lp_tree_store::SECTOR_MAGIC`.
pub const TREE_STORE_SECTOR_MAGIC: [u8; 4] = *b"LTS1";

/// Does `storage`, a partition of `block_count` 4 KB blocks, hold a tree
/// store — does any block begin with [`TREE_STORE_SECTOR_MAGIC`]?
///
/// The littlefs build's half of the tree-store guard (ADR
/// `2026-10-10-fs-tree-refused-state-and-update-guard`, amended 2026-10-16):
/// asked only after littlefs would not mount, before anything formats. Every
/// block is looked at, because the store levels its wear: its headers can be
/// anywhere, and its first two sectors may well be free. Only the magic is
/// read, never a CRC or a version — a newer store may have moved them, and
/// it must be held all the same. A block that will not read counts as one:
/// never format what could not be looked at. Read-only; four bytes a block.
pub fn holds_tree_store<S: Storage>(storage: &mut S, block_count: u32) -> bool {
    let mut head = [0u8; 4];
    (0..block_count).any(|block| match storage.read(block, 0, &mut head) {
        Ok(()) => head == TREE_STORE_SECTOR_MAGIC,
        Err(_) => true,
    })
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

    /// The magic is the store's own: no copy of a tree-store crate is linked
    /// into a littlefs image, so this test is what keeps the two equal.
    #[test]
    fn the_tree_store_magic_is_the_stores() {
        assert_eq!(
            TREE_STORE_SECTOR_MAGIC,
            lp_tree_store::SECTOR_MAGIC.to_le_bytes()
        );
    }

    /// A real store, built by `lp-tree-store`, is recognised wherever its
    /// headers are — at the front, and moved to the back with its first
    /// sectors free (its wear levelling puts them anywhere). Blank flash and
    /// a littlefs filesystem with files are not stores.
    #[test]
    fn a_tree_store_is_recognised_wherever_its_headers_are() {
        let store = tree_store_image(176);
        let mut front = ram_of(&store);
        assert!(holds_tree_store(&mut front, 176));
        let mut back = ram_of(&rotated(&store, 120));
        assert!(
            !back.data()[..2 * BLOCK as usize]
                .windows(4)
                .any(|w| w == TREE_STORE_SECTOR_MAGIC),
            "the premise: the rotated store's first two sectors hold no header"
        );
        assert!(holds_tree_store(&mut back, 176));

        assert!(!holds_tree_store(&mut RamStorage::new(BLOCK, 176), 176));
        assert!(!holds_tree_store(&mut legacy_image(), 240));
    }

    /// A block that will not read is never taken for blank.
    #[test]
    fn a_block_that_will_not_read_counts_as_a_store() {
        struct Unreadable;
        impl Storage for Unreadable {
            fn read(&mut self, block: u32, _: u32, buf: &mut [u8]) -> Result<(), LfsError> {
                if block == 7 {
                    return Err(LfsError::Io);
                }
                buf.fill(0xFF);
                Ok(())
            }
            fn write(&mut self, _: u32, _: u32, _: &[u8]) -> Result<(), LfsError> {
                Err(LfsError::Io)
            }
            fn erase(&mut self, _: u32) -> Result<(), LfsError> {
                Err(LfsError::Io)
            }
        }
        assert!(holds_tree_store(&mut Unreadable, 176));
        assert!(!holds_tree_store(&mut Unreadable, 7));
    }

    /// The C6's verdict, end to end: a littlefs build over a tree store does
    /// not mount it, refuses it, and writes not one byte.
    #[test]
    fn a_littlefs_build_refuses_a_tree_store_and_writes_nothing() {
        fn cfg() -> Config {
            config(176)
        }
        let store = rotated(&tree_store_image(176), 37);
        let mut flash = ram_of(&store);
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
        let outcome = LpFsFlash::init_guarded(View(&mut flash), cfg, |s| {
            if holds_tree_store(s, 176) {
                FormatVerdict::Refuse
            } else {
                FormatVerdict::Format
            }
        })
        .unwrap();
        assert!(matches!(outcome, FlashFsInit::Refused));
        drop(outcome);
        assert!(flash.data() == &store[..], "the refusal wrote to flash");
    }

    /// The defect's mechanism, cut at every flash operation of the create:
    /// the file is absent or whole, never on flash empty or short, its
    /// sibling is kept, and no listing shows the temporary. Both an inline
    /// file and one past littlefs's 512 B inline limit.
    #[test]
    fn a_cut_anywhere_in_a_create_leaves_no_file_or_the_whole_one() {
        let base = base_image();
        for data in [b"{\"f32\":1.5}".to_vec(), vec![0x5Au8; 5_000]] {
            let path = "/projects/demo/.lp/panel.json".as_path();
            let write = |fs: &LpFsFlash<CutStorage>| {
                let _ = fs.write_file(path, &data);
            };
            let ops = ops_of(&base, write);
            assert!(ops > 2, "a create takes more than two flash operations");
            for cut in 0..ops {
                let fs = mount(cut_at(&base, cut, write));
                match fs.read_file(path) {
                    Err(FsError::NotFound(_)) => {}
                    Ok(bytes) => assert_eq!(bytes, data, "cut at op {cut} of {ops}"),
                    Err(e) => panic!("cut at op {cut} of {ops}: {e}"),
                }
                assert_eq!(
                    fs.read_file("/projects/demo/project.json".as_path())
                        .unwrap(),
                    b"{\"name\":\"demo\"}"
                );
                let listed = fs.list_dir("/projects".as_path(), true).unwrap();
                assert!(
                    listed.iter().all(|p| !is_create_temp(p.as_str())),
                    "cut at op {cut}: a listing shows the temporary: {listed:?}"
                );
            }
        }
    }

    /// An overwrite was already one commit; it stays one: old or new.
    #[test]
    fn a_cut_anywhere_in_an_overwrite_leaves_the_old_bytes_or_the_new() {
        let base = base_image();
        let path = "/projects/demo/project.json".as_path();
        let new = b"{\"name\":\"demo, renamed\"}".to_vec();
        let write = |fs: &LpFsFlash<CutStorage>| {
            let _ = fs.write_file(path, &new);
        };
        let ops = ops_of(&base, write);
        for cut in 0..ops {
            let bytes = mount(cut_at(&base, cut, write)).read_file(path).unwrap();
            assert!(
                bytes == new || bytes == b"{\"name\":\"demo\"}",
                "cut at op {cut} of {ops}: {bytes:?}"
            );
        }
    }

    /// A temporary a cut left behind is never listed, and the next create of
    /// that file takes it over.
    #[test]
    fn a_left_temporary_is_hidden_and_the_next_create_takes_it_over() {
        let fs = mount(base_image());
        {
            let inner = fs.inner.borrow();
            inner
                .fs
                .write_file("projects/demo/clock.json#lp-new", b"")
                .unwrap();
        }
        assert_eq!(
            fs.list_dir("/projects/demo".as_path(), false).unwrap(),
            vec![LpPathBuf::from("/projects/demo/project.json")]
        );
        fs.write_file("/projects/demo/clock.json".as_path(), b"{}")
            .unwrap();
        assert_eq!(
            fs.read_file("/projects/demo/clock.json".as_path()).unwrap(),
            b"{}"
        );
        let inner = fs.inner.borrow();
        let names: Vec<_> = inner
            .fs
            .list_dir("projects/demo")
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert_eq!(names, ["clock.json", "project.json"]);
    }

    #[test]
    fn a_reserved_name_is_refused() {
        let fs = mount(base_image());
        assert!(matches!(
            fs.write_file("/projects/demo/x#lp-new".as_path(), b"{}"),
            Err(FsError::InvalidPath(_))
        ));
    }

    // ---- helpers ----

    use core::cell::Cell;

    /// A `sectors`-sector tree store holding a few files, as `lp-tree-store`
    /// writes it.
    fn tree_store_image(sectors: u32) -> Vec<u8> {
        use lp_nor_sim::{NorFlashSim, NorGeometry};
        use lp_tree_store::{SoftSha256, StoreConfig, TreeStore};
        let mut st = match TreeStore::format(
            NorFlashSim::new(NorGeometry::c6(sectors)),
            SoftSha256,
            StoreConfig::default(),
        ) {
            Ok(st) => st,
            Err((e, ..)) => panic!("format: {e:?}"),
        };
        st.put("/hardware.json", b"{\"board\":\"kept\"}").unwrap();
        st.put("/projects/p/project.json", b"{\"name\":\"p\"}")
            .unwrap();
        st.put("/projects/p/.lp/panel.json", b"{}").unwrap();
        let mut out = vec![0u8; (sectors * BLOCK) as usize];
        st.flash().peek(0, &mut out);
        out
    }

    /// `image` with its sectors moved `by` places towards the end (wrapping).
    fn rotated(image: &[u8], by: usize) -> Vec<u8> {
        let mut out = image.to_vec();
        out.rotate_right(by * BLOCK as usize);
        out
    }

    fn ram_of(image: &[u8]) -> RamStorage {
        let mut ram = RamStorage::new(BLOCK, image.len() as u32 / BLOCK);
        for (block, chunk) in image.chunks(BLOCK as usize).enumerate() {
            ram.write(block as u32, 0, chunk).unwrap();
        }
        ram
    }
    use lpfs::lp_path::AsLpPath;

    const CUT_BLOCKS: u32 = 64;

    fn cut_config() -> Config {
        config(CUT_BLOCKS)
    }

    /// RAM flash that counts its programs and erases and fails the
    /// `cut_at`-th and every one after it without changing a byte: a clean
    /// power cut, the bytes left being the flash at the cut.
    struct CutStorage {
        bytes: Rc<RefCell<Vec<u8>>>,
        ops: Rc<Cell<u64>>,
        cut_at: u64,
    }

    impl CutStorage {
        fn cut(&self) -> bool {
            let n = self.ops.get();
            self.ops.set(n + 1);
            n >= self.cut_at
        }
    }

    impl Storage for CutStorage {
        fn read(&mut self, block: u32, offset: u32, buf: &mut [u8]) -> Result<(), LfsError> {
            let at = (block * BLOCK + offset) as usize;
            buf.copy_from_slice(&self.bytes.borrow()[at..at + buf.len()]);
            Ok(())
        }
        fn write(&mut self, block: u32, offset: u32, data: &[u8]) -> Result<(), LfsError> {
            if self.cut() {
                return Err(LfsError::Io);
            }
            let at = (block * BLOCK + offset) as usize;
            self.bytes.borrow_mut()[at..at + data.len()].copy_from_slice(data);
            Ok(())
        }
        fn erase(&mut self, block: u32) -> Result<(), LfsError> {
            if self.cut() {
                return Err(LfsError::Io);
            }
            let at = (block * BLOCK) as usize;
            self.bytes.borrow_mut()[at..at + BLOCK as usize].fill(0xFF);
            Ok(())
        }
    }

    /// `bytes` as flash that is cut at its `cut_at`-th program or erase.
    fn storage(bytes: Vec<u8>, cut_at: u64) -> (CutStorage, Rc<RefCell<Vec<u8>>>, Rc<Cell<u64>>) {
        let bytes = Rc::new(RefCell::new(bytes));
        let ops = Rc::new(Cell::new(0));
        let storage = CutStorage {
            bytes: bytes.clone(),
            ops: ops.clone(),
            cut_at,
        };
        (storage, bytes, ops)
    }

    /// Mount `bytes`, which must hold a filesystem (never formats).
    fn mount(bytes: Vec<u8>) -> LpFsFlash<CutStorage> {
        let (storage, _, _) = storage(bytes, u64::MAX);
        match LpFsFlash::init_guarded(storage, cut_config, |_| FormatVerdict::Hold).unwrap() {
            FlashFsInit::Mounted(fs) => fs,
            _ => panic!("the image does not mount"),
        }
    }

    /// A filesystem holding `/projects/demo/project.json`.
    fn base_image() -> Vec<u8> {
        let (storage, bytes, _) = storage(vec![0xFF; (CUT_BLOCKS * BLOCK) as usize], u64::MAX);
        let (fs, formatted) = LpFsFlash::init(storage, cut_config).unwrap();
        assert!(formatted);
        fs.write_file(
            "/projects/demo/project.json".as_path(),
            b"{\"name\":\"demo\"}",
        )
        .unwrap();
        drop(fs);
        bytes.take()
    }

    /// How many programs and erases `step` takes on `base`.
    fn ops_of(base: &[u8], step: impl Fn(&LpFsFlash<CutStorage>)) -> u64 {
        let (storage, _, ops) = storage(base.to_vec(), u64::MAX);
        let fs = LpFsFlash::from_fs(
            Filesystem::mount(storage, cut_config())
                .map_err(|(e, _)| e)
                .unwrap(),
        );
        let before = ops.get();
        step(&fs);
        ops.get() - before
    }

    /// The flash after `step` on `base` is cut at its `cut`-th operation.
    fn cut_at(base: &[u8], cut: u64, step: impl Fn(&LpFsFlash<CutStorage>)) -> Vec<u8> {
        let (storage, bytes, _) = storage(base.to_vec(), cut);
        let fs = LpFsFlash::from_fs(
            Filesystem::mount(storage, cut_config())
                .map_err(|(e, _)| e)
                .unwrap(),
        );
        step(&fs);
        drop(fs);
        bytes.take()
    }
}
