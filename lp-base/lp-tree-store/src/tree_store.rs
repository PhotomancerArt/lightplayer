//! `TreeStore`: the path API over the record log.
//!
//! **Per-call commits.** Every `put`, `append`, `put_chunk_deflated`,
//! `delete` and `delete_prefix` outside a transaction writes its records,
//! path-copies its directories and writes a root before it returns.
//!
//! **Transactions.** Inside `begin` … `commit`, content records go to flash
//! as they arrive (*pending*: reachable from the working tree, so GC keeps
//! them), directory changes are held as a small delta in RAM (written as
//! pending directories when it outgrows `txn_delta_max`), and only `commit`
//! writes the root. A cut before that root leaves the pre-transaction state;
//! `abort` drops the delta and puts the path table back.
//!
//! A per-call write is the same machinery as a one-call transaction.

use alloc::string::String;
use alloc::vec::Vec;

use crate::flash::Flash;
use crate::node_read::read_node_into;
use crate::object_hasher::ObjectHasher;
use crate::object_id::{IdTag, ObjectId, path_hash};
use crate::path_table::{PathSlot, PathTable};
use crate::record_kind::{ChunkCodec, RecordKind};
use crate::record_log::RecordLog;
use crate::root_record::RootRecord;
use crate::sector_header::{HeadKind, SECTOR_HEADER_LEN};
use crate::store_config::StoreConfig;
use crate::store_error::StoreError;
use crate::store_stats::TreeStoreStats;
use crate::tree_delta::{FileEntry, TreeDelta};
use crate::txn_undo::TxnUndo;

pub(crate) type Res<T, F> = Result<T, StoreError<<F as Flash>::Error>>;

/// Deepest path (components) the store accepts and mount follows.
pub const MAX_DEPTH: usize = 32;

/// The committed root.
#[derive(Clone, Debug)]
pub(crate) struct Committed {
    pub id: ObjectId,
    pub root: RootRecord,
}

/// The tree as written so far (committed, plus a transaction's flushed
/// directories).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WorkDirs {
    pub cold: ObjectId,
    pub hot: ObjectId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Txn {
    None,
    /// One call, committed (or rolled back) before it returns.
    Implicit,
    /// `begin` … `commit`/`abort`.
    Explicit,
}

/// The tree store.
pub struct TreeStore<F: Flash, H: ObjectHasher> {
    pub(crate) log: RecordLog<F>,
    pub(crate) hasher: H,
    pub(crate) cfg: StoreConfig,
    pub(crate) committed: Option<Committed>,
    pub(crate) max_root_seq: u64,
    pub(crate) work: WorkDirs,
    pub(crate) delta: TreeDelta,
    pub(crate) table: PathTable,
    pub(crate) undo: TxnUndo,
    pub(crate) txn: Txn,
    pub(crate) stats: TreeStoreStats,
}

impl<F: Flash, H: ObjectHasher> TreeStore<F, H> {
    /// Kill and erase every sector (retiring any that will not erase), then
    /// commit an empty tree. Works on any flash content.
    pub fn format(flash: &mut F, hasher: &mut H, cfg: &StoreConfig) -> Res<(), F> {
        check_config(flash.sector_count(), flash.sector_size(), cfg)?;
        let mut st = TreeStore::<&mut F, &mut H>::empty(flash, hasher, cfg.clone());
        for s in 0..st.log.sector_count {
            st.log.kill_and_erase(s)?;
        }
        let empty = st.write_dir_bytes(HeadKind::Cold, &[0, 0])?;
        st.work = WorkDirs {
            cold: empty,
            hot: empty,
        };
        st.write_root()
    }

