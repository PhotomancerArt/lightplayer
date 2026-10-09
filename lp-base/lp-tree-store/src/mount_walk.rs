//! Mount's second pass: index only the chosen root's closure, so mount's RAM
//! is bounded by the live set, not by the garbage on flash.
//!
//! The walk is a mark ([`visit`], the step GC's mark uses) that has no index
//! to look ids up in. It goes level by level: the ids the last level named
//! are looked for by one scan of record **headers** over every trusted
//! sector, newest sector first, and the first copy seen is the one kept —
//! the copy in the sector with the highest sector seq (FORMAT.md "Record",
//! mount step 2). Then each found record is visited, and what it names is
//! the next level. An id no trusted sector holds makes the closure
//! incomplete (`Corrupt`), as a missing record does in a mark.
//!
//! The first pass (`record_scan.rs`) CRC-checked every record and set each
//! sector's `end` to just past its last trusted record; this pass reads
//! headers only inside `[SECTOR_HEADER_LEN, end)`, and stops a sector at a
//! header that no longer parses (a re-read that differs: the walk never
//! trusts more than the first pass did). Records that name others are read
//! whole and CRC-checked again by the visit, as in a mark.
//!
//! A directory too big for one record is a flagged multi: its entries are
//! read once every chunk under it is indexed, a level or two later.
//!
//! Cost: one header scan per level of the tree (16 B per record on flash
//! per level, stopping early once every wanted id is found), for RAM of the
//! live index plus the two widest adjacent levels (16 B per id).

use alloc::vec::Vec;

use crate::flash::Flash;
use crate::gc_mark::{MarkRole, push_dir_node_entries, visit};
use crate::multi_node::{multi_child, parse_multi};
use crate::object_id::ObjectId;
use crate::ram_index::{RamIndex, RecordLoc};
use crate::record_header::{HeaderRead, RECORD_HEADER_LEN, RecordHeader};
use crate::record_kind::RecordKind;
use crate::record_log::RecordLog;
use crate::record_scan::RootCandidate;
use crate::sector_header::SECTOR_HEADER_LEN;
use crate::store_error::StoreError;

type R<T, F> = Result<T, StoreError<<F as Flash>::Error>>;

/// What the walk leaves: `log.index` holds exactly the closure of `root`.
pub struct Walked {
    /// Exact live bytes per sector.
    pub live: Vec<u16>,
    /// Header scans made (one per level below the root).
    pub scans: u32,
}

/// Index the closure of `root` into `log.index` (which it replaces).
/// `sectors` lists the trusted sectors as (sector seq, sector, …) in
/// ascending sector seq; each one's `log.sectors.end` is where its trusted
/// records end.
pub fn index_closure<F: Flash, K>(
    log: &mut RecordLog<F>,
    root: RootCandidate,
    sectors: &[(u32, u32, K)],
) -> R<Walked, F> {
    log.index = RamIndex::default();
    let mut out = Walked {
        live: alloc::vec![0u16; log.sector_count as usize],
        scans: 0,
    };
    let mut level: Vec<(ObjectId, MarkRole)> = alloc::vec![(root.id, MarkRole::Root)];
    let mut next: Vec<(ObjectId, MarkRole)> = Vec::new();
    let mut deferred: Vec<ObjectId> = Vec::new();
    let mut dir_bytes = Vec::new();
    let mut peak = 0;
    log.index.insert(root.id, root.loc);
    loop {
        // Visit the level (every id in it is indexed now).
        for &(id, role) in &level {
            let loc = log
                .index
                .get(id)
                .ok_or(StoreError::Corrupt("missing record"))?;
            let h = visit(
                log,
                id,
                role,
                loc,
                &mut next,
                &mut dir_bytes,
                Some(&mut deferred),
            )?;
            let l = &mut out.live[loc.sector as usize];
            *l = l.saturating_add(h.total_len() as u16);
        }
        // Directory multis whose chunks are all indexed: their entries now.
        let mut i = 0;
        while i < deferred.len() {
            if subtree_indexed(log, deferred[i])? {
                let id = deferred.swap_remove(i);
                push_dir_node_entries(log, id, &mut dir_bytes, &mut next)?;
            } else {
                i += 1;
            }
        }
        peak = peak.max(
            log.index.ram_bytes()
                + (level.capacity() + next.capacity()) * size_of::<(ObjectId, MarkRole)>()
                + deferred.capacity() * size_of::<ObjectId>()
                + dir_bytes.capacity(),
        );
        // The next level: what was named and is not indexed yet, by id.
        next.retain(|&(id, _)| !log.index.contains(id));
        next.sort_unstable_by_key(|e| e.0.0);
        next.dedup_by_key(|e| e.0.0);
        next.shrink_to_fit();
        if next.is_empty() {
            if !deferred.is_empty() {
                return Err(StoreError::Corrupt("directory chunks"));
            }
            break;
        }
        level = core::mem::take(&mut next);
        locate(log, &level, sectors)?;
        out.scans += 1;
    }
    log.note(peak);
    Ok(out)
}

