//! Making room before a write: the admission rule (every write phase: the
//! live bytes and the write's against the usable sectors, each with a
//! packing slack — on the live upper bounds, made exact by a mark when they
//! say no), then the cheap check (the layout has the room), then a full
//! mark that makes the live bytes exact and prunes the index, then the
//! refusal memory and the packing bound (`NoSpace` before anything is
//! written), then GC until it fits, and last a renewed head.
//!
//! This is the only place GC runs, and it runs only at the start of a write
//! phase, when everything already written is reachable from the mark's
//! roots: the committed root, the working tree, and the delta's file ids.

use alloc::vec::Vec;

use crate::flash::Flash;
use crate::gc_copy::collect_sector;
use crate::gc_mark::{MarkRole, mark, prune};
use crate::gc_victim::choose_victim;
use crate::object_hasher::ObjectHasher;
use crate::object_id::ObjectId;
use crate::sector_header::{HeadKind, SECTOR_HEADER_LEN};
use crate::space_estimate::{HOT_LEN, admits, fits_after_compaction, sectors_needed};
use crate::store_error::StoreError;
use crate::tree_store::{Res, TreeStore};

/// A write GC could not make room for: its live bytes plus the write's,
/// and the free sectors GC left. Until a write needs fewer bytes or a
/// sector more is free (a delete, a shrink, a GC that won a sector), one
/// needing as many bytes is refused without GC running again: a refused
/// write retried in a loop cost one sector tens of thousands of erases.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Refusal {
    pub bytes: u64,
    pub free: u32,
}

impl<F: Flash, H: ObjectHasher> TreeStore<F, H> {
    /// Make the free sectors cover `need` (records in append order) plus
    /// the reserve. Also the index's bound: between marks it holds every
    /// record written since, so once it has grown an eighth (+ 16) past the
    /// live set of the last mark, mark now and prune it back.
    pub(crate) fn ensure_room(&mut self, need: &[(HeadKind, u32)]) -> Res<(), F> {
        #[cfg(feature = "mutants")]
        if self.dry {
            return Ok(());
        }
        let live = self.live_after_mark;
        if self.log.index.len() > live + live / 8 + 16 {
            self.mark_and_prune(false)?;
        }
        // Admission reads only the live set and the write, never the
        // layout, so a step and its re-run after a power cut get the same
        // answer (the 2026-10-10 defect: near full, "fits" by the layout
        // was yes for one and no for the other).
        let mut lens = None;
        if !self.admitted(need) {
            lens = Some(self.mark_and_prune(true)?);
            if !self.admitted(need) {
                return Err(StoreError::NoSpace);
            }
        }
        if self.enough(need) {
            return Ok(());
        }
        let lens = match lens {
            Some(l) => l,
            None => self.mark_and_prune(true)?,
        };
        if self.enough(need) {
            return Ok(());
        }
        let bytes = self.live_bytes() + need.iter().map(|n| u64::from(n.1)).sum::<u64>();
        if self
            .refused
            .is_some_and(|r| bytes >= r.bytes && self.log.free_count() <= r.free)
        {
            return Err(StoreError::NoSpace);
        }
        // Every live record (the mark just found them) plus the new ones.
        if !self.within_bound(&lens, need) {
            return Err(StoreError::NoSpace);
        }
        // Collect, a pass at a time: each sector the pass has not written,
        // garbage first (each such collection wins its garbage back), then
        // sectors whose only waste is a tail, every copy packed and every
        // head topped off before it closes (`gc_copy.rs`). A pass ends when
        // every sector has been collected once; another runs only while
        // passes still shrink what the closed sectors waste, so GC ends.
        // Last, a head's own garbage, which no victim reaches (a full hot
        // head is mostly old roots and hot directories): renew the head —
        // its live records copied to a new head of its kind, the old one
        // erased — when that lets the write open fewer sectors. Renewing
        // frees no sector; it gives the head its garbage back as room.
        let mut pass_from = self.log.next_sector_seq;
        let mut waste = self.waste();
        for _ in 0..self.log.sector_count * 4 {
            if self.log.free_count() == 0 {
                break;
            }
            let (victim, pack) = match choose_victim(&self.log, self.cfg.gc_policy, pass_from) {
                Some(s) => (s, true),
                None => match self
                    .renewal_helps(HeadKind::Cold, need)
                    .or_else(|| self.renewal_helps(HeadKind::Hot, need))
                {
                    Some(s) => (s, false),
                    None => {
                        let now = self.waste();
                        if now >= waste {
                            break;
                        }
                        (pass_from, waste) = (self.log.next_sector_seq, now);
                        continue;
                    }
                },
            };
            collect_sector(&mut self.log, victim, pack, &lens, pass_from)?;
            stat!(self.stats.gc_runs += 1);
            if self.enough(need) {
                return Ok(());
            }
        }
        self.refused = Some(Refusal {
            bytes,
            free: self.log.free_count(),
        });
        Err(StoreError::NoSpace)
    }