    /// Scan the flash and adopt the newest complete root (I1). Never panics
    /// on any content; a flash with no complete root is an error, which
    /// hands the flash and the hasher back.
    pub fn mount(
        flash: F,
        hasher: H,
        cfg: StoreConfig,
    ) -> Result<Self, (StoreError<F::Error>, F, H)> {
        if let Err(e) = check_config(flash.sector_count(), flash.sector_size(), &cfg) {
            return Err((e, flash, hasher));
        }
        let mut st = Self::empty(flash, hasher, cfg);
        match st.load() {
            Ok(()) => Ok(st),
            Err(e) => Err((e, st.log.flash, st.hasher)),
        }
    }

    // ---- reads ------------------------------------------------------------

    /// The file at `path` (read-your-writes inside a transaction).
    pub fn get(&mut self, path: &str) -> Res<Option<Vec<u8>>, F> {
        let Some(fe) = self.lookup(path)? else {
            return Ok(None);
        };
        let mut out = Vec::with_capacity(fe.size as usize);
        read_node_into(&mut self.log, fe.id, &mut out)?;
        if out.len() != fe.size as usize {
            return Err(StoreError::Corrupt("file size"));
        }
        Ok(Some(out))
    }

    /// The file's size, from the path table (no flash read unless its hash
    /// collided).
    pub fn file_size(&mut self, path: &str) -> Res<Option<u32>, F> {
        Ok(self.lookup(path)?.map(|fe| fe.size))
    }

    pub fn exists(&mut self, path: &str) -> Res<bool, F> {
        Ok(self.lookup(path)?.is_some())
    }

    /// Every file path starting with `prefix` (a plain string prefix),
    /// sorted. Walks the directories on flash.
    pub fn list(&mut self, prefix: &str) -> Res<Vec<String>, F> {
        self.list_files(prefix)
    }

    // ---- writes -----------------------------------------------------------

    /// Write `bytes` at `path`, replacing any file there.
    pub fn put(&mut self, path: &str, bytes: &[u8]) -> Res<(), F> {
        check_path(path)?;
        if bytes.len() > u32::MAX as usize {
            return Err(StoreError::TooLarge);
        }
        self.op(|st| {
            let existed = st.existing(path)?;
            st.put_inner(path, existed, bytes)
        })
    }

    /// Append `bytes` to the file at `path` (creating it): writes the new
    /// chunk records and the file's new multi spine — not the file again.
    pub fn append(&mut self, path: &str, bytes: &[u8]) -> Res<(), F> {
        check_path(path)?;
        self.op(|st| st.append_inner(path, bytes))
    }

    /// Write one host-deflated chunk at `offset` of `path`: `0` replaces
    /// the file with this chunk, the file's size appends it; anything else
    /// is `BadOffset`. The chunk is inflated (into a buffer of
    /// `logical_len` ≤ 4096 bytes) and hashed; if it does not inflate to
    /// exactly `logical_len`, or its id is not `expected` (when given), it
    /// is refused with `Corrupt` and nothing is written. The deflated bytes
    /// are stored as they came (or, when they do not fit a record or do not
    /// shrink, the logical bytes stored).
    pub fn put_chunk_deflated(
        &mut self,
        path: &str,
        offset: u32,
        logical_len: u32,
        expected: Option<ObjectId>,
        deflated: &[u8],
    ) -> Res<(), F> {
        check_path(path)?;
        self.op(|st| st.deflated_inner(path, offset, logical_len, expected, deflated))
    }

    /// Delete the file at `path`; `false` if there was none.
    pub fn delete(&mut self, path: &str) -> Res<bool, F> {
        check_path(path)?;
        self.op(|st| {
            let Some(fe) = st.existing(path)? else {
                return Ok(false);
            };
            st.record_delete(path, fe);
            Ok(true)
        })
    }

    /// Remove every path starting with `prefix` (a plain string prefix). A
    /// prefix `"<dir>/"` removes the directory as one change.
    pub fn delete_prefix(&mut self, prefix: &str) -> Res<(), F> {
        self.op(|st| st.delete_prefix_inner(prefix))
    }

