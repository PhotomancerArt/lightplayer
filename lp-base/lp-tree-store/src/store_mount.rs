//! Mount: read every sector header (a good header with an unknown incompat
//! flag or head kind, or another sector size, refuses the mount:
//! `Unsupported`); scan every trusted sector's records in sector-sequence
//! order (pass 1, `record_scan.rs`: where each sector's trusted records end,
//! the newest two roots and each sector's id filter — nothing indexed); pick
//! the root by I1 (the newest CRC-good root whose closure is complete, else
//! the one before it — no further, README defect 12) by indexing its
//! closure (pass 2, `mount_walk.rs`: the index holds only live records, so
//! mount's RAM does not grow with the garbage on flash); resume each head
//! only if its tail reads all `0xFF`. The newest sector of each kind has its
//! tail settled during pass 1, before the walk or a resume trusts it
//! (`record_scan.rs`: a torn record can read whole on one read only).
//!
//! **Fast, then whole.** Mount's cost was every byte ever written: pass 1
//! read and CRC-checked every record in every trusted sector, though only
//! the last program in the newest sector of each kind can be torn (FORMAT.md
//! "Finding the committed state"). So it first reads the sectors no cut can
//! have reached by their header chains (roots read whole), checks only the
//! newest sector of each kind (and any a newer writer closed), and lets the
//! closure walk read every record it adopts whole — blobs too, so a mounted
//! store still reads back every file. Only when that adopts no root (a
//! blank or foreign partition, an interrupted format, damage) does it read
//! every record of every sector as before, and the verdict (`NoStore`,
//! `Damaged`) comes from that reading, so it is the same as it was. Both
//! readings agree on any flash a writer of this format leaves, power cuts
//! included; they differ only over damage no cut makes (a newer copy that
//! does not check over an older one that does: the fast pass gives up and
//! the whole one mounts it, `mount_walk.rs`'s tests and `lp-store-bench`'s
//! `shadowed_copy` fuzz images).

use alloc::vec::Vec;

use crate::flash::Flash;
use crate::heap_sort::heap_sort_by;
use crate::mount_verdict::FormatResidue;
use crate::mount_walk::{Walked, index_closure};
use crate::object_id::ObjectId;
use crate::object_hasher::ObjectHasher;
use crate::record_scan::{RootCandidates, scan_sector};
use crate::root_record::RootRecord;
use crate::sector_filter::SectorFilter;
use crate::sector_header::{HeadKind, SECTOR_HEADER_LEN, SectorHeader, SectorRead};
use crate::store_error::StoreError;
use crate::tree_store::{Committed, Res, TreeStore, WorkDirs};

