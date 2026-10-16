//! **Prototype (A2, feature `checkpoint`, off by default; not part of format
//! version 3 — a ⚑ decision before FORMAT.md freezes).** A checkpoint of the
//! index, so mount reads the sector headers, the sectors written since the
//! checkpoint, the checkpoint and the closure — not a header of every record
//! on flash: its cost is bounded by the sectors, the live set and the
//! checkpoint interval, not by what was ever written.
//!
//! **On flash, additive only** (FORMAT.md "Versioning and extension"):
//!
//! - **Checkpoint chunk records**, record kind `6` (codec 0), a kind no
//!   known record names: a version-3 reader skips them as unknown records,
//!   and they are never live (no mark reaches them; GC drops them). Payload:
//!   the next chunk's reference (sector u16 ‖ offset u16 ‖ id u64; all zero
//!   for the last) ‖ a piece of the checkpoint stream. Id:
//!   `H(6 ‖ payload)`.
//! - **Root tail tag `1`**, 12 bytes: the first chunk's reference. Skippable:
//!   a reader that does not know it ignores it, a writer that does not know
//!   it drops it, and a missing or stale one is "not known" — mount then
//!   reads as without it. A writer carries its tag forward from root to
//!   root until it writes the next checkpoint.
//! - **The stream**: `q` u32 — the smaller of the two write heads' sector
//!   seqs when it was written: every record written since is in a sector
//!   with seq ≥ `q` — ‖ sector count u16 ‖ each sector's records end u16 ‖
//!   entry count u32 ‖ entries (id u64 ‖ sector u16 ‖ offset u16), sorted by
//!   id: the writer's index at that commit (its live records and anything
//!   written since its last mark).
//!
//! **Mount with it** (`try_mount`): find the newest root in the newest hot
//! sector holding one (roots are written to the hot head, and GC copies the
//! committed root there); read its tag and the chunk chain (each chunk's
//! header must name the kind, the id and a good CRC); read the sectors with
//! seq ≥ `q` as the fast mount does (the newest of each kind checked and
//! settled), collecting their records; take the older sectors' record ends
//! from the stream (a sector older than `q` is not appended to after it);
//! index the stream's entries in sectors older than `q` and the recent
//! sectors' records over them (newest copy first); and run GC's mark from
//! the root over that index — every record it reaches is read where the
//! index says, its header must name its id, and every root, directory and
//! multi is read whole and CRC-checked — then prune the index to what it
//! marked. Anything that does not hold (no tag, a chunk that does not check,
//! a newer root in a recent sector, a mark that fails) and mount reads as it
//! does without a checkpoint. Cut-safe the way the rest is: the chunks are
//! written and read back before the root that names them (I1), a cut before
//! that root leaves them garbage, and nothing else trusts them.

use alloc::vec::Vec;

use crate::flash::Flash;
use crate::gc_mark::{MarkRole, mark, prune};
use crate::mount_walk::Walked;
use crate::object_hasher::ObjectHasher;
use crate::object_id::{IdTag, ObjectId};
use crate::ram_index::{RamIndex, RecordLoc};
use crate::record_header::RECORD_HEADER_LEN;
use crate::record_log::RecordLog;
use crate::record_scan::{RootCandidates, scan_sector};
use crate::sector_filter::SectorFilter;
use crate::sector_header::{HeadKind, SECTOR_HEADER_LEN};
use crate::store_error::StoreError;
use crate::tree_store::{Res, TreeStore};

/// The root tail tag naming the first chunk.
pub const CHECKPOINT_TAG: u8 = 1;
/// The chunk records' kind byte.
pub const CHUNK_KIND: u8 = 6;
/// A chunk reference: sector u16 ‖ offset u16 ‖ id u64.
const REF_LEN: usize = 12;

/// Where a chunk is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChunkRef {
    pub loc: RecordLoc,
    pub id: ObjectId,
}

impl ChunkRef {
    fn encode(r: Option<ChunkRef>) -> [u8; REF_LEN] {
        let mut b = [0u8; REF_LEN];
        if let Some(r) = r {
            b[0..2].copy_from_slice(&(r.loc.sector as u16).to_le_bytes());
            b[2..4].copy_from_slice(&(r.loc.offset as u16).to_le_bytes());
            b[4..12].copy_from_slice(&r.id.0.to_le_bytes());
        }
        b
    }

    fn decode(b: &[u8]) -> Option<ChunkRef> {
        let id = u64::from_le_bytes(b[4..12].try_into().ok()?);
        (id != 0).then(|| ChunkRef {
            loc: RecordLoc {
                sector: u32::from(u16::from_le_bytes([b[0], b[1]])),
                offset: u32::from(u16::from_le_bytes([b[2], b[3]])),
            },
            id: ObjectId(id),
        })
    }
}