    /// Delete the file at `path` and the directory at `path`, as one
    /// change (`LpFs::delete_dir`).
    pub fn delete_file_and_tree(&mut self, path: &str) -> Res<(), F> {
        check_path(path)?;
        self.op(|st| {
            if let Some(fe) = st.existing(path)? {
                st.record_delete(path, fe);
            }
            let mut prefix = String::from(path);
            prefix.push('/');
            st.delete_prefix_inner(&prefix)
        })
    }

    // ---- transactions -----------------------------------------------------

    /// Start a transaction: until `commit`, nothing is committed.
    pub fn begin(&mut self) -> Res<(), F> {
        if self.txn != Txn::None {
            return Err(StoreError::InTransaction);
        }
        self.txn = Txn::Explicit;
        Ok(())
    }

    /// Commit the transaction: write its directories and one root. Outside a
    /// transaction this does nothing (every call already committed). On
    /// `NoSpace` the transaction stays open (abort it).
    pub fn commit(&mut self) -> Res<(), F> {
        if self.txn != Txn::Explicit {
            return Ok(());
        }
        self.commit_inner()?;
        self.txn = Txn::None;
        Ok(())
    }

    /// Drop the transaction: back to the committed tree. Its records on
    /// flash are garbage.
    pub fn abort(&mut self) -> Res<(), F> {
        if self.txn == Txn::Explicit {
            self.abort_inner();
            self.txn = Txn::None;
        }
        Ok(())
    }

    pub fn in_transaction(&self) -> bool {
        self.txn == Txn::Explicit
    }

    // ---- the rest ---------------------------------------------------------

    pub fn flash(&self) -> &F {
        &self.log.flash
    }

    /// For a harness installing fault plans. Changing cells under the store
    /// is not supported.
    pub fn flash_mut(&mut self) -> &mut F {
        &mut self.log.flash
    }

    pub fn into_flash(self) -> F {
        self.log.into_flash()
    }

    pub fn into_parts(self) -> (F, H) {
        (self.log.flash, self.hasher)
    }

    pub fn config(&self) -> &StoreConfig {
        &self.cfg
    }

    /// Sectors free right now (by the live upper bound).
    pub fn free_sectors(&self) -> u32 {
        self.log.free_count()
    }

    pub fn stats(&self) -> TreeStoreStats {
        let mut s = self.stats.clone();
        s.index_entries = self.log.index.len();
        s.index_ram_bytes = self.log.index.ram_bytes();
        s.path_table_entries = self.table.len();
        s.path_table_ram_bytes = self.table.ram_bytes();
        s.sector_table_ram_bytes = self.log.sectors.ram_bytes();
        s.resident_ram_bytes = s.index_ram_bytes + s.path_table_ram_bytes + s.sector_table_ram_bytes;
        s.transient_peak_bytes = s.transient_peak_bytes.max(self.log.largest_buffer);
        s.records_written = self.log.counters.records_written;
        s.record_bytes_written = self.log.counters.record_bytes_written;
        s.sectors_opened = self.log.counters.sectors_opened;
        s.erases = self.log.counters.erases;
        s.verify_failures = self.log.counters.verify_failures;
        s.retired_sectors = self.log.sectors.retired.len();
        s
    }

    /// Forget the transient peak (a harness measuring one operation).
    pub fn reset_transient_peak(&mut self) {
        self.log.largest_buffer = 0;
        self.stats.transient_peak_bytes = 0;
    }

    // ---- internals --------------------------------------------------------

    pub(crate) fn empty(flash: F, hasher: H, cfg: StoreConfig) -> Self {
        Self {
            log: RecordLog::new(flash),
            hasher,
            cfg,
            committed: None,
            max_root_seq: 0,
            work: WorkDirs {
                cold: ObjectId::NONE,
                hot: ObjectId::NONE,
            },
            delta: TreeDelta::default(),
            table: PathTable::default(),
            undo: TxnUndo::default(),
            txn: Txn::None,
            stats: TreeStoreStats::default(),
        }
    }

