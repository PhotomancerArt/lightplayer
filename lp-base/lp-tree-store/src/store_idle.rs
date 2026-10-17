//! Cleaning up while nothing is asked of the store (`collect_step`): the
//! GC `ensure_room` would otherwise run in front of a write, done one
//! small piece at a time, before it is needed.
//!
//! A step does **one piece** — one of these, the first that applies — so
//! that each costs about one erase (a C6's ~19 ms), whatever the store
//! holds:
//!
//! 1. **Mark**, when no mark is held: the live bytes made exact and the
//!    index pruned (the same mark a write phase makes), and each live
//!    record's length kept. It is kept across steps until the next write
//!    (any call that can write, any mark: `wake_idle`, `mark_and_prune`):
//!    between writes the steps only move records, which keeps the index's
//!    positions, the lengths and the live bytes exact. A write drops it.
//! 2. **Erase the victim the last steps emptied**: its live records are all
//!    copied out, so it holds only garbage; it is killed and erased as GC
//!    erases a victim, and a write finds it erased.
//! 3. **Forget a garbage-only sector**: a trusted sector no live record is
//!    in has its header killed (24 bytes, no erase). It was already free;
//!    now a mount reads its header and nothing else (a trusted sector costs
//!    a mount every record in it), and opening it erases it as before. An
//!    erase here would be repeated after the next mount anyway: a mount
//!    never trusts a sector to be erased.
//! 4. **Copy out of a victim**, at most `StoreConfig::idle_copy_bytes`
//!    (the record that crosses it finishes), the victim kept until it is
//!    empty. A victim is chosen when none is under way:
//!    - while fewer sectors are free than the low water
//!      (`StoreConfig::idle_free_percent` of the usable sectors): one
//!      holding at least `idle_min_garbage` bytes of garbage, by the store's
//!      GC policy — each such collection wins garbage back;
//!    - near full, while fewer than `reserve + idle_pack_free` sectors are
//!      free, **pack**: the victim GC itself would take next — garbage
//!      first, then a sector whose only waste is its tail — a pass at a
//!      time, each sector at most once a pass, another pass only while the
//!      last one shrank what the closed sectors waste (GC's own rule, so the
//!      steps end). Near full a write's GC re-packs most of the store in
//!      front of it (seconds on a C6); done here, the write finds its room.
//!      A pass rewrites most of the store, so a new one starts only once the
//!      waste has grown a sector's capacity past what the last packing left:
//!      room for a sector to be won. Without that, every few hundred bytes
//!      of garbage (a panel save) bought a whole re-pack — 170 erases a
//!      write, against the write path's one in three (the near-full wear
//!      test). A write does not restart a pass under way.
//!
//! A copy is GC's own (read back before the next one; I2), and the victim
//! is erased only once every live record in it has a copy. Between the
//! pieces the store is in a state a cut inside GC already leaves — records
//! with a second, older copy in the victim — and a write may come between
//! them: it drops the victim under way (its copies so far stand; what is
//! left of it stays live where it is) and the emptied one (a later step
//! forgets it).
//!
//! When a step finds nothing to do, the store is **settled**: later steps
//! return at once, with no mark, until the next write. No step runs inside
//! a transaction (`begin` … `commit`), so the mark's roots are the
//! committed tree alone.

use crate::flash::Flash;
use crate::gc_copy::copy_out;
use crate::gc_victim::{choose_idle_victim, choose_victim};
use crate::object_hasher::ObjectHasher;
use crate::tree_store::{Res, TreeStore, Txn};

impl<F: Flash, H: ObjectHasher> TreeStore<F, H> {
    /// One small piece of cleanup, for a caller with nothing else to do
    /// (the server's quiet ticks, paced by `lpfs::IdlePacer`): a mark, an
    /// erase, a forget or a victim's next copies (module docs). `true` if it
    /// did one; `false` inside a transaction, and once the store is settled
    /// (until the next write). Cut anywhere, the store reads the state it
    /// had: a step changes no file.
    pub fn collect_step(&mut self) -> Res<bool, F> {
        if self.txn != Txn::None || self.idle_settled {
            return Ok(false);
        }
        let Some(lens) = self.idle_lens.take() else {
            self.idle_lens = Some(self.mark_and_prune(true)?);
            return Ok(true);
        };
        let did = self.collect_piece(&lens);
        if !self.idle_settled {
            self.idle_lens = Some(lens);
        }
        did
    }

    /// A step's piece after the mark (`lens`: the held mark's lengths).
    fn collect_piece(&mut self, lens: &[u16]) -> Res<bool, F> {
        if let Some(s) = self.idle_emptied.take()
            && (self.garbage_only(s) || mutant!(IdleErasesBeforeEmpty))
        {
            self.log.kill_and_erase(s)?;
            stat!(
                self.stats.gc_runs += 1;
                self.stats.idle_collections += 1;
            );
            return Ok(true);
        }
        if let Some(s) = self.garbage_only_sector() {
            self.log.forget_sector(s)?;
            stat!(self.stats.idle_forgets += 1);
            return Ok(true);
        }
        let Some((victim, pass_from)) = self.idle_victim.take().or_else(|| self.next_victim())
        else {
            self.idle_settled = true;
            self.idle_pass = None;
            return Ok(false);
        };
        let budget = self.cfg.idle_copy_bytes.max(1);
        if copy_out(&mut self.log, victim, true, lens, pass_from, budget)?
            || mutant!(IdleErasesBeforeEmpty)
        {
            self.idle_emptied = Some(victim);
        } else {
            self.idle_victim = Some((victim, pass_from));
        }
        Ok(true)
    }

    /// A write (or a commit, or a mark) may have made garbage and moved the
    /// index: idle steps look again, from a new mark. The victim under way
    /// and the emptied one are dropped (a later step forgets the emptied
    /// one). A packing pass under way goes on (sectors the write opened are
    /// past its first sequence, so it does not re-pack them).
    pub(crate) fn wake_idle(&mut self) {
        self.idle_settled = false;
        if !mutant!(IdleKeepsMarkAcrossWrites) {
            self.idle_lens = None;
            self.idle_victim = None;
            self.idle_emptied = None;
        }
    }

    /// The next victim, and the first sequence of its pass: below the low
    /// water one with garbage, near full the packing pass's (module docs).
    fn next_victim(&mut self) -> Option<(u32, u32)> {
        let free = self.log.free_count();
        if free < self.idle_free_target()
            && let Some(v) =
                choose_idle_victim(&self.log, self.cfg.gc_policy, self.cfg.idle_min_garbage)
        {
            return Some((v, self.log.next_sector_seq));
        }
        if free > 0 && free < self.cfg.reserve + self.cfg.idle_pack_free {
            return self.pack_victim();
        }
        None
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
    /// mark made, kept exact by the steps since).
    fn garbage_only_sector(&self) -> Option<u32> {
        (0..self.log.sector_count).find(|&s| self.garbage_only(s))
    }

    /// `s` is written, holds no live byte, and is not a head, retired, or
    /// GC's victim.
    fn garbage_only(&self, s: u32) -> bool {
        self.log.sectors.is_written(s)
            && self.log.sectors.live[s as usize] == 0
            && (mutant!(IdleForgetsHeads) || !self.log.is_head(s))
            && !self.log.sectors.is_retired(s)
            && self.log.gc_victim != Some(s)
    }

    /// Sectors an idle step keeps free when garbage allows.
    fn idle_free_target(&self) -> u32 {
        let usable = self.log.sector_count - self.log.sectors.retired.len() as u32;
        (usable * self.cfg.idle_free_percent.min(100)).div_ceil(100)
    }
}
