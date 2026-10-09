//! Mount: read every sector header (a good header with an unknown incompat
//! flag or head kind, or another sector size, refuses the mount:
//! `Unsupported`); check every trusted sector's records in
//! sector-sequence order (pass 1, `record_scan.rs`: where each sector's
//! trusted records end, and the newest two roots — nothing indexed); pick
//! the root by I1 (the newest CRC-good root whose closure is complete, else
//! the one before it — no further, README defect 12) by indexing its
//! closure (pass 2, `mount_walk.rs`: the index holds only live records, so
//! mount's RAM does not grow with the garbage on flash); resume each head
//! only if its tail reads all `0xFF`.

use alloc::vec::Vec;

use crate::flash::Flash;
use crate::heap_sort::heap_sort_by;
use crate::mount_walk::index_closure;
use crate::object_hasher::ObjectHasher;
use crate::record_scan::{RootCandidates, scan_sector};
use crate::root_record::RootRecord;
use crate::sector_header::{HeadKind, SECTOR_HEADER_LEN, SectorHeader, SectorRead};
use crate::store_error::StoreError;
use crate::tree_store::{Committed, Res, TreeStore, WorkDirs};

impl<F: Flash, H: ObjectHasher> TreeStore<F, H> {
    pub(crate) fn load(&mut self) -> Res<(), F> {
        let n = self.log.sector_count;
        let mut valid: Vec<(u32, u32, HeadKind)> = Vec::new();
        // Closed to appends: a record failed to check, or an unknown compat flag.
        let mut closed = alloc::vec![false; n as usize];
        for s in 0..n {
            let mut h = [0u8; SECTOR_HEADER_LEN as usize];
            self.log.read(self.log.addr(s, 0), &mut h)?;
            match SectorHeader::decode(&h, self.log.sector_size) {
                SectorRead::Untrusted => {}
                SectorRead::Unsupported(why) => return Err(StoreError::Unsupported(why)),
                SectorRead::Trusted { header, appendable } => {
                    valid.push((header.seq, s, header.kind));
                    self.log.sectors.erase_count[s as usize] = header.erase_count;
                    self.log.sectors.seq[s as usize] = header.seq;
                    closed[s as usize] = !appendable;
                }
            }
        }
        heap_sort_by(&mut valid, |a, b| (a.0, a.1) < (b.0, b.1));
        // Pass 1: every trusted record, CRC-checked; roots kept apart.
        let mut roots = RootCandidates::default();
        for &(_, s, _) in &valid {
            let scan = scan_sector(&mut self.log, s, &mut roots)?;
            self.log.sectors.end[s as usize] = scan.end as u16;
            closed[s as usize] |= scan.closed;
        }
        self.max_root_seq = roots.max_seq;
        self.log.next_sector_seq = valid.last().map_or(1, |v| v.0.wrapping_add(1));

        // Pass 2: index the newest root's closure, else the one before it.
        self.log.note(valid.capacity() * 12 + closed.capacity());
        let mut chosen = None;
        for c in roots.best.iter().flatten() {
            match index_closure(&mut self.log, *c, &valid) {
                Ok(w) => {
                    chosen = Some((c.id, w));
                    break;
                }
                Err(StoreError::Corrupt(_)) => continue,
                Err(e) => return Err(e),
            }
        }
        let Some((id, walked)) = chosen else {
            return Err(StoreError::Corrupt("no complete root"));
        };
        stat!(
            self.stats.marks += 1;
            self.stats.mount_scans = walked.scans;
        );
        self.log.sectors.live = walked.live;
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
            let cand = valid
                .iter()
                .rev()
                .find(|v| v.2 == kind)
                .map(|v| v.1)
                .filter(|&s| !closed[s as usize] && !self.log.sectors.is_retired(s));
            if let Some(s) = cand {
                let end = u32::from(self.log.sectors.end[s as usize]);
                if self.log.reads_erased(s, end)? {
                    self.log.heads[kind.index()] = Some(s);
                }
            }
        }
        stat!(self.stats.mount_bytes_read = self.log.counters.bytes_read);
        Ok(())
    }
}