    pub(crate) fn max_payload(&self) -> usize {
        (self.cfg.record_max - crate::record_header::RECORD_HEADER_LEN) as usize
    }

    /// Run one call: inside an explicit transaction as is (a failed call
    /// leaves the transaction as it was — calls change RAM state only after
    /// their records are written); outside, as a one-call transaction.
    pub(crate) fn op<T>(&mut self, f: impl FnOnce(&mut Self) -> Res<T, F>) -> Res<T, F> {
        if self.txn == Txn::Explicit {
            if self.delta.ram_bytes() >= self.cfg.txn_delta_max as usize {
                self.flush()?;
            }
            return f(self);
        }
        self.txn = Txn::Implicit;
        let r = f(self).and_then(|v| self.commit_inner().map(|()| v));
        if r.is_err() {
            self.abort_inner();
        }
        self.txn = Txn::None;
        r
    }

    /// Write the delta's directories, then a root if anything changed.
    pub(crate) fn commit_inner(&mut self) -> Res<(), F> {
        self.flush()?;
        let unchanged = self.committed.as_ref().is_some_and(|c| {
            c.root.cold_dir == self.work.cold
                && c.root.hot_dir == self.work.hot
                && c.root.retired == self.log.sectors.retired
        });
        if !unchanged {
            self.write_root()?;
        }
        self.undo.clear();
        Ok(())
    }

    pub(crate) fn abort_inner(&mut self) {
        self.undo.restore(&mut self.table);
        self.delta.clear();
        if let Some(c) = &self.committed {
            self.work = WorkDirs {
                cold: c.root.cold_dir,
                hot: c.root.hot_dir,
            };
        }
    }

    /// A root naming the working tree and the retired list, seq + 1.
    pub(crate) fn write_root(&mut self) -> Res<(), F> {
        let root = RootRecord {
            seq: self.max_root_seq.wrapping_add(1),
            cold_dir: self.work.cold,
            hot_dir: self.work.hot,
            retired: self.log.sectors.retired.clone(),
        };
        let payload = root.encode();
        let id = ObjectId::of(&mut self.hasher, IdTag::Root, &[&payload]);
        let len = crate::record_header::RECORD_HEADER_LEN + payload.len() as u32;
        self.ensure_room(&[(HeadKind::Hot, len)])?;
        self.log
            .append(HeadKind::Hot, RecordKind::Root, ChunkCodec::Stored, id, &[&payload])?;
        self.max_root_seq = root.seq;
        self.committed = Some(Committed { id, root });
        self.stats.commits += 1;
        Ok(())
    }

    /// The file at `path` by the path table (a collided row walks).
    pub(crate) fn lookup(&mut self, path: &str) -> Res<Option<FileEntry>, F> {
        if !valid_path(path) {
            return Ok(None);
        }
        let h = path_hash(&mut self.hasher, path);
        match self.table.get(h) {
            PathSlot::Absent => Ok(None),
            PathSlot::File { id, size } => Ok(Some(FileEntry { id, size })),
            PathSlot::Collided => self.walk_file(path),
        }
    }

    /// Whether the file at `path` exists, for a write: the table's row is
    /// confirmed by a walk unless this transaction wrote the path itself, so
    /// a row that belongs to another path (a hash collision) is found.
    pub(crate) fn existing(&mut self, path: &str) -> Res<Option<FileEntry>, F> {
        let h = path_hash(&mut self.hasher, path);
        match self.table.get(h) {
            PathSlot::Absent => Ok(None),
            PathSlot::File { id, size } if matches!(self.delta.lookup(path), Some(Some(_))) => {
                Ok(Some(FileEntry { id, size }))
            }
            _ => self.walk_file(path),
        }
    }