impl<F: Flash, H: ObjectHasher> TreeStore<F, H> {
    pub(crate) fn load(&mut self) -> Res<(), F> {
        let n = self.log.sector_count;
        // The trusted sectors, then sorted by sector seq (2 B each: the seq
        // and the kind are in the sector table).
        let mut valid: Vec<u16> = Vec::new();
        // Closed to appends: a record failed to check, or an unknown compat flag.
        let mut closed = alloc::vec![false; n as usize];
        let mut residue = FormatResidue::default();
        for s in 0..n {
            let mut h = [0u8; SECTOR_HEADER_LEN as usize];
            self.log.read(self.log.addr(s, 0), &mut h)?;
            match SectorHeader::decode(&h, self.log.sector_size) {
                SectorRead::Untrusted => {}
                SectorRead::Unsupported(why) => return Err(StoreError::Unsupported(why)),
                SectorRead::Trusted { header, appendable } => {
                    residue.saw_trusted_sector();
                    valid.push(s as u16);
                    self.log.sectors.erase_count[s as usize] = header.erase_count;
                    self.log.sectors.seq[s as usize] = header.seq;
                    self.log.sectors.set_kind(s, header.kind);
                    closed[s as usize] = !appendable;
                }
            }
        }
        let seqs = &self.log.sectors.seq;
        heap_sort_by(&mut valid, |&a, &b| (seqs[a as usize], a) < (seqs[b as usize], b));
        // The newest sector of each kind: the only place a cut can have torn
        // the last program, and the head a resume would append to.
        let newest = HeadKind::ALL.map(|kind| {
            valid
                .iter()
                .rev()
                .map(|&s| u32::from(s))
                .find(|&s| self.log.sectors.kind(s) == kind)
                .filter(|&s| !closed[s as usize])
        });
        // Fast first: cold sectors by their header chains, then the closure
        // read whole (blobs too). Only when that adopts no root is every
        // record read and checked, as FORMAT.md's mount steps say, and the
        // verdict taken from that (`mount_verdict.rs`).
        //
        // "The newest sector of each kind" is the largest sector seq only
        // while the seqs in use have not wrapped: a store whose seqs span
        // half the counter or more may have wrapped, its newest sectors
        // carrying small seqs (`counter_wrap_tests.rs`), and is read whole.
        let seq_of = |log: &crate::record_log::RecordLog<F>, s: Option<&u16>| {
            s.map(|&s| log.sectors.seq[s as usize])
        };
        let newest_seq = seq_of(&self.log, valid.last());
        let wrapped = match (seq_of(&self.log, valid.first()), newest_seq) {
            (Some(a), Some(b)) => b - a >= 1 << 31,
            _ => false,
        };
        let attempts: &[bool] = if wrapped {
            &[true]
        } else if mutant!(NoFullScan) {
            &[false]
        } else {
            &[false, true]
        };
        let mut chosen = None;
        for &full in attempts {
            stat!(self.stats.mount_read_every_record = full);
            chosen = self.scan_and_walk(&valid, &mut closed, newest, &mut residue, full)?;
            if chosen.is_some() {
                break;
            }
        }
        let Some((id, walked)) = chosen else {
            return Err(residue.no_root());
        };
        self.log.next_sector_seq = newest_seq.map_or(1, |q| q.wrapping_add(1));
        self.log.note(valid.capacity() * 2 + closed.capacity() * 2);
        stat!(
            self.stats.marks += 1;
            self.stats.mount_scans = walked.scans;
        );
        self.log.sectors.live = walked.live;
        #[cfg(feature = "mutants")]
        if mutant!(DedupAgainstAllRecords) {
            crate::mutants::index_every_record(&mut self.log, &valid)?;
        }
        self.log.index.shrink();
        self.live_after_mark = self.log.index.len();
        // A closed sector is never appended to: its end is the sector size.
        for (s, &c) in closed.iter().enumerate() {
            if c {
                self.log.sectors.end[s] = self.log.sector_size as u16;
            }
        }
        let (_, payload) = self.log.read_record(id)?;
        let root = RootRecord::decode(&payload).ok_or(StoreError::Corrupt("root"))?;
        self.log.sectors.retired = root.retired.clone();
        self.work = WorkDirs {
            cold: root.cold_dir,
            hot: root.hot_dir,
        };
        self.committed = Some(Committed { id, root });

        for kind in HeadKind::ALL {
            let cand = newest[kind.index()]
                .filter(|&s| !closed[s as usize] && !self.log.sectors.is_retired(s));
            if let Some(s) = cand {
                let end = u32::from(self.log.sectors.end[s as usize]);
                if mutant!(ResumeWithoutTailCheck) || self.log.reads_erased(s, end)? {
                    self.log.heads[kind.index()] = Some(s);
                }
            }
        }
        stat!(self.stats.mount_bytes_read = self.log.counters.bytes_read);
        Ok(())
    }

    /// Pass 1 over the trusted sectors (`valid`, ascending sector seq) and
    /// pass 2 from the newest root, else the one before it. `full`: every
    /// sector's records are read and checked whole; otherwise only the
    /// sectors a cut can have torn (the newest of each kind, settled) and
    /// those a newer writer closed, the rest by their header chains with
    /// only roots read — the closure walk then reads every record it
    /// adopts whole, blobs included. `closed` comes in as the headers left
    /// it and goes out with the scan's closes added.
    fn scan_and_walk(
        &mut self,
        valid: &[u16],
        closed: &mut [bool],
        newest: [Option<u32>; 2],
        residue: &mut FormatResidue,
        full: bool,
    ) -> Res<Option<(ObjectId, Walked)>, F> {
        let mut roots = RootCandidates::default();
        let mut scan_closed = alloc::vec![false; closed.len()];
        let mut filter = SectorFilter::new(self.log.sector_count);
        for s in valid.iter().map(|&s| u32::from(s)) {
            // `newest` leaves out a closed newest sector: checked as closed.
            let settle = newest.contains(&Some(s));
            let check = full || closed[s as usize] || (settle && !mutant!(NewestByHeaders));
            let scan = scan_sector(
                &mut self.log,
                s,
                &mut roots,
                residue,
                &mut filter,
                check,
                settle,
            )?;
            self.log.sectors.end[s as usize] = scan.end as u16;
            scan_closed[s as usize] = scan.closed;
        }
        self.max_root_seq = roots.max_seq;
        self.log.note(filter.ram_bytes());
        #[cfg(test)]
        crate::mount_cost_tests::PASS1.with(|p| {
            p.set((self.log.counters.read_calls, self.log.counters.bytes_read))
        });
        // The fast reading adopts the newest root or nothing: a closure it
        // cannot complete may be one the whole reading completes (a damaged
        // newer copy over a good older one), and falling back a root on it
        // would roll a store back that is whole (`shadowed_copy`). The one
        // step back I1 allows is the whole reading's.
        let tries = if full { 2 } else { 1 };
        for c in roots.best.iter().flatten().take(tries) {
            match index_closure(&mut self.log, *c, valid, &filter) {
                Ok(w) => {
                    for (c, s) in closed.iter_mut().zip(scan_closed) {
                        *c |= s;
                    }
                    return Ok(Some((c.id, w)));
                }
                Err(StoreError::Corrupt(_)) => continue,
                Err(e) => return Err(e),
            }
        }
        Ok(None)
    }
}
