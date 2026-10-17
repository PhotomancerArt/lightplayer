//! Cleaning up while nothing is asked of the store (`collect_step`): the
//! GC `ensure_room` would otherwise run in front of a write, done one
//! bounded piece at a time, before it is needed.
//!
//! Each step marks (the live bytes exact, the index pruned: the same mark a
//! write phase makes), then does at most one thing:
//!
//! 1. **Forget a garbage-only sector**: a trusted sector no live record is
//!    in has its header killed (24 bytes, no erase). It was already free;
//!    now a mount reads its header and nothing else (a trusted sector costs
//!    a mount every record in it), and opening it erases it as before. An
//!    erase here would be repeated after the next mount anyway: a mount
//!    never trusts a sector to be erased.
//! 2. **Collect a sector with garbage** while fewer sectors are free than
//!    the low water (`StoreConfig::idle_free_percent` of the usable
//!    sectors): one holding at least `idle_min_garbage` bytes of garbage,
//!    by the store's GC policy, collected as GC does (copies read back
//!    before the erase, I2). Each such collection wins garbage back.
//! 3. **Pack, near full**: while fewer than `reserve + idle_pack_free`
//!    sectors are free, the victim GC itself would take next — garbage
//!    first, then a sector whose only waste is its tail — a pass at a
//!    time, each sector at most once a pass, another pass only while the
//!    last one shrank what the closed sectors waste (GC's own rule, so the
//!    steps end). Near full a write's GC re-packs most of the store in
//!    front of it (seconds on a C6); done here, the write finds its room.
//!    A pass rewrites most of the store, so a new one starts only once the
//!    waste has grown a sector's capacity past what the last packing left:
//!    room for a sector to be won. Without that, every few hundred bytes of
//!    garbage (a panel save) bought a whole re-pack — 170 erases a write,
//!    against the write path's one in three (the near-full wear test). A
//!    write does not restart a pass under way.
//!
//! When a step finds nothing to do, the store is **settled**: later steps
//! return at once, with no mark, until the next write. No step runs inside
//! a transaction (`begin` … `commit`), so the mark's roots are the
//! committed tree alone.

use crate::flash::Flash;
use crate::gc_copy::collect_sector;
use crate::gc_victim::{choose_idle_victim, choose_victim};
use crate::object_hasher::ObjectHasher;
use crate::tree_store::{Res, TreeStore, Txn};

impl<F: Flash, H: ObjectHasher> TreeStore<F, H> {
    /// One bounded piece of cleanup, for a caller with nothing else to do
    /// (the server's quiet ticks): a mark, then at most one garbage-only
    /// sector forgotten or one victim collected (module docs). `true` if it
    /// changed the flash; `false` inside a transaction, and once the store
    /// is settled (until the next write). Cut anywhere, the store reads the
    /// state it had: a step changes no file.
    pub fn collect_step(&mut self) -> Res<bool, F> {
        if self.txn != Txn::None || self.idle_settled {
            return Ok(false);
        }
        let lens = self.mark_and_prune(true)?;
        if let Some(s) = self.garbage_only_sector() {
            self.log.forget_sector(s)?;
            stat!(self.stats.idle_forgets += 1);
            return Ok(true);
        }
        let free = self.log.free_count();
        let victim = if free < self.idle_free_target() {
            choose_idle_victim(&self.log, self.cfg.gc_policy, self.cfg.idle_min_garbage)
        } else {
            None
        };
        let victim = match victim {
            Some(v) => Some((v, self.log.next_sector_seq)),
            None if free > 0 && free < self.cfg.reserve + self.cfg.idle_pack_free => {
                self.pack_victim()
            }
            None => None,
        };
        let Some((victim, pass_from)) = victim else {
            self.idle_settled = true;
            self.idle_pass = None;
            return Ok(false);
        };
        collect_sector(&mut self.log, victim, true, &lens, pass_from)?;
        stat!(
            self.stats.gc_runs += 1;
            self.stats.idle_collections += 1;
        );
        Ok(true)
    }

    /// A write (or a commit) may have made garbage: idle steps look again.
    /// A packing pass under way goes on (sectors the write opened are past
    /// its first sequence, so it does not re-pack them).
    pub(crate) fn wake_idle(&mut self) {
        self.idle_settled = false;
    }

    /// The packing pass's next victim and the pass's first sequence: GC's
    /// own choice within the pass under way, or the first of a new pass
    /// when the last one shrank the waste; `None` when it did not.
    fn pack_victim(&mut self) -> Option<(u32, u32)> {
        let (pass_from, waste) = match self.idle_pass {
            Some(p) => p,
            None => {
                let now = self.waste();
                if let Some(floor) = self.idle_packed_waste
                    && now < floor + u64::from(self.log.sector_capacity())
                {
                    self.idle_packed_waste = Some(floor.min(now));
                    return None;
                }
                *self.idle_pass.insert((self.log.next_sector_seq, now))
            }
        };
        if let Some(v) = choose_victim(&self.log, self.cfg.gc_policy, pass_from) {
            return Some((v, pass_from));
        }
        let now = self.waste();
        if now >= waste {
            self.idle_pass = None;
            self.idle_packed_waste = Some(now);
            return None;
        }
        let pass_from = self.log.next_sector_seq;
        self.idle_pass = Some((pass_from, now));
        choose_victim(&self.log, self.cfg.gc_policy, pass_from).map(|v| (v, pass_from))
    }

    /// A trusted sector holding no live record (by the exact live bytes a
    /// mark just made): not a head, not retired, not GC's victim.
    fn garbage_only_sector(&self) -> Option<u32> {
        (0..self.log.sector_count).find(|&s| {
            self.log.sectors.is_written(s)
                && self.log.sectors.live[s as usize] == 0
                && (mutant!(IdleForgetsHeads) || !self.log.is_head(s))
                && !self.log.sectors.is_retired(s)
                && self.log.gc_victim != Some(s)
        })
    }

    /// Sectors an idle step keeps free when garbage allows.
    fn idle_free_target(&self) -> u32 {
        let usable = self.log.sector_count - self.log.sectors.retired.len() as u32;
        (usable * self.cfg.idle_free_percent.min(100)).div_ceil(100)
    }
}