/// The writer's checkpoint state (in `RecordLog`, RAM only).
#[derive(Default)]
pub struct CheckpointState {
    /// The checkpoint the next root names.
    pub current: Option<ChunkRef>,
    /// The sectors its chunks are in: erasing one makes it stale.
    pub sectors: Vec<u16>,
    /// Records appended since it was written.
    pub records_since: u32,
}

impl CheckpointState {
    /// A sector is being erased.
    pub fn erasing(&mut self, s: u32) {
        if self.sectors.contains(&(s as u16)) {
            self.current = None;
            self.sectors.clear();
        }
    }
}

/// The tag's value in a root payload, if it carries one that parses.
pub fn root_tag(payload: &[u8]) -> Option<ChunkRef> {
    let n = usize::from(u16::from_le_bytes([*payload.get(24)?, *payload.get(25)?]));
    let mut t = 26 + 2 * n;
    while t + 3 <= payload.len() {
        let len = usize::from(u16::from_le_bytes([payload[t + 1], payload[t + 2]]));
        if payload[t] == CHECKPOINT_TAG && len == REF_LEN && t + 3 + len <= payload.len() {
            return ChunkRef::decode(&payload[t + 3..t + 3 + len]);
        }
        t += 3 + len;
    }
    None
}

/// The tag entry for `r`.
pub fn tag_bytes(r: ChunkRef) -> [u8; 3 + REF_LEN] {
    let mut b = [0u8; 3 + REF_LEN];
    b[0] = CHECKPOINT_TAG;
    b[1..3].copy_from_slice(&(REF_LEN as u16).to_le_bytes());
    b[3..].copy_from_slice(&ChunkRef::encode(Some(r)));
    b
}

impl<F: Flash, H: ObjectHasher> TreeStore<F, H> {
    /// Whether the next root should come with a new checkpoint.
    pub(crate) fn checkpoint_due(&self) -> bool {
        let every = self.cfg.checkpoint_every;
        every > 0 && (self.log.ckpt.current.is_none() || self.log.ckpt.records_since >= every)
    }

    /// Write a checkpoint of the index as it is (the commit's records are
    /// all written): its chunks last to first, each naming the next. `None`
    /// when it does not fit or GC erased a chunk meanwhile.
    pub(crate) fn write_checkpoint(&mut self) -> Res<Option<ChunkRef>, F> {
        let q = HeadKind::ALL
            .iter()
            .filter_map(|k| self.log.heads[k.index()])
            .map(|s| self.log.sectors.seq[s as usize])
            .min()
            .unwrap_or(self.log.next_sector_seq);
        let ix = &self.log.index;
        let n = self.log.sector_count as usize;
        let mut stream = Vec::with_capacity(10 + 2 * n + 12 * ix.len());
        stream.extend_from_slice(&q.to_le_bytes());
        stream.extend_from_slice(&(n as u16).to_le_bytes());
        for &e in &self.log.sectors.end {
            stream.extend_from_slice(&e.to_le_bytes());
        }
        stream.extend_from_slice(&(ix.len() as u32).to_le_bytes());
        for i in 0..ix.len() {
            let loc = ix.loc_at(i);
            stream.extend_from_slice(&ix.id_at(i).0.to_le_bytes());
            stream.extend_from_slice(&(loc.sector as u16).to_le_bytes());
            stream.extend_from_slice(&(loc.offset as u16).to_le_bytes());
        }
        self.log.note(stream.capacity());
        let piece = self.max_payload() - REF_LEN;
        let pieces: Vec<&[u8]> = stream.chunks(piece).collect();
        let len = |p: &[u8]| RECORD_HEADER_LEN + (REF_LEN + p.len()) as u32;
        let need: u32 = pieces.iter().map(|p| len(p)).sum();
        match self.ensure_room(&[(HeadKind::Cold, need)]) {
            Ok(()) => {}
            Err(StoreError::NoSpace) => return Ok(None),
            Err(e) => return Err(e),
        }
        self.log.ckpt.current = None;
        self.log.ckpt.sectors.clear();
        let mut next: Option<ChunkRef> = None;
        let mut sectors = Vec::new();
        for p in pieces.iter().rev() {
            let r = ChunkRef::encode(next);
            let id = ObjectId::of(&mut self.hasher, IdTag::Checkpoint, &[&r, p]);
            let loc = self
                .log
                .append_raw(HeadKind::Cold, CHUNK_KIND, id, &[&r, p])?;
            if !sectors.contains(&(loc.sector as u16)) {
                sectors.push(loc.sector as u16);
            }
            next = Some(ChunkRef { loc, id });
            // A chunk written is not live: the next mark forgets it.
        }
        self.log.ckpt.sectors = sectors;
        self.log.ckpt.records_since = 0;
        self.log.ckpt.current = next;
        Ok(next)
    }

