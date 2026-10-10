//! Making room before a write: the cheap check (live upper bounds), then a
//! full mark that makes them exact and prunes the index, then the packing
//! bound (`NoSpace` before anything is written), then GC until it fits,
//! and last a renewed head.
//!
//! This is the only place GC runs, and it runs only at the start of a write
//! phase, when everything already written is reachable from the mark's
//! roots: the committed root, the working tree, and the delta's file ids.

use alloc::vec::Vec;

use crate::flash::Flash;
use crate::gc_copy::collect_sector;
use crate::gc_mark::{MarkRole, mark, prune};
use crate::gc_victim::{Victim, choose_victim};
use crate::object_hasher::ObjectHasher;
use crate::object_id::ObjectId;
use crate::sector_header::{HeadKind, SECTOR_HEADER_LEN};
use crate::space_estimate::{fits_after_compaction, sectors_needed};
use crate::store_error::StoreError;
use crate::tree_store::{Res, TreeStore};

impl<F: Flash, H: ObjectHasher> TreeStore<F, H> {
    /// Make the free sectors cover `need` (records in append order) plus
    /// the reserve. Also the index's bound: between marks it holds every
    /// record written since, so once it has grown an eighth (+ 16) past the
    /// live set of the last mark, mark now and prune it back.
    pub(crate) fn ensure_room(&mut self, need: &[(HeadKind, u32)]) -> Res<(), F> {
        let live = self.live_after_mark;
        if self.log.index.len() > live + live / 8 + 16 {
            self.mark_and_prune()?;
        }
        if self.enough(need) {
            return Ok(());
        }
        self.mark_and_prune()?;
        if self.enough(need) {
            return Ok(());
        }
        // Exact live bytes (the mark just made them so) plus the new records.
        if !self.within_bound(need) {
            return Err(StoreError::NoSpace);
        }
        // Collect while a victim holds garbage: each such collection wins
        // its garbage back, so the total only falls and the run ends, even
        // when no single one frees a sector (small garbage spread over many
        // sectors frees one only once enough of it is collected). Tail-only
        // compaction can churn without gaining a sector when records are
        // near `record_max`, so only a run of those that frees nothing past
        // the best so far stops it. Last, a head's own garbage, which no
        // victim reaches (a full hot head is mostly old roots and hot
        // directories): renew the head — its live records copied to a new
        // head of its kind, the old one erased — when that lets the write
        // open fewer sectors. Renewing frees no sector; it gives the head its
        // garbage back as room.
        let mut best_free = self.log.free_count();
        let mut stalls = 0;
        for _ in 0..self.log.sector_count * 4 {
            if self.log.free_count() == 0 {
                break;
            }
            let victim = match choose_victim(&self.log, self.cfg.gc_policy)
                .filter(|_| stalls <= self.cfg.reserve + 2)
            {
                Some(v) => v,
                None => match self
                    .renewal_helps(HeadKind::Cold, need)
                    .or_else(|| self.renewal_helps(HeadKind::Hot, need))
                {
                    Some(sector) => Victim {
                        sector,
                        has_garbage: true,
                    },
                    None => break,
                },
            };
            collect_sector(&mut self.log, victim.sector)?;
            stat!(self.stats.gc_runs += 1);
            if self.enough(need) {
                return Ok(());
            }
            let free = self.log.free_count();
            if free > best_free {
                best_free = free;
                stalls = 0;
            } else if !victim.has_garbage {
                stalls += 1;
            }
        }
        Err(StoreError::NoSpace)
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

    /// The packing bound over the sectors' live bytes and the new records.
    fn within_bound(&self, need: &[(HeadKind, u32)]) -> bool {
        let usable = self.log.sector_count - self.log.sectors.retired.len() as u32;
        let live: u64 = self.log.sectors.live.iter().map(|&l| u64::from(l)).sum();
        let bytes = live + need.iter().map(|n| u64::from(n.1)).sum::<u64>();
        fits_after_compaction(bytes, self.log.sector_capacity(), usable, self.cfg.reserve)
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
    /// index (and make the sectors' live bytes exact).
    pub(crate) fn mark_and_prune(&mut self) -> Res<(), F> {
        let mut roots: Vec<(ObjectId, MarkRole)> = Vec::new();
        if let Some(c) = &self.committed {
            roots.push((c.id, MarkRole::Root));
        }
        roots.push((self.work.cold, MarkRole::Dir));
        roots.push((self.work.hot, MarkRole::Dir));
        roots.extend(self.delta.set_ids().map(|id| (id, MarkRole::Node)));
        roots.extend(self.inflight.iter().copied());
        let m = mark(&mut self.log, &roots)?;
        stat!(self.stats.marks += 1);
        prune(&mut self.log, m);
        self.live_after_mark = self.log.index.len();
        Ok(())
    }
}
