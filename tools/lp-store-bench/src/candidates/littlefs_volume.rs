//! What F1 and F2 share: `littlefs-rust` (the crate the firmware links) on
//! [`NorFlashSim`], with the firmware's geometry
//! (`lp-fw/fw-esp32c6/src/flash_storage.rs`: block 4096, cache 512, lookahead
//! 64, read/prog 16, no wear-levelling cycles; block count from the config),
//! plus the file-level helpers both adapters use.
//!
//! littlefs's `Filesystem` owns its storage and has no accessor, so the flash
//! lives in an `Rc` shared between the [`Storage`] impl and the adapter: the
//! adapter clones it for `flash_snapshot` and takes it back after `unmount`.
//! The storage also remembers the last flash error, because littlefs folds
//! every block-device error into `LFS_ERR_IO`: a cut must come back as
//! `PowerLost`, the read watchdog as itself.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use littlefs_rust::{Config, Error as LfsError, FileType, Filesystem, OpenFlags, Storage};
use lp_nor_sim::{NorError, NorFlashSim};

use crate::{CandidateConfig, StoreError};

/// The firmware's littlefs block (= one NOR sector).
pub const BLOCK_SIZE: u32 = 4096;
/// The firmware's read/prog cache (and so each open file's cache).
pub const CACHE_SIZE: u32 = 512;
/// The firmware's allocator lookahead (bits for 512 blocks).
pub const LOOKAHEAD_SIZE: u32 = 64;

/// The flash, shared between littlefs's storage and the adapter.
pub struct SharedFlash {
    flash: RefCell<NorFlashSim>,
    last_error: Cell<Option<NorError>>,
}

impl SharedFlash {
    pub fn new(flash: NorFlashSim) -> Rc<Self> {
        Rc::new(Self {
            flash: RefCell::new(flash),
            last_error: Cell::new(None),
        })
    }

    pub fn snapshot(&self) -> NorFlashSim {
        self.flash.borrow().clone()
    }

    /// The flash back out of a shared handle (cheap clone when another `Rc`
    /// is still alive, e.g. a dropped-but-not-yet-freed filesystem).
    pub fn take(this: Rc<Self>) -> NorFlashSim {
        match Rc::try_unwrap(this) {
            Ok(s) => s.flash.into_inner(),
            Err(rc) => rc.snapshot(),
        }
    }

    /// A littlefs error as the harness sees it: what the flash said last wins
    /// over littlefs's own reading of it.
    pub fn store_error(&self, e: LfsError) -> StoreError {
        if !self.flash.borrow().is_powered() {
            return StoreError::PowerLost;
        }
        match (e, self.last_error.get()) {
            (_, Some(NorError::Watchdog)) => StoreError::Other("flash read watchdog".into()),
            (LfsError::NoSpace, _) => StoreError::NoSpace,
            (LfsError::Corrupt, _) => StoreError::Corrupt("littlefs: corrupt".into()),
            (e, _) => StoreError::Other(format!("littlefs: {e:?}")),
        }
    }

    fn note(&self, r: Result<(), NorError>) -> Result<(), LfsError> {
        r.map_err(|e| {
            self.last_error.set(Some(e));
            LfsError::Io
        })
    }
}

/// littlefs's block device: block/offset onto the sim's flat addresses.
pub struct LfsNorStorage(pub Rc<SharedFlash>);

impl Storage for LfsNorStorage {
    fn read(&mut self, block: u32, offset: u32, buf: &mut [u8]) -> Result<(), LfsError> {
        let r = self
            .0
            .flash
            .borrow_mut()
            .read(block * BLOCK_SIZE + offset, buf);
        self.0.note(r)
    }

    fn write(&mut self, block: u32, offset: u32, data: &[u8]) -> Result<(), LfsError> {
        let r = self
            .0
            .flash
            .borrow_mut()
            .program(block * BLOCK_SIZE + offset, data);
        self.0.note(r)
    }

    fn erase(&mut self, block: u32) -> Result<(), LfsError> {
        let r = self.0.flash.borrow_mut().erase_sector(block);
        self.0.note(r)
    }
}

/// An open littlefs file on the sim.
pub type LfsFile<'a> = littlefs_rust::File<'a, LfsNorStorage>;