    /// Mount by the newest root's checkpoint, or `None` (mount reads as it
    /// does without one). `valid`: the trusted sectors by ascending seq.
    pub(crate) fn try_checkpoint_mount(
        &mut self,
        valid: &[u16],
        closed: &mut [bool],
        newest: [Option<u32>; 2],
    ) -> Res<Option<(ObjectId, Walked)>, F> {
        let seq = |log: &RecordLog<F>, s: u16| log.sectors.seq[s as usize];
        let mut roots = RootCandidates::default();
        let mut residue = crate::mount_verdict::FormatResidue::default();
        let mut filter = SectorFilter::new(self.log.sector_count);
        let mut recent: Vec<(ObjectId, RecordLoc)> = Vec::new();
        let mut scanned = alloc::vec![false; self.log.sector_count as usize];
        // The newest hot sector holding a root.
        for &s in valid.iter().rev() {
            let s32 = u32::from(s);
            if self.log.sectors.kind(s32) != HeadKind::Hot {
                continue;
            }
            self.scan_one(
                s32,
                closed,
                newest,
                &mut roots,
                &mut residue,
                &mut filter,
                &mut recent,
            )?;
            scanned[s as usize] = true;
            if roots.best[0].is_some() {
                break;
            }
        }
        let Some(root) = roots.best[0] else {
            return Ok(None);
        };
        let payload = self.read_at(root.loc)?;
        let Some(first) = payload.as_deref().and_then(root_tag) else {
            return Ok(None);
        };
        let Some(stream) = self.read_chain(first)? else {
            return Ok(None);
        };
        let Some(ck) = Stream::parse(&stream, self.log.sector_count) else {
            return Ok(None);
        };
        if seq(&self.log, root.loc.sector as u16) < ck.q {
            return Ok(None);
        }
        // The sectors written since: read as the fast mount reads them.
        for &s in valid {
            if seq(&self.log, s) >= ck.q && !scanned[s as usize] {
                let s32 = u32::from(s);
                self.scan_one(
                    s32,
                    closed,
                    newest,
                    &mut roots,
                    &mut residue,
                    &mut filter,
                    &mut recent,
                )?;
            }
        }
        if roots.best[0].map(|b| b.id) != Some(root.id) {
            return Ok(None);
        }
        self.max_root_seq = roots.max_seq;
        // The older sectors: their ends and their records, from the stream.
        let mut entries: Vec<(ObjectId, RecordLoc, u32)> = Vec::new();
        for &s in valid {
            if seq(&self.log, s) < ck.q {
                let e = ck.end(s as usize);
                if u32::from(e) < SECTOR_HEADER_LEN || u32::from(e) > self.log.sector_size {
                    return Ok(None);
                }
                self.log.sectors.end[s as usize] = e;
            }
        }
        let old = |log: &RecordLog<F>, sector: u32| {
            sector < log.sector_count
                && log.sectors.is_written(sector)
                && log.sectors.seq[sector as usize] < ck.q
                && valid.contains(&(sector as u16))
        };
        for (id, loc) in ck.entries() {
            if old(&self.log, loc.sector) {
                entries.push((id, loc, 0));
            }
        }
        for (id, loc) in recent {
            entries.push((id, loc, 1 + self.log.sectors.seq[loc.sector as usize]));
        }
        // By id, the newest copy first; one entry per id.
        crate::heap_sort::heap_sort_by(&mut entries, |a, b| (a.0.0, b.2) < (b.0.0, a.2));
        entries.dedup_by_key(|e| e.0.0);
        self.log.index = RamIndex::from_sorted(entries.iter().map(|e| (e.0, e.1)));
        drop(entries);
        let mut m = match mark(&mut self.log, &[(root.id, MarkRole::Root)], false) {
            Ok(m) => m,
            Err(StoreError::Corrupt(_)) => return Ok(None),
            Err(e) => return Err(e),
        };
        prune(&mut self.log, &mut m);
        self.log.ckpt.current = Some(first);
        self.log.ckpt.sectors.clear();
        self.log.ckpt.sectors.push(first.loc.sector as u16);
        let live = core::mem::take(&mut self.log.sectors.live);
        Ok(Some((
            root.id,
            Walked {
                live,
                #[cfg(feature = "stats")]
                scans: 0,
            },
        )))
    }