    /// The admission rule over the live bytes by kind (upper bounds between
    /// marks, exact right after one) and the new records.
    fn admitted(&self, need: &[(HeadKind, u32)]) -> bool {
        let mut bytes = [0u64; 2];
        for (s, &l) in self.log.sectors.live.iter().enumerate() {
            bytes[self.log.sectors.kind(s as u32).index()] += u64::from(l);
        }
        for &(head, len) in need {
            bytes[head.index()] += u64::from(len);
        }
        let usable = self.log.sector_count - self.log.sectors.retired.len() as u32;
        admits(bytes, self.log.sector_capacity(), usable, self.cfg.reserve)
    }

    fn live_bytes(&self) -> u64 {
        self.log.sectors.live.iter().map(|&l| u64::from(l)).sum()
    }

    /// The `kind` head, if it holds garbage and the write would open fewer
    /// sectors with that garbage as room (and a sector is free to renew it
    /// into).
    fn renewal_helps(&self, kind: HeadKind, need: &[(HeadKind, u32)]) -> Option<u32> {
        let s = self.log.heads[kind.index()]?;
        let live = u32::from(self.log.sectors.live[s as usize]);
        let used = u32::from(self.log.sectors.end[s as usize]) - SECTOR_HEADER_LEN;
        if used <= live || self.log.free_count() == 0 {
            return None;
        }
        let cap = self.log.sector_capacity();
        let mut remaining = [
            self.log.head_remaining(HeadKind::Cold),
            self.log.head_remaining(HeadKind::Hot),
        ];
        let now = sectors_needed(remaining, need.iter().copied(), cap);
        remaining[kind.index()] = cap - live;
        (sectors_needed(remaining, need.iter().copied(), cap) < now).then_some(s)
    }

    /// What the closed sectors hold that no write can use: everything not
    /// live in the written sectors but the heads (garbage and unused tails).
    pub(crate) fn waste(&self) -> u64 {
        let cap = self.log.sector_capacity();
        (0..self.log.sector_count)
            .filter(|&s| {
                self.log.sectors.is_written(s)
                    && !self.log.is_free(s)
                    && !self.log.is_head(s)
                    && !self.log.sectors.is_retired(s)
            })
            .map(|s| u64::from(cap.saturating_sub(u32::from(self.log.sectors.live[s as usize]))))
            .sum()
    }

    /// The packing bound over the live records' lengths (each tagged with
    /// its sector's kind) and the new ones.
    fn within_bound(&mut self, lens: &[u16], need: &[(HeadKind, u32)]) -> bool {
        let usable = self.log.sector_count - self.log.sectors.retired.len() as u32;
        let tag = |kind: HeadKind| if kind == HeadKind::Hot { HOT_LEN } else { 0 };
        let mut sizes: Vec<u16> = (0..lens.len())
            .map(|i| lens[i] | tag(self.log.sectors.kind(self.log.index.loc_at(i).sector)))
            .collect();
        sizes.extend(need.iter().map(|&(head, len)| len as u16 | tag(head)));
        self.log
            .note(lens.len() * 2 + sizes.capacity() * 2 + usable as usize * 4);
        fits_after_compaction(sizes, self.log.sector_capacity(), usable, self.cfg.reserve)
    }

    fn enough(&self, need: &[(HeadKind, u32)]) -> bool {
        let remaining = [
            self.log.head_remaining(HeadKind::Cold),
            self.log.head_remaining(HeadKind::Hot),
        ];
        let opened = sectors_needed(remaining, need.iter().copied(), self.log.sector_capacity());
        self.log.free_count() >= opened + self.cfg.reserve
    }

    /// Mark from everything that must survive and keep only that in the
    /// index (and make the sectors' live bytes exact); returns each index
    /// entry's record length when asked.
    pub(crate) fn mark_and_prune(&mut self, want_lens: bool) -> Res<Vec<u16>, F> {
        let mut roots: Vec<(ObjectId, MarkRole)> = Vec::new();
        if let Some(c) = &self.committed {
            roots.push((c.id, MarkRole::Root));
        }
        roots.push((self.work.cold, MarkRole::Dir));
        roots.push((self.work.hot, MarkRole::Dir));
        if !mutant!(GcForgetsPending) {
            roots.extend(self.delta.set_ids().map(|id| (id, MarkRole::Node)));
            roots.extend(self.inflight.iter().copied());
        }
        let mut m = mark(&mut self.log, &roots, want_lens)?;
        stat!(self.stats.marks += 1);
        prune(&mut self.log, &mut m);
        self.live_after_mark = self.log.index.len();
        Ok(m.lens)
    }
}