/// One header scan, newest sector first: index each id of `want` (sorted
/// by id, none indexed yet) where it first appears. Every id must be found.
fn locate<F: Flash, K>(
    log: &mut RecordLog<F>,
    want: &[(ObjectId, MarkRole)],
    sectors: &[(u32, u32, K)],
) -> R<(), F> {
    let mut missing = want.len();
    'sectors: for &(_, s, _) in sectors.iter().rev() {
        let end = u32::from(log.sectors.end[s as usize]);
        let mut off = SECTOR_HEADER_LEN;
        while off + RECORD_HEADER_LEN <= end {
            let mut raw = [0u8; RECORD_HEADER_LEN as usize];
            log.read(log.addr(s, off), &mut raw)?;
            let h = match RecordHeader::parse(&raw) {
                HeaderRead::Record(h) => h,
                // Garbage the first pass checked: step over it.
                HeaderRead::Unknown { len } => {
                    off += RECORD_HEADER_LEN + u32::from(len);
                    continue;
                }
                HeaderRead::End | HeaderRead::Bad => break,
            };
            if off + h.total_len() > end {
                break;
            }
            // The first sighting is the newest copy; later ones are older.
            if want.binary_search_by_key(&h.id.0, |e| e.0.0).is_ok() && !log.index.contains(h.id) {
                log.index.insert(
                    h.id,
                    RecordLoc {
                        sector: s,
                        offset: off,
                    },
                );
                missing -= 1;
                if missing == 0 {
                    break 'sectors;
                }
            }
            off += h.total_len();
        }
    }
    if missing > 0 {
        return Err(StoreError::Corrupt("missing record"));
    }
    Ok(())
}

