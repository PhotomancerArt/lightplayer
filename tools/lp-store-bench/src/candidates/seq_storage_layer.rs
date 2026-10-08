//! **S1 — `sequential-storage` + a chunk/commit layer.** The crate's `map`
//! (8.0.2, MIT OR Apache-2.0) over [`NorFlashSim`], with documents chunked
//! because an item must fit in one 4 KiB page.
//!
//! - **Keys** are 64-bit hashes (FNV-1a, then a SplitMix finaliser): a
//!   document's manifest under `h("M", path)`, chunk *i* of generation *g*
//!   under `h("C", path, g, i)`.
//! - **Values** carry a one-byte tag. A chunk is `'C' | gen u32 | idx u16 |
//!   owner u64 (the manifest key) | ≤ 1000 B of data`. A manifest is `'M' |
//!   gen u32 | chunks u16 | len u32 | crc32 u32 | path`.
//! - **A put** writes the next generation's chunks, then the manifest (the
//!   switch: the document is old until it lands, new after), then removes the
//!   old generation's chunks. Per-document atomic; a step is not
//!   (`step_atomic: false`).
//! - **Deletes wait for `commit`.** `delete_prefix` records the prefix; until
//!   the commit the matching documents read as gone (unless put again since),
//!   and the commit removes each one still matching, manifest first. Applying
//!   a delete at once would make a push's wipe-then-rewrite lose every
//!   document not yet rewritten when a cut comes; deferred, each document goes
//!   straight from old to new. (A document put and then deleted in one step
//!   is still briefly its put value — not old, not new — if a cut lands
//!   before the commit.)
//! - **`list` scans** with the crate's own iteration (`fetch_all_items`),
//!   last item per key wins, keeping manifests: no RAM index and no
//!   path-list key to keep consistent, at the price of a full scan per call.
//! - **Orphans** — chunks of a generation whose manifest never landed, or
//!   whose old generation was not yet removed, when a cut came — are swept
//!   once per mount, before the first write: a scan, then `remove_item` for
//!   every chunk its manifest does not name. Mount itself reads nothing (the
//!   crate repairs lazily, on the operation that finds damage).
//! - **Format** erases the partition (`erase_all`); an erased partition is an
//!   empty map.
//! - **Cache:** heap page-state + page-pointer caches (1 + 8 B per page), no
//!   key-pointer cache (keys are many and hashed).
//! - **Async:** the crate's futures run on a null-waker `block_on` written
//!   here — the flash is synchronous, so every poll completes (a test edge,
//!   per AGENTS.md "Sans-IO core"). The flash newtype declares the C6's
//!   4-byte word (as `esp-storage` does).

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::pin::pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

use embedded_storage_async::nor_flash::{ErrorType, MultiwriteNorFlash, NorFlash, ReadNorFlash};
use lp_nor_sim::{NorError, NorFlashSim, NorSimFlashError};
use sequential_storage::Error as SeqError;
use sequential_storage::cache::page_pointers::HeapPagePointers;
use sequential_storage::cache::page_states::HeapPageStates;
use sequential_storage::cache::{Cache, Uncached};
use sequential_storage::map::{MapConfig, MapStorage};

use crate::{Candidate, CandidateConfig, CandidateReport, CandidateStore, StoreError};

/// Data bytes per chunk (the spike's best).
pub const CHUNK: usize = 1000;
/// The longest path a manifest holds.
pub const MAX_PATH: usize = 255;
const CHUNK_HEAD: usize = 1 + 4 + 2 + 8;
const MANIFEST_HEAD: usize = 1 + 4 + 2 + 4 + 4;
/// key (8) + the larger value, rounded up to the 4-byte word.
const BUF: usize = (8 + max(CHUNK_HEAD + CHUNK, MANIFEST_HEAD + MAX_PATH)).div_ceil(4) * 4;

const fn max(a: usize, b: usize) -> usize {
    if a > b { a } else { b }
}

type SeqCache = Cache<HeapPageStates, HeapPagePointers, Uncached, u64>;
type Map = MapStorage<u64, SeqFlash, SeqCache>;

pub struct SeqStorageLayer;