    /// `scan_sector` as the fast mount runs it, its records collected.
    #[allow(
        clippy::too_many_arguments,
        reason = "the prototype keeps scan_sector as it is"
    )]
    fn scan_one(
        &mut self,
        s: u32,
        closed: &mut [bool],
        newest: [Option<u32>; 2],
        roots: &mut RootCandidates,
        residue: &mut crate::mount_verdict::FormatResidue,
        filter: &mut SectorFilter,
        recent: &mut Vec<(ObjectId, RecordLoc)>,
    ) -> Res<(), F> {
        let settle = newest.contains(&Some(s));
        let check = closed[s as usize] || settle;
        let scan = scan_sector(&mut self.log, s, roots, residue, filter, check, settle)?;
        self.log.sectors.end[s as usize] = scan.end as u16;
        closed[s as usize] |= scan.closed;
        // The records it trusted, by a second walk of their headers (the
        // prototype keeps `scan_sector` as it is).
        let mut off = SECTOR_HEADER_LEN;
        while off + RECORD_HEADER_LEN <= scan.end {
            let h = match self.log.header_raw(s, off)? {
                crate::record_header::HeaderRead::Record(h) => {
                    recent.push((
                        h.id,
                        RecordLoc {
                            sector: s,
                            offset: off,
                        },
                    ));
                    h.total_len()
                }
                crate::record_header::HeaderRead::Unknown { len } => {
                    RECORD_HEADER_LEN + u32::from(len)
                }
                _ => break,
            };
            off += h;
        }
        Ok(())
    }

    /// A root's payload at `loc`, if its header and CRC check.
    fn read_at(&mut self, loc: RecordLoc) -> Res<Option<Vec<u8>>, F> {
        let mut raw = [0u8; RECORD_HEADER_LEN as usize];
        self.log
            .read(self.log.addr(loc.sector, loc.offset), &mut raw)?;
        match self.log.read_payload(loc, &raw) {
            Ok(p) => Ok(Some(p)),
            Err(StoreError::Corrupt(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// The stream a chunk chain holds, if every chunk checks.
    fn read_chain(&mut self, first: ChunkRef) -> Res<Option<Vec<u8>>, F> {
        let mut out = Vec::new();
        let mut at = Some(first);
        let mut hops = 0;
        while let Some(r) = at {
            hops += 1;
            if hops > self.log.sector_count * 16 || r.loc.sector >= self.log.sector_count {
                return Ok(None);
            }
            let mut raw = [0u8; RECORD_HEADER_LEN as usize];
            self.log
                .read(self.log.addr(r.loc.sector, r.loc.offset), &mut raw)?;
            let id = u64::from_le_bytes(raw[4..12].try_into().unwrap());
            let len = usize::from(u16::from_le_bytes([raw[2], raw[3]]));
            if raw[0] != CHUNK_KIND
                || raw[1] != 0
                || id != r.id.0
                || len < REF_LEN
                || r.loc.offset + RECORD_HEADER_LEN + len as u32 > self.log.sector_size
            {
                return Ok(None);
            }
            let p = match self.log.read_payload(r.loc, &raw) {
                Ok(p) => p,
                Err(StoreError::Corrupt(_)) => return Ok(None),
                Err(e) => return Err(e),
            };
            out.extend_from_slice(&p[REF_LEN..]);
            at = ChunkRef::decode(&p[..REF_LEN]);
        }
        self.log.note(out.capacity());
        Ok(Some(out))
    }
}

/// A parsed stream.
struct Stream<'a> {
    q: u32,
    ends: &'a [u8],
    entries: &'a [u8],
}

impl<'a> Stream<'a> {
    fn parse(b: &'a [u8], sectors: u32) -> Option<Self> {
        let q = u32::from_le_bytes(b.get(0..4)?.try_into().ok()?);
        let n = usize::from(u16::from_le_bytes(b.get(4..6)?.try_into().ok()?));
        if n != sectors as usize {
            return None;
        }
        let ends = b.get(6..6 + 2 * n)?;
        let at = 6 + 2 * n;
        let count = u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?) as usize;
        let entries = b.get(at + 4..)?;
        (entries.len() == 12 * count).then_some(Stream { q, ends, entries })
    }

    fn end(&self, s: usize) -> u16 {
        u16::from_le_bytes([self.ends[2 * s], self.ends[2 * s + 1]])
    }

    fn entries(&self) -> impl Iterator<Item = (ObjectId, RecordLoc)> + 'a {
        self.entries.chunks_exact(12).map(|e| {
            (
                ObjectId(u64::from_le_bytes(e[0..8].try_into().unwrap())),
                RecordLoc {
                    sector: u32::from(u16::from_le_bytes([e[8], e[9]])),
                    offset: u32::from(u16::from_le_bytes([e[10], e[11]])),
                },
            )
        })
    }
}