/// The firmware's littlefs configuration for `cfg.sectors` blocks. The
/// `block_cycles` dial sets littlefs's metadata-pair wear levelling (erases a
/// pair takes before littlefs relocates it); unset is −1, off, which is
/// `Config::new`'s default and what the firmware runs with today.
pub fn lfs_config(cfg: &CandidateConfig) -> Config {
    let mut c = Config::new(BLOCK_SIZE, cfg.sectors);
    c.cache_size = CACHE_SIZE;
    c.lookahead_size = LOOKAHEAD_SIZE;
    c.block_cycles = cfg
        .dial("block_cycles")
        .and_then(|v| v.parse().ok())
        .filter(|&v: &i32| v != 0)
        .unwrap_or(-1);
    c
}

/// RAM a mounted littlefs holds with `open_files` files open: the `lfs_t`,
/// the read and prog caches, the lookahead, and per open file its `lfs_file_t`
/// plus its own cache. `size_of` is the host's (64-bit pointers); on RV32 the
/// structs are smaller, so this errs high.
pub fn lfs_ram_bytes(open_files: u64) -> u64 {
    let lfs = std::mem::size_of::<littlefs_rust_core::Lfs>() as u64
        + std::mem::size_of::<littlefs_rust_core::LfsConfig>() as u64;
    let caches = 2 * CACHE_SIZE as u64 + LOOKAHEAD_SIZE as u64;
    let file = std::mem::size_of::<littlefs_rust_core::LfsFile>() as u64
        + std::mem::size_of::<littlefs_rust_core::LfsFileConfig>() as u64
        + CACHE_SIZE as u64;
    lfs + caches + open_files * file
}

/// A mounted littlefs on the shared flash, with path-level helpers.
pub struct LfsVolume {
    fs: Option<Filesystem<LfsNorStorage>>,
    flash: Rc<SharedFlash>,
}

impl LfsVolume {
    /// `lfs_format` onto `flash` (littlefs erases what it needs).
    pub fn format(flash: &mut NorFlashSim, cfg: &CandidateConfig) -> Result<(), StoreError> {
        let shared = SharedFlash::new(flash.clone());
        let mut storage = LfsNorStorage(shared.clone());
        let r = Filesystem::format(&mut storage, &lfs_config(cfg));
        let e = r.err().map(|e| shared.store_error(e));
        drop(storage);
        *flash = SharedFlash::take(shared);
        e.map_or(Ok(()), Err)
    }

    /// `lfs_mount` (read-only: littlefs repairs orphans on its first write).
    pub fn mount(
        flash: NorFlashSim,
        cfg: &CandidateConfig,
    ) -> Result<Self, (StoreError, NorFlashSim)> {
        let shared = SharedFlash::new(flash);
        match Filesystem::mount(LfsNorStorage(shared.clone()), lfs_config(cfg)) {
            Ok(fs) => Ok(Self {
                fs: Some(fs),
                flash: shared,
            }),
            Err((e, storage)) => {
                let err = shared.store_error(e);
                drop(storage);
                Err((err, SharedFlash::take(shared)))
            }
        }
    }

    fn fs(&self) -> &Filesystem<LfsNorStorage> {
        self.fs.as_ref().expect("volume is mounted")
    }

    pub fn err(&self, e: LfsError) -> StoreError {
        self.flash.store_error(e)
    }

    pub fn snapshot(&self) -> NorFlashSim {
        self.flash.snapshot()
    }

    /// Unmount (littlefs writes nothing on unmount) and take the flash.
    pub fn into_flash(mut self) -> NorFlashSim {
        if let Some(fs) = self.fs.take() {
            // An unmount error changes nothing on flash; the flash is ours
            // either way through the shared handle.
            let _ = fs.unmount();
        }
        SharedFlash::take(self.flash)
    }

    /// Blocks littlefs counts as allocated (`lfs_fs_size`).
    pub fn used_blocks(&self) -> Option<u32> {
        self.fs().fs_size().ok()
    }

    /// Create every missing directory above `path` (as the firmware's
    /// `ensure_parent_dirs` does).
    pub fn ensure_parent_dirs(&self, path: &str) -> Result<(), StoreError> {
        let Some((parent, _)) = path.rsplit_once('/') else {
            return Ok(());
        };
        let mut acc = String::new();
        for seg in parent.split('/').filter(|s| !s.is_empty()) {
            acc.push('/');
            acc.push_str(seg);
            match self.fs().mkdir(&acc) {
                Ok(()) | Err(LfsError::Exists) => {}
                Err(e) => return Err(self.err(e)),
            }
        }
        Ok(())
    }