impl Candidate for SeqStorageLayer {
    fn name(&self) -> &str {
        "s1"
    }

    fn format(&self, flash: &mut NorFlashSim, cfg: &CandidateConfig) -> Result<(), StoreError> {
        let shared = Rc::new(RefCell::new(flash.clone()));
        let mut map = new_map(&shared, cfg);
        let r = block_on(map.erase_all()).map_err(|e| seq_err(&shared, e));
        drop(map);
        *flash = take(shared);
        r
    }

    fn mount(
        &self,
        flash: NorFlashSim,
        cfg: &CandidateConfig,
    ) -> Result<Box<dyn CandidateStore>, (StoreError, NorFlashSim)> {
        let shared = Rc::new(RefCell::new(flash));
        Ok(Box::new(SeqStore {
            map: new_map(&shared, cfg),
            flash: shared,
            buf: vec![0u8; BUF],
            swept: false,
            pages: cfg.sectors,
            doomed: Vec::new(),
            rewritten: BTreeSet::new(),
        }))
    }
}

fn new_map(shared: &Rc<RefCell<NorFlashSim>>, cfg: &CandidateConfig) -> Map {
    let pages = cfg.sectors as usize;
    MapStorage::new(
        SeqFlash(shared.clone()),
        MapConfig::new(0..cfg.sectors * 4096),
        Cache::new(
            HeapPageStates::new(pages),
            HeapPagePointers::new(pages),
            Uncached,
        ),
    )
}

fn take(shared: Rc<RefCell<NorFlashSim>>) -> NorFlashSim {
    match Rc::try_unwrap(shared) {
        Ok(c) => c.into_inner(),
        Err(rc) => rc.borrow().clone(),
    }
}

/// A crate error as the harness sees it (a cut is whatever the flash says).
fn seq_err(flash: &Rc<RefCell<NorFlashSim>>, e: SeqError<NorSimFlashError>) -> StoreError {
    if !flash.borrow().is_powered() {
        return StoreError::PowerLost;
    }
    match e {
        SeqError::Storage { value, .. } => match value.0 {
            NorError::PowerLost => StoreError::PowerLost,
            NorError::Watchdog => StoreError::Other("flash read watchdog".into()),
            NorError::OutOfBounds => StoreError::Other("flash out of bounds".into()),
        },
        SeqError::FullStorage => StoreError::NoSpace,
        SeqError::Corrupted { .. } => StoreError::Corrupt("sequential-storage: corrupted".into()),
        e => StoreError::Other(format!("sequential-storage: {e:?}")),
    }
}

/// The async NOR traits over the shared sim, with the C6's 4-byte word.
struct SeqFlash(Rc<RefCell<NorFlashSim>>);

impl ErrorType for SeqFlash {
    type Error = NorSimFlashError;
}

impl ReadNorFlash for SeqFlash {
    const READ_SIZE: usize = 4;

    async fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
        self.0
            .borrow_mut()
            .read(offset, bytes)
            .map_err(NorSimFlashError)
    }

    fn capacity(&self) -> usize {
        self.0.borrow().geometry().capacity() as usize
    }
}

impl NorFlash for SeqFlash {
    const WRITE_SIZE: usize = 4;
    const ERASE_SIZE: usize = 4096;

    async fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error> {
        let mut f = self.0.borrow_mut();
        for s in from / 4096..to / 4096 {
            f.erase_sector(s).map_err(NorSimFlashError)?;
        }
        Ok(())
    }

    async fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
        self.0
            .borrow_mut()
            .program(offset, bytes)
            .map_err(NorSimFlashError)
    }
}

impl MultiwriteNorFlash for SeqFlash {}

/// Run a future whose every poll completes (the flash is synchronous). A
/// `Pending` could never be woken, so it is a bug, and panics.
fn block_on<F: Future>(f: F) -> F::Output {
    let mut f = pin!(f);
    let mut cx = Context::from_waker(Waker::noop());
    match f.as_mut().poll(&mut cx) {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("sequential-storage future pended on a synchronous flash"),
    }
}

