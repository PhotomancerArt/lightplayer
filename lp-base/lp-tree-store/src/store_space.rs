//! Making room before a write: the cheap check (live upper bounds), then a
//! full mark that makes them exact and prunes the index, then the packing
//! bound (`NoSpace` before anything is written), then GC until it fits.
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
use crate::sector_header::HeadKind;
use crate::space_estimate::{fits_after_compaction, sectors_needed};
use crate::store_error::StoreError;
use crate::tree_store::{Res, TreeStore};

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
            self.mark_and_prune()?;
        }
        if self.enough(need) {
            return Ok(());
        }
        self.mark_and_prune()?;
        if self.enough(need) {
            return Ok(());
        }
        let usable = self.log.sector_count - self.log.sectors.retired.len() as u32;
        // Exact live bytes (the mark just made them so) plus the new records.
        let live: u64 = self.log.sectors.live.iter().map(|&l| u64::from(l)).sum();
        let bytes = live + need.iter().map(|n| u64::from(n.1)).sum::<u64>();
        if !fits_after_compaction(bytes, self.log.sector_capacity(), usable, self.cfg.reserve) {
            return Err(StoreError::NoSpace);
        }
        // Stop when collections stop freeing sectors (tail-only compaction
        // of near-`record_max` records can churn without gaining any).
        let mut best_free = self.log.free_count();
        let mut stalls = 0;
        for _ in 0..self.log.sector_count * 4 {
            let free = self.log.free_count();
            if free == 0 {
                break;
            }
            if free > best_free {
                best_free = free;
                stalls = 0;
            } else {
                stalls += 1;
                if stalls > self.cfg.reserve + 2 {
                    break;
                }
            }
            let Some(victim) = choose_victim(&self.log, self.cfg.gc_policy) else {
                break;
            };
            collect_sector(&mut self.log, victim)?;
            stat!(self.stats.gc_runs += 1);
            if self.enough(need) {
                return Ok(());
            }
        }
        Err(StoreError::NoSpace)
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
        if !mutant!(GcForgetsPending) {
            roots.extend(self.delta.set_ids().map(|id| (id, MarkRole::Node)));
            roots.extend(self.inflight.iter().copied());
        }
        let m = mark(&mut self.log, &roots)?;
        stat!(self.stats.marks += 1);
        prune(&mut self.log, m);
        self.live_after_mark = self.log.index.len();
        Ok(())
    }
}
