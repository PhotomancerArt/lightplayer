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
    /// the reserve.
    pub(crate) fn ensure_room(&mut self, need: &[(HeadKind, u32)]) -> Res<(), F> {
        if self.enough(need) {
            return Ok(());
        }
        let lens = self.mark_and_prune(true)?;
        if self.enough(need) {
            return Ok(());
        }
        let usable = self.log.sector_count - self.log.sectors.retired.len() as u32;
        let mut sizes = lens;
        sizes.extend(need.iter().map(|n| n.1 as u16));
        self.log.note(sizes.capacity() * 2);
        if !fits_after_compaction(sizes, self.log.sector_capacity(), usable, self.cfg.reserve) {
            return Err(StoreError::NoSpace);
        }
        for _ in 0..self.log.sector_count * 4 {
            if self.log.free_count() == 0 {
                break;
            }
            let Some(victim) = choose_victim(&self.log, self.cfg.gc_policy) else {
                break;
            };
            let copied = collect_sector(&mut self.log, victim)?;
            self.stats.gc_runs += 1;
            self.stats.gc_copies += copied.records;
            self.stats.gc_copy_bytes += copied.bytes;
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
    /// index; returns the live records' lengths when asked.
    pub(crate) fn mark_and_prune(&mut self, want_lens: bool) -> Res<Vec<u16>, F> {
        let mut roots: Vec<(ObjectId, MarkRole)> = Vec::new();
        if let Some(c) = &self.committed {
            roots.push((c.id, MarkRole::Root));
        }
        roots.push((self.work.cold, MarkRole::Dir));
        roots.push((self.work.hot, MarkRole::Dir));
        roots.extend(self.delta.set_ids().map(|id| (id, MarkRole::Node)));
        let mut m = mark(&mut self.log, &roots, want_lens)?;
        self.stats.marks += 1;
        let lens = core::mem::take(&mut m.lens);
        prune(&mut self.log, m);
        Ok(lens)
    }
}