/// A document's manifest.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Manifest {
    gen_: u32,
    chunks: u16,
    len: u32,
    crc: u32,
    path: String,
}

impl Manifest {
    fn encode(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(MANIFEST_HEAD + self.path.len());
        v.push(b'M');
        v.extend_from_slice(&self.gen_.to_le_bytes());
        v.extend_from_slice(&self.chunks.to_le_bytes());
        v.extend_from_slice(&self.len.to_le_bytes());
        v.extend_from_slice(&self.crc.to_le_bytes());
        v.extend_from_slice(self.path.as_bytes());
        v
    }

    fn decode(v: &[u8]) -> Option<Self> {
        if v.len() < MANIFEST_HEAD || v[0] != b'M' {
            return None;
        }
        Some(Self {
            gen_: u32::from_le_bytes(v[1..5].try_into().ok()?),
            chunks: u16::from_le_bytes(v[5..7].try_into().ok()?),
            len: u32::from_le_bytes(v[7..11].try_into().ok()?),
            crc: u32::from_le_bytes(v[11..15].try_into().ok()?),
            path: String::from_utf8(v[MANIFEST_HEAD..].to_vec()).ok()?,
        })
    }
}

/// A chunk's header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ChunkHead {
    gen_: u32,
    idx: u16,
    owner: u64,
}

impl ChunkHead {
    fn decode(v: &[u8]) -> Option<Self> {
        if v.len() < CHUNK_HEAD || v[0] != b'C' {
            return None;
        }
        Some(Self {
            gen_: u32::from_le_bytes(v[1..5].try_into().ok()?),
            idx: u16::from_le_bytes(v[5..7].try_into().ok()?),
            owner: u64::from_le_bytes(v[7..15].try_into().ok()?),
        })
    }
}