    /// The path table and the delta after writing `fe` at `path`.
    pub(crate) fn record_set(&mut self, path: &str, existed: Option<FileEntry>, fe: FileEntry) {
        let h = path_hash(&mut self.hasher, path);
        let prior = self.table.get(h);
        self.undo.save(h, prior);
        let slot = match prior {
            PathSlot::Absent => PathSlot::File {
                id: fe.id,
                size: fe.size,
            },
            PathSlot::File { .. } if existed.is_some() => PathSlot::File {
                id: fe.id,
                size: fe.size,
            },
            _ => PathSlot::Collided,
        };
        self.table.set(h, slot);
        self.delta.set(path, fe);
        self.note_txn_ram();
    }

    /// The path table and the delta after deleting the (existing) file.
    pub(crate) fn record_delete(&mut self, path: &str, _existed: FileEntry) {
        let h = path_hash(&mut self.hasher, path);
        let prior = self.table.get(h);
        self.undo.save(h, prior);
        if prior != PathSlot::Collided {
            self.table.set(h, PathSlot::Absent);
        }
        self.delta.delete(path);
        self.note_txn_ram();
    }

    pub(crate) fn note_txn_ram(&mut self) {
        let n = self.delta.ram_bytes() + self.undo.ram_bytes();
        self.log.note(n);
    }

    fn delete_prefix_inner(&mut self, prefix: &str) -> Res<(), F> {
        let files = self.list_files(prefix)?;
        self.log.note(
            files.capacity() * core::mem::size_of::<String>()
                + files.iter().map(|f| f.capacity()).sum::<usize>(),
        );
        let tree = prefix
            .strip_suffix('/')
            .filter(|d| valid_path(d) && !files.is_empty());
        for p in &files {
            let h = path_hash(&mut self.hasher, p);
            let prior = self.table.get(h);
            self.undo.save(h, prior);
            if prior != PathSlot::Collided {
                self.table.set(h, PathSlot::Absent);
            }
            if tree.is_none() {
                self.delta.delete(p);
            }
        }
        if let Some(dir) = tree {
            self.delta.delete_tree(dir);
        }
        self.note_txn_ram();
        Ok(())
    }
}

/// Absolute, no trailing `/`, no empty component, at most [`MAX_DEPTH`]
/// components and `u16::MAX` bytes.
pub fn valid_path(path: &str) -> bool {
    path.len() > 1
        && path.len() <= usize::from(u16::MAX)
        && path.starts_with('/')
        && !path.ends_with('/')
        && !path[1..].split('/').any(str::is_empty)
        && path.split('/').count() <= MAX_DEPTH + 1
}

/// The hot path: last two components are `.lp/panel.json`.
pub fn is_hot(path: &str) -> bool {
    path.ends_with("/.lp/panel.json")
}

pub(crate) fn head_for(path: &str) -> HeadKind {
    if is_hot(path) {
        HeadKind::Hot
    } else {
        HeadKind::Cold
    }
}

fn check_path<E>(path: &str) -> Result<(), StoreError<E>> {
    if valid_path(path) {
        Ok(())
    } else {
        Err(StoreError::InvalidPath)
    }
}

fn check_config<E>(
    sector_count: u32,
    sector_size: u32,
    cfg: &StoreConfig,
) -> Result<(), StoreError<E>> {
    if !(512..=32768).contains(&sector_size) {
        return Err(StoreError::BadConfig("sector size must be 512..=32768"));
    }
    if !(4..=u32::from(u16::MAX)).contains(&sector_count) {
        return Err(StoreError::BadConfig("sector count must be 4..=65535"));
    }
    if cfg.record_max < 128 || cfg.record_max > sector_size - SECTOR_HEADER_LEN {
        return Err(StoreError::BadConfig(
            "record_max must be 128..=sector size - 20",
        ));
    }
    if cfg.reserve + 2 >= sector_count {
        return Err(StoreError::BadConfig("reserve leaves no room"));
    }
    Ok(())
}
