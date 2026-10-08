//! Mount: read every sector header; scan every valid sector's records in
//! sector-sequence order into the index; pick the root by I1 (the newest
//! CRC-good root whose closure is complete, else the one before it — no
//! further, README defect 12); mark from it and prune the index to the live
//! set; walk the tree to build the path table; resume each head only if
//! its tail reads all `0xFF`.

use alloc::string::String;
use alloc::vec::Vec;

use crate::dir_node::EntryKind;
use crate::flash::Flash;
use crate::gc_mark::{MarkRole, mark, prune};
use crate::heap_sort::heap_sort_by;
use crate::node_read::read_dir;
use crate::object_hasher::ObjectHasher;
use crate::object_id::{ObjectId, path_hash};
use crate::record_scan::{RootCandidates, scan_sector};
use crate::root_record::RootRecord;
use crate::sector_header::{HeadKind, SECTOR_HEADER_LEN, SectorHeader};
use crate::store_error::StoreError;
use crate::tree_store::{Committed, MAX_DEPTH, Res, TreeStore, WorkDirs, is_hot, valid_path};

impl<F: Flash, H: ObjectHasher> TreeStore<F, H> {
    pub(crate) fn load(&mut self) -> Res<(), F> {
        let n = self.log.sector_count;
        let mut valid: Vec<(u32, u32, HeadKind)> = Vec::new();
        for s in 0..n {
            let mut h = [0u8; SECTOR_HEADER_LEN as usize];
            self.log.read(self.log.addr(s, 0), &mut h)?;
            if let Some(hd) = SectorHeader::decode(&h) {
                valid.push((hd.seq, s, hd.kind));
                self.log.sectors.erase_count[s as usize] = hd.erase_count;
                self.log.sectors.seq[s as usize] = hd.seq;
            }
        }
        heap_sort_by(&mut valid, |a, b| (a.0, a.1) < (b.0, b.1));
        let mut roots = RootCandidates::default();
        let mut closed = alloc::vec![false; n as usize];
        for &(_, s, _) in &valid {
            let scan = scan_sector(&mut self.log, s, &mut roots)?;
            self.log.sectors.end[s as usize] = scan.end as u16;
            closed[s as usize] = scan.closed;
        }
        for c in roots.best.iter().flatten() {
            self.log.index.push_unsorted(c.id, c.loc);
        }
        let seqs = &self.log.sectors.seq;
        let mut index = core::mem::take(&mut self.log.index);
        index.sort_dedup(|s| seqs[s as usize]);
        self.log.index = index;
        self.log.note(self.log.index.ram_bytes() + valid.capacity() * 12);
        self.max_root_seq = roots.max_seq;
        self.log.next_sector_seq = valid.last().map_or(1, |v| v.0.wrapping_add(1));

        let mut chosen = None;
        for c in roots.best.iter().flatten() {
            match mark(&mut self.log, &[(c.id, MarkRole::Root)], false) {
                Ok(m) => {
                    chosen = Some((c.id, m));
                    break;
                }
                Err(StoreError::Corrupt(_)) => continue,
                Err(e) => return Err(e),
            }
        }
        let Some((id, m)) = chosen else {
            return Err(StoreError::Corrupt("no complete root"));
        };
        self.stats.marks += 1;
        prune(&mut self.log, m);
        let (_, payload) = self.log.read_record(id)?;
        let root = RootRecord::decode(&payload).ok_or(StoreError::Corrupt("root"))?;
        self.log.sectors.retired = root.retired.clone();
        self.work = WorkDirs {
            cold: root.cold_dir,
            hot: root.hot_dir,
        };
        self.committed = Some(Committed { id, root });
        self.build_path_table()?;

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
        self.stats.mount_bytes_read = self.log.counters.bytes_read;
        Ok(())
    }

    /// Hash every path in the tree into the path table.
    fn build_path_table(&mut self) -> Res<(), F> {
        let mut path = String::new();
        self.table_walk(self.work.cold, &mut path, 0)?;
        for e in read_dir(&mut self.log, self.work.hot)? {
            if e.kind != EntryKind::File || !valid_path(&e.name) || !is_hot(&e.name) {
                return Err(StoreError::Corrupt("hot dir entry"));
            }
            let h = path_hash(&mut self.hasher, &e.name);
            self.table.push_unsorted(h, e.id, e.size);
        }
        self.table.finish_build();
        Ok(())
    }

    fn table_walk(&mut self, id: ObjectId, path: &mut String, depth: usize) -> Res<(), F> {
        if depth > MAX_DEPTH {
            return Err(StoreError::Corrupt("dir depth"));
        }
        for e in read_dir(&mut self.log, id)? {
            if e.name.is_empty() || e.name.contains('/') {
                return Err(StoreError::Corrupt("dir entry name"));
            }
            let len = path.len();
            path.push('/');
            path.push_str(&e.name);
            match e.kind {
                EntryKind::File => {
                    let h = path_hash(&mut self.hasher, path);
                    self.table.push_unsorted(h, e.id, e.size);
                }
                EntryKind::Dir => self.table_walk(e.id, path, depth + 1)?,
            }
            path.truncate(len);
        }
        Ok(())
    }
}