fn hash(parts: &[&[u8]]) -> u64 {
    let mut x: u64 = 0xcbf2_9ce4_8422_2325;
    for p in parts {
        for &b in *p {
            x ^= b as u64;
            x = x.wrapping_mul(0x0100_0000_01b3);
        }
        x ^= 0xff;
        x = x.wrapping_mul(0x0100_0000_01b3);
    }
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

fn manifest_key(path: &str) -> u64 {
    hash(&[b"M", path.as_bytes()])
}

fn chunk_key(path: &str, gen_: u32, idx: u16) -> u64 {
    hash(&[
        b"C",
        path.as_bytes(),
        &gen_.to_le_bytes(),
        &idx.to_le_bytes(),
    ])
}

struct SeqStore {
    map: Map,
    flash: Rc<RefCell<NorFlashSim>>,
    buf: Vec<u8>,
    swept: bool,
    pages: u32,
    /// Prefixes deleted since the last commit.
    doomed: Vec<String>,
    /// Paths put since the last delete that matched them.
    rewritten: BTreeSet<String>,
}

/// Every live item, last per key: manifests by key, chunk heads by key.
#[derive(Default)]
struct Scan {
    manifests: BTreeMap<u64, Manifest>,
    chunks: BTreeMap<u64, ChunkHead>,
}

impl SeqStore {
    fn err(&self, e: SeqError<NorSimFlashError>) -> StoreError {
        seq_err(&self.flash, e)
    }

    fn fetch(&mut self, key: u64) -> Result<Option<Vec<u8>>, StoreError> {
        let r = block_on(self.map.fetch_item::<&[u8]>(&mut self.buf, &key));
        match r {
            Ok(v) => Ok(v.map(<[u8]>::to_vec)),
            Err(e) => Err(self.err(e)),
        }
    }

    fn store(&mut self, key: u64, value: &[u8]) -> Result<(), StoreError> {
        let r = block_on(self.map.store_item(&mut self.buf, &key, &value));
        r.map_err(|e| self.err(e))
    }

    fn remove(&mut self, key: u64) -> Result<(), StoreError> {
        let r = block_on(self.map.remove_item(&mut self.buf, &key));
        r.map_err(|e| self.err(e))
    }

    fn manifest(&mut self, path: &str) -> Result<Option<Manifest>, StoreError> {
        let Some(v) = self.fetch(manifest_key(path))? else {
            return Ok(None);
        };
        let m = Manifest::decode(&v)
            .ok_or_else(|| StoreError::Corrupt(format!("{path}: bad manifest")))?;
        if m.path != path {
            return Err(StoreError::Corrupt(format!(
                "{path}: manifest key collides with {}",
                m.path
            )));
        }
        Ok(Some(m))
    }

    /// One pass over every item with the crate's iterator.
    fn scan(&mut self) -> Result<Scan, StoreError> {
        let mut out = Scan::default();
        let flash = self.flash.clone();
        let buf = &mut self.buf;
        let r: Result<(), SeqError<NorSimFlashError>> = block_on(async {
            let mut it = self.map.fetch_all_items(buf).await?;
            let mut item = vec![0u8; BUF];
            loop {
                let Some((key, v)) = it.next::<&[u8]>(&mut item).await? else {
                    break;
                };
                if let Some(m) = Manifest::decode(v) {
                    out.chunks.remove(&key);
                    out.manifests.insert(key, m);
                } else if let Some(c) = ChunkHead::decode(v) {
                    out.manifests.remove(&key);
                    out.chunks.insert(key, c);
                }
            }
            Ok(())
        });
        r.map_err(|e| seq_err(&flash, e))?;
        Ok(out)
    }

    /// Remove every chunk no manifest names (once per mount, before writing).
    fn sweep_orphans(&mut self) -> Result<(), StoreError> {
        if self.swept {
            return Ok(());
        }
        let scan = self.scan()?;
        let live: BTreeSet<u64> = scan
            .manifests
            .values()
            .flat_map(|m| (0..m.chunks).map(|i| chunk_key(&m.path, m.gen_, i)))
            .collect();
        for (key, head) in &scan.chunks {
            let named = live.contains(key)
                && scan
                    .manifests
                    .get(&head.owner)
                    .is_some_and(|m| m.gen_ == head.gen_ && head.idx < m.chunks);
            if !named {
                self.remove(*key)?;
            }
        }
        self.swept = true;
        Ok(())
    }

    /// Deleted since the last commit and not put again?
    fn is_doomed(&self, path: &str) -> bool {
        !self.rewritten.contains(path) && self.doomed.iter().any(|p| path.starts_with(p.as_str()))
    }

    fn remove_doc(&mut self, m: &Manifest) -> Result<(), StoreError> {
        self.remove(manifest_key(&m.path))?;
        for i in 0..m.chunks {
            self.remove(chunk_key(&m.path, m.gen_, i))?;
        }
        Ok(())
    }
}

impl CandidateStore for SeqStore {
    fn put(&mut self, path: &str, bytes: &[u8]) -> Result<(), StoreError> {
        if path.len() > MAX_PATH {
            return Err(StoreError::Other(format!("path longer than {MAX_PATH}")));
        }
        self.sweep_orphans()?;
        let old = self.manifest(path)?;
        let gen_ = old.as_ref().map_or(1, |m| m.gen_.wrapping_add(1));
        let owner = manifest_key(path);
        let chunks = bytes.len().div_ceil(CHUNK);
        if chunks > u16::MAX as usize {
            return Err(StoreError::Other("document too large".into()));
        }
        for (i, part) in bytes.chunks(CHUNK).enumerate() {
            let mut v = Vec::with_capacity(CHUNK_HEAD + part.len());
            v.push(b'C');
            v.extend_from_slice(&gen_.to_le_bytes());
            v.extend_from_slice(&(i as u16).to_le_bytes());
            v.extend_from_slice(&owner.to_le_bytes());
            v.extend_from_slice(part);
            self.store(chunk_key(path, gen_, i as u16), &v)?;
        }
        let m = Manifest {
            gen_,
            chunks: chunks as u16,
            len: bytes.len() as u32,
            crc: lp_crc32::crc32(bytes),
            path: path.into(),
        };
        self.store(owner, &m.encode())?;
        if !self.doomed.is_empty() {
            self.rewritten.insert(path.into());
        }
        if let Some(old) = old {
            for i in 0..old.chunks {
                self.remove(chunk_key(path, old.gen_, i))?;
            }
        }
        Ok(())
    }

    fn get(&mut self, path: &str) -> Result<Option<Vec<u8>>, StoreError> {
        if self.is_doomed(path) {
            return Ok(None);
        }
        let Some(m) = self.manifest(path)? else {
            return Ok(None);
        };
        let mut out = Vec::with_capacity(m.len as usize);
        for i in 0..m.chunks {
            let v = self
                .fetch(chunk_key(path, m.gen_, i))?
                .ok_or_else(|| StoreError::Corrupt(format!("{path}: chunk {i} missing")))?;
            let h = ChunkHead::decode(&v)
                .ok_or_else(|| StoreError::Corrupt(format!("{path}: chunk {i} malformed")))?;
            if h.gen_ != m.gen_ || h.idx != i {
                return Err(StoreError::Corrupt(format!("{path}: chunk {i} is foreign")));
            }
            out.extend_from_slice(&v[CHUNK_HEAD..]);
        }
        if out.len() != m.len as usize || lp_crc32::crc32(&out) != m.crc {
            return Err(StoreError::Corrupt(format!("{path}: content crc")));
        }
        Ok(Some(out))
    }

    fn delete_prefix(&mut self, prefix: &str) -> Result<(), StoreError> {
        self.rewritten.retain(|p| !p.starts_with(prefix));
        self.doomed.push(prefix.into());
        Ok(())
    }

    fn list(&mut self, prefix: &str) -> Result<Vec<String>, StoreError> {
        let mut out: Vec<String> = self
            .scan()?
            .manifests
            .into_values()
            .map(|m| m.path)
            .filter(|p| p.starts_with(prefix) && !self.is_doomed(p))
            .collect();
        out.sort();
        out.dedup();
        Ok(out)
    }

    fn commit(&mut self) -> Result<(), StoreError> {
        if self.doomed.is_empty() {
            return Ok(());
        }
        self.sweep_orphans()?;
        let doomed: Vec<Manifest> = self
            .scan()?
            .manifests
            .into_values()
            .filter(|m| self.is_doomed(&m.path))
            .collect();
        for m in &doomed {
            self.remove_doc(m)?;
        }
        self.doomed.clear();
        self.rewritten.clear();
        Ok(())
    }

    fn into_flash(self: Box<Self>) -> NorFlashSim {
        let flash = self.flash.clone();
        drop(self);
        take(flash)
    }

    fn flash_snapshot(&self) -> NorFlashSim {
        self.flash.borrow().clone()
    }

    fn report(&self) -> CandidateReport {
        let cache = self.pages as u64 * (1 + 8);
        let mut extra = BTreeMap::new();
        extra.insert("cache_bytes".into(), cache as f64);
        extra.insert("data_buffer_bytes".into(), BUF as f64);
        extra.insert("chunk_bytes".into(), CHUNK as f64);
        CandidateReport {
            // The page caches, the crate's data buffer, the iterator's item
            // buffer during a scan, and the map handle itself.
            ram_bytes: cache + 2 * BUF as u64 + std::mem::size_of::<Map>() as u64,
            used_sectors: None,
            step_atomic: false,
            extra,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::candidates::candidate_tests::{round_trip, small_sweep};

    #[test]
    fn values_round_trip_and_keys_separate() {
        let m = Manifest {
            gen_: 7,
            chunks: 3,
            len: 2500,
            crc: 9,
            path: "/projects/a/x.json".into(),
        };
        assert_eq!(Manifest::decode(&m.encode()), Some(m));
        assert_ne!(chunk_key("/a", 1, 0), chunk_key("/a", 2, 0));
        assert_ne!(chunk_key("/a", 1, 0), manifest_key("/a"));
        assert!(BUF >= 8 + CHUNK_HEAD + CHUNK && BUF % 4 == 0);
    }

    #[test]
    fn round_trips_fault_free() {
        round_trip(&SeqStorageLayer);
    }

    #[test]
    fn small_exhaustive_sweep_runs() {
        let s = small_sweep(&SeqStorageLayer);
        eprintln!("s1 small sweep: {s:?}");
        assert!(s.cases > 0);
    }
}