    /// Write a whole file (create or truncate), parents made as needed. Unlike
    /// `Filesystem::write_file`, the close (where littlefs commits) is checked.
    pub fn write_file(&self, path: &str, bytes: &[u8]) -> Result<(), StoreError> {
        self.ensure_parent_dirs(path)?;
        let f = self
            .fs()
            .open(
                path,
                OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::TRUNC,
            )
            .map_err(|e| self.err(e))?;
        let mut off = 0;
        while off < bytes.len() {
            off += f.write(&bytes[off..]).map_err(|e| self.err(e))? as usize;
        }
        f.close().map_err(|e| self.err(e))
    }

    /// Open a file (for F2's streaming reads and writes).
    pub fn open(&self, path: &str, flags: OpenFlags) -> Result<LfsFile<'_>, LfsError> {
        self.fs().open(path, flags)
    }

    /// Does a file (or directory) exist at `path`?
    pub fn exists(&self, path: &str) -> Result<bool, StoreError> {
        match self.fs().stat(path) {
            Ok(_) => Ok(true),
            Err(LfsError::NoEntry | LfsError::NotDir) => Ok(false),
            Err(e) => Err(self.err(e)),
        }
    }

    /// One directory's entries, littlefs's error as is.
    pub fn list_dir(&self, path: &str) -> Result<Vec<littlefs_rust::DirEntry>, LfsError> {
        self.fs().list_dir(path)
    }

    /// A whole file, or `None` when it does not exist.
    pub fn read_file(&self, path: &str) -> Result<Option<Vec<u8>>, StoreError> {
        match self.fs().read_to_vec(path) {
            Ok(b) => Ok(Some(b)),
            Err(LfsError::NoEntry | LfsError::IsDir | LfsError::NotDir) => Ok(None),
            Err(e) => Err(self.err(e)),
        }
    }

    /// Remove a file or empty directory; a missing one is fine.
    pub fn remove(&self, path: &str) -> Result<(), StoreError> {
        match self.fs().remove(path) {
            Ok(()) | Err(LfsError::NoEntry) => Ok(()),
            Err(e) => Err(self.err(e)),
        }
    }

    pub fn rename(&self, from: &str, to: &str) -> Result<(), StoreError> {
        self.fs().rename(from, to).map_err(|e| self.err(e))
    }

    /// Every file whose path starts with `prefix`, sorted (a recursive walk
    /// that only descends into directories the prefix can reach).
    pub fn files_under(&self, prefix: &str) -> Result<Vec<String>, StoreError> {
        let mut out = Vec::new();
        self.walk("", prefix, &mut out)?;
        out.sort();
        Ok(out)
    }

    fn walk(&self, dir: &str, prefix: &str, out: &mut Vec<String>) -> Result<(), StoreError> {
        let listing = match self.fs().list_dir(if dir.is_empty() { "/" } else { dir }) {
            Ok(l) => l,
            Err(LfsError::NoEntry | LfsError::NotDir) => return Ok(()),
            Err(e) => return Err(self.err(e)),
        };
        for e in listing {
            let child = format!("{dir}/{}", e.name);
            match e.file_type {
                FileType::File => {
                    if child.starts_with(prefix) {
                        out.push(child);
                    }
                }
                FileType::Dir => {
                    let as_dir = format!("{child}/");
                    if as_dir.starts_with(prefix) || prefix.starts_with(&as_dir) {
                        self.walk(&child, prefix, out)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Remove every file whose path starts with `prefix` (except those `keep`
    /// names), then every directory the prefix covers that is left empty,
    /// deepest first (the firmware's `delete_dir_recursive`, generalised to a
    /// prefix).
    pub fn delete_prefix(
        &self,
        prefix: &str,
        keep: &dyn Fn(&str) -> bool,
    ) -> Result<(), StoreError> {
        self.delete_in("", prefix, keep).map(|_| ())
    }

    /// Returns whether anything was kept under `dir`.
    fn delete_in(
        &self,
        dir: &str,
        prefix: &str,
        keep: &dyn Fn(&str) -> bool,
    ) -> Result<bool, StoreError> {
        let listing = match self.fs().list_dir(if dir.is_empty() { "/" } else { dir }) {
            Ok(l) => l,
            Err(LfsError::NoEntry | LfsError::NotDir) => return Ok(false),
            Err(e) => return Err(self.err(e)),
        };
        let mut kept = false;
        for e in listing {
            let child = format!("{dir}/{}", e.name);
            match e.file_type {
                FileType::File => {
                    if child.starts_with(prefix) {
                        if keep(&child) {
                            kept = true;
                        } else {
                            self.remove(&child)?;
                        }
                    }
                }
                FileType::Dir => {
                    let as_dir = format!("{child}/");
                    let covered = as_dir.starts_with(prefix);
                    let mut kept_below = false;
                    if covered || prefix.starts_with(&as_dir) {
                        kept_below = self.delete_in(&child, prefix, keep)?;
                    }
                    if covered && !kept_below {
                        self.remove(&child)?;
                    }
                    kept |= kept_below;
                }
            }
        }
        Ok(kept)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_nor_sim::{FaultPlan, TearModel};

    fn mount(f: NorFlashSim) -> LfsVolume {
        LfsVolume::mount(f, &CandidateConfig::new(128))
            .map_err(|(e, _)| e)
            .unwrap()
    }

    /// A library defect, pinned (found by F2's random walk, 2026-10-08): a
    /// clean cut late in a rename *across directories* makes the entry that
    /// sorts right after the moved one in the source directory vanish — here
    /// the whole `/.lp` directory. A same-directory rename does not do it, so
    /// F2 keeps its temporaries beside their targets. If this starts failing,
    /// littlefs-rust was fixed: drop the pin and re-run the F2 sweeps.
    #[test]
    fn littlefs_cross_dir_rename_cut_loses_the_next_entry() {
        let cfg = CandidateConfig::new(128);
        let mut f = NorFlashSim::new(cfg.geometry());
        LfsVolume::format(&mut f, &cfg).unwrap();
        let v = mount(f);
        v.write_file("/.lp/access.json", b"{}").unwrap();
        v.write_file("/hardware.json", b"{}").unwrap();
        v.ensure_parent_dirs("/projects/b/.lp/panel.json").unwrap();
        v.write_file("/.f2-new.tmp", &[b'q'; 346]).unwrap();
        let mut pre = v.into_flash();
        pre.power_cycle(FaultPlan::none());
        let rename = |v: &LfsVolume| v.rename("/.f2-new.tmp", "/projects/b/.lp/panel.json");
        let v = mount(pre.clone());
        rename(&v).unwrap();
        let n = v.snapshot().ops_since_plan();
        drop(v);
        let lost: Vec<u64> = (0..=n)
            .filter(|&k| {
                let mut f = pre.clone();
                f.power_cycle(FaultPlan {
                    cut_after: Some(k),
                    tear: TearModel::Clean,
                    seed: 1,
                });
                let v = mount(f);
                let _ = rename(&v);
                let mut f = v.into_flash();
                f.power_cycle(FaultPlan::none());
                let files = mount(f).files_under("/").unwrap();
                !files.contains(&"/.lp/access.json".to_string())
            })
            .collect();
        assert!(!lost.is_empty(), "littlefs-rust no longer loses an entry");
    }

    /// A library defect, pinned (found by the 30-day endurance run,
    /// 2026-10-08): littlefs-rust 0.1.0's `lfs_dir_commitcrc` never writes the
    /// FCRC tag that upstream's (disk version 2.1) writes after every commit,
    /// so a fetched metadata pair never proves its tail erased and every
    /// commit compacts — one sector erase per file write instead of one per
    /// block-full of commits. Upstream would erase ~20 times here. If this
    /// starts failing, littlefs-rust was fixed: drop the pin and re-run the
    /// endurance numbers.
    #[test]
    fn littlefs_compacts_on_every_commit() {
        let cfg = CandidateConfig::new(128);
        let mut f = NorFlashSim::new(cfg.geometry());
        LfsVolume::format(&mut f, &cfg).unwrap();
        let v = mount(f);
        let panel = [b'p'; 350];
        v.write_file("/projects/a/.lp/panel.json", &panel).unwrap();
        let before = v.snapshot().stats().erases_total();
        for _ in 0..200 {
            v.write_file("/projects/a/.lp/panel.json", &panel).unwrap();
        }
        let erases = v.snapshot().stats().erases_total() - before;
        assert!(
            erases >= 190,
            "littlefs-rust no longer compacts every commit ({erases} erases)"
        );
    }
}