/// Every record under node `id` is indexed (multis are read to see their
/// children; their CRC is checked).
fn subtree_indexed<F: Flash>(log: &mut RecordLog<F>, id: ObjectId) -> R<bool, F> {
    let Some(loc) = log.index.get(id) else {
        return Ok(false);
    };
    if log.header_at(loc)?.kind != RecordKind::Multi {
        return Ok(true);
    }
    let (_, p) = log.read_record(id)?;
    let m = parse_multi(&p).ok_or(StoreError::Corrupt("multi"))?;
    for i in 0..m.count {
        if !subtree_indexed(log, multi_child(&p, i))? {
            return Ok(false);
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use alloc::format;
    use alloc::vec;
    use alloc::vec::Vec;

    use lp_nor_sim::{NorFlashSim, NorGeometry};

    use crate::StoreConfig;
    use crate::sector_header::{HeadKind, SECTOR_HEADER_LEN, SectorHeader, SectorRead};
    use crate::test_support::{Store, formatted, mount, noise, snapshot, text};

    /// After mount the index and the live bytes are exactly what a full
    /// mark from the committed root keeps — on a flash with garbage, GC
    /// copies, multi-part files, and a directory big enough to be a level-1
    /// multi (its entries are read two levels after it is found).
    #[test]
    fn mount_indexes_exactly_what_a_mark_keeps() {
        let c = StoreConfig {
            record_max: 256,
            ..StoreConfig::default()
        };
        let mut st = mount(formatted(NorGeometry::c6(32), &c), &c);
        st.put("/big.bin", &noise(1, 30_000)).unwrap();
        st.begin().unwrap();
        for i in 0..160u64 {
            let p = format!("/d/a-file-with-a-long-name-{i:03}.json");
            st.put(&p, &text(i, 40)).unwrap();
        }
        st.commit().unwrap();
        // Live records land between garbage, so GC has to copy some.
        for round in 0..60u64 {
            st.put("/churn.bin", &noise(round, 6000)).unwrap();
            st.put(&format!("/keep/k{round:02}.json"), &text(round, 200))
                .unwrap();
            st.put("/p/.lp/panel.json", &text(round, 300)).unwrap();
        }
        assert!(st.stats().gc_copies > 5, "{:?}", st.stats());
        let want = snapshot(&mut st);

        let mut st = mount(st.into_flash(), &c);
        assert!(st.stats().mount_scans >= 4, "{:?}", st.stats());
        let (index, live) = index_and_live(&st);
        st.mark_and_prune(false).unwrap();
        assert_eq!(index_and_live(&st), (index, live));
        assert_eq!(snapshot(&mut st), want);
    }

    /// Of two copies of a record the one in the later-opened sector is
    /// indexed (FORMAT.md mount step 2): copy a sector's records into a
    /// blank sector under a newer header, and every record that was indexed
    /// in the old sector moves to the copy.
    #[test]
    fn the_copy_in_the_newest_sector_is_indexed() {
        let c = StoreConfig::default();
        let geom = NorGeometry::c6(16);
        let mut st = mount(formatted(geom, &c), &c);
        for i in 0..12u64 {
            st.put(&format!("/m/f{i}.json"), &text(i, 700)).unwrap();
        }
        let want = snapshot(&mut st);
        let st = mount(st.into_flash(), &c);
        let (from, _) = st
            .log
            .sectors
            .live
            .iter()
            .enumerate()
            .max_by_key(|e| *e.1)
            .unwrap();
        let from = from as u32;
        let moved: Vec<(u64, u32)> = (0..st.log.index.len())
            .filter(|&i| st.log.index.loc_at(i).sector == from)
            .map(|i| (st.log.index.id_at(i).0, st.log.index.loc_at(i).offset))
            .collect();
        assert!(moved.len() > 3, "{moved:?}");
        let mut f = st.into_flash();
        let to = (0..16).find(|&s| f.sector_is_blank(s)).unwrap();
        copy_sector_newer(&mut f, from, to);

        let mut st = mount(f, &c);
        for (id, offset) in moved {
            let loc = st.log.index.get(crate::ObjectId(id)).unwrap();
            assert_eq!((loc.sector, loc.offset), (to, offset));
        }
        assert_eq!(snapshot(&mut st), want);
    }

    fn index_and_live(st: &Store) -> (Vec<(u64, u32, u32)>, Vec<u16>) {
        let ix = &st.log.index;
        let entries = (0..ix.len())
            .map(|i| (ix.id_at(i).0, ix.loc_at(i).sector, ix.loc_at(i).offset))
            .collect();
        (entries, st.log.sectors.live.clone())
    }

    /// Program sector `from`'s records into blank sector `to` under a header
    /// newer than every other.
    fn copy_sector_newer(f: &mut NorFlashSim, from: u32, to: u32) {
        let size = f.geometry().sector_size;
        let mut newest = 0;
        let mut kind = HeadKind::Cold;
        for s in 0..f.geometry().sector_count {
            let mut h = [0u8; SECTOR_HEADER_LEN as usize];
            f.read(s * size, &mut h).unwrap();
            if let SectorRead::Trusted { header: hd, .. } = SectorHeader::decode(&h, size) {
                newest = newest.max(hd.seq);
                if s == from {
                    kind = hd.kind;
                }
            }
        }
        let mut body = vec![0u8; (size - SECTOR_HEADER_LEN) as usize];
        f.read(from * size + SECTOR_HEADER_LEN, &mut body).unwrap();
        let header = SectorHeader {
            seq: newest + 1,
            erase_count: 1,
            kind,
        };
        f.program(to * size, &header.encode(size)).unwrap();
        f.program(to * size + SECTOR_HEADER_LEN, &body).unwrap();
    }
}
