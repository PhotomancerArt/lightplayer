//! Mount's second pass: index only the chosen root's closure, so mount's RAM
//! is bounded by the live set, not by the garbage on flash.
//!
//! The walk is a mark ([`visit`], the step GC's mark uses) that has no index
//! to look ids up in. It goes level by level: the ids the last level named
//! are looked for by one scan of record **headers** over the trusted
//! sectors, newest sector first, and the first copy seen is the one kept —
//! the copy in the sector with the highest sector seq (FORMAT.md "Record",
//! mount step 2). Each record is visited where it is found (its header is
//! not read again: only a payload is read, once), and what it names is the
//! next level. An id no trusted sector holds makes the closure incomplete
//! (`Corrupt`), as a missing record does in a mark.
//!
//! A scan skips every sector whose filter (`sector_filter.rs`, built from
//! the headers pass 1 walked) says it holds none of the ids still wanted,
//! so a level of a few directories reads the few sectors that may hold them,
//! not every header on flash; a level wanting many ids scans every sector.
//! What a scan finds is indexed in one merge when it ends.
//!
//! The first pass (`record_scan.rs`) set each sector's `end` to just past its
//! last trusted record; this pass reads headers only inside
//! `[SECTOR_HEADER_LEN, end)`, and stops a sector at a header that no longer
//! parses (a re-read that differs: the walk never trusts more than the first
//! pass did). Every record that names others is read whole and CRC-checked
//! by the visit, as in a mark; a blob is checked when it is read (every
//! read of a record checks its CRC), not here.
//!
//! A directory too big for one record is a flagged multi: its entries are
//! read once every chunk under it is indexed, a level or two later.
//!
//! **With mount's whole index** (`mount_index.rs`: pass 1 kept every record
//! it read, while that fit its RAM cap) a level's ids are found by lookup,
//! not by a scan: each closure record's header is read once where the index
//! says the adopted copy is, and visited there. The same copies, the same
//! levels, the same deferral of directory multis — only the finding differs,
//! so both ways index the same closure (the tests below compare them).
//!
//! Cost: per level, the headers of the sectors that may hold its ids
//! (stopping once every wanted id is found), plus each closure record's
//! payload once — with the whole index, one header read per closure record
//! instead of the scans; RAM: the live index plus the two widest adjacent
//! levels (16 B per id, and 4 B per id of the level being scanned).

use alloc::vec::Vec;

use crate::flash::Flash;
use crate::gc_mark::{MarkRole, VisitOut, push_dir_node_entries, visit, visit_header};
use crate::heap_sort::heap_sort_by;
use crate::mount_index::MountIndex;
use crate::multi_node::{multi_child, parse_multi};
use crate::object_id::ObjectId;
use crate::ram_index::{RamIndex, RecordLoc};
use crate::record_header::{HeaderRead, RECORD_HEADER_LEN, RecordHeader};
use crate::record_kind::RecordKind;
use crate::record_log::RecordLog;
use crate::record_scan::RootCandidate;
use crate::sector_filter::SectorFilter;
use crate::sector_header::SECTOR_HEADER_LEN;
use crate::store_error::StoreError;

type R<T, F> = Result<T, StoreError<<F as Flash>::Error>>;

/// `locate`'s "not found yet" (no record sits at sector 0xFFFF).
const NOT_FOUND: u32 = u32::MAX;

/// A level wanting more ids than this scans every sector.
const FILTER_MAX_WANTED: usize = 96;

/// What the walk leaves: `log.index` holds exactly the closure of `root`.
pub struct Walked {
    /// Exact live bytes per sector.
    pub live: Vec<u16>,
    /// Header scans made (one per level below the root).
    #[cfg(feature = "stats")]
    pub scans: u32,
}

/// Index the closure of `root` into `log.index` (which it replaces).
/// `sectors` lists the trusted sectors in ascending sector seq; each one's `log.sectors.end` is where its trusted
/// records end.
/// `index`: mount's whole id index, when pass 1 kept one (found by lookup;
/// else by scans over `sectors` with `filter`).
pub fn index_closure<F: Flash>(
    log: &mut RecordLog<F>,
    root: RootCandidate,
    sectors: &[u16],
    filter: &SectorFilter,
    index: Option<&MountIndex>,
) -> R<Walked, F> {
    log.index = RamIndex::default();
    let mut out = Walked {
        live: alloc::vec![0u16; log.sector_count as usize],
        #[cfg(feature = "stats")]
        scans: 0,
    };
    let mut next: Vec<(ObjectId, MarkRole)> = Vec::new();
    let mut deferred: Vec<ObjectId> = Vec::new();
    let mut dir_bytes = Vec::new();
    let mut peak = 0;
    log.index.insert(root.id, root.loc);
    let mut found = VisitOut {
        stack: &mut next,
        dir_bytes: &mut dir_bytes,
        defer: Some(&mut deferred),
    };
    let h = visit(log, root.id, MarkRole::Root, root.loc, &mut found)?;
    add_live(&mut out.live, root.loc, h.total_len());
    loop {
        // Directory multis whose chunks are all indexed: their entries now.
        let deferred = found.defer.as_deref_mut().expect("the walk defers");
        let mut i = 0;
        while i < deferred.len() {
            if subtree_indexed(log, deferred[i])? {
                let id = deferred.swap_remove(i);
                push_dir_node_entries(log, id, found.dir_bytes, found.stack)?;
            } else {
                i += 1;
            }
        }
        let deferred_cap = deferred.capacity();
        let pending = deferred.is_empty();
        // The next level: what was named and is not indexed yet, by id.
        let next = &mut *found.stack;
        next.retain(|&(id, _)| !log.index.contains(id));
        heap_sort_by(next, |a, b| a.0.0 < b.0.0);
        next.dedup_by_key(|e| e.0.0);
        next.shrink_to_fit();
        if next.is_empty() {
            if !pending {
                return Err(StoreError::Corrupt("directory chunks"));
            }
            break;
        }
        let level = core::mem::take(next);
        peak = peak.max(
            log.index.ram_bytes()
                + level.capacity() * size_of::<(ObjectId, MarkRole)>()
                + deferred_cap * size_of::<ObjectId>()
                + found.dir_bytes.capacity(),
        );
        match index {
            Some(ix) => locate_indexed(log, &level, ix, &mut found, &mut out.live)?,
            None => locate(log, &level, sectors, filter, &mut found, &mut out.live)?,
        }
        peak = peak.max(
            log.index.ram_bytes()
                + (level.capacity() + found.stack.capacity()) * size_of::<(ObjectId, MarkRole)>()
                + deferred_cap * size_of::<ObjectId>()
                + found.dir_bytes.capacity(),
        );
        stat!(out.scans += 1);
    }
    log.note(peak);
    Ok(out)
}

fn add_live(live: &mut [u16], loc: RecordLoc, len: u32) {
    let l = &mut live[loc.sector as usize];
    *l = l.saturating_add(len as u16);
}

/// One header scan, newest sector first: index each id of `want` (sorted
/// by id, none indexed yet) where it first appears, and visit it there
/// (its header is not read again). Every id must be found.
fn locate<F: Flash>(
    log: &mut RecordLog<F>,
    want: &[(ObjectId, MarkRole)],
    sectors: &[u16],
    filter: &SectorFilter,
    found: &mut VisitOut<'_>,
    live: &mut [u16],
) -> R<(), F> {
    let mut missing = want.len();
    // Where each wanted id was found (packed), indexed in one merge at the
    // end: one insert at a time into the sorted index moves the entries
    // after it each time, quadratic in the live set.
    let mut at = alloc::vec![NOT_FOUND; want.len()];
    // Past this many, nearly every sector may hold one: scan them all.
    let filtered = want.len() <= FILTER_MAX_WANTED;
    'sectors: for s in sectors.iter().rev().map(|&s| u32::from(s)) {
        if filtered
            && !want
                .iter()
                .zip(&at)
                .any(|(w, &a)| a == NOT_FOUND && filter.may_hold(s, w.0))
        {
            continue;
        }
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
            if let Ok(i) = want.binary_search_by_key(&h.id.0, |e| e.0.0)
                && at[i] == NOT_FOUND
            {
                let loc = RecordLoc {
                    sector: s,
                    offset: off,
                };
                at[i] = loc.pack();
                visit_header(log, h.id, want[i].1, loc, &raw, found)?;
                add_live(live, loc, h.total_len());
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
    log.index.merge_sorted(
        want.iter()
            .zip(at)
            .map(|(w, a)| (w.0, RecordLoc::unpack(a)))
            .rev(),
        want.len(),
    );
    Ok(())
}

/// [`locate`] by mount's whole index: each id of `want` (sorted by id, none
/// indexed yet) is visited where the index says its adopted copy is.
fn locate_indexed<F: Flash>(
    log: &mut RecordLog<F>,
    want: &[(ObjectId, MarkRole)],
    index: &MountIndex,
    found: &mut VisitOut<'_>,
    live: &mut [u16],
) -> R<(), F> {
    let mut at = Vec::with_capacity(want.len());
    for &(id, role) in want {
        let loc = index.get(id).ok_or(StoreError::Corrupt("missing record"))?;
        let h = visit(log, id, role, loc, found)?;
        add_live(live, loc, h.total_len());
        at.push(loc.pack());
    }
    log.index.merge_sorted(
        want.iter()
            .zip(at)
            .map(|(w, a)| (w.0, RecordLoc::unpack(a)))
            .rev(),
        want.len(),
    );
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
    use crate::record_header::RECORD_HEADER_LEN;
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

        for index_max in [c.mount_index_max, 0] {
            let c = StoreConfig {
                mount_index_max: index_max,
                ..c.clone()
            };
            let mut st = mount(f.clone(), &c);
            assert_eq!(st.stats().mount_indexed, index_max > 0);
            for &(id, offset) in &moved {
                let loc = st.log.index.get(crate::ObjectId(id)).unwrap();
                assert_eq!((loc.sector, loc.offset), (to, offset));
            }
            assert_eq!(snapshot(&mut st), want);
        }
    }

    /// A newer copy that does not check, over an older one that does: the
    /// fast mount adopts the newer copy, finds that multi's CRC wrong, and
    /// the whole reading (which closes the copy's sector at that record, as
    /// FORMAT.md step 2 reads it) mounts the committed state from the older
    /// copy. What `lp-store-bench`'s `shadowed_copy` fuzz images hold the
    /// store to.
    #[test]
    fn a_damaged_newer_copy_falls_back_to_reading_every_record() {
        let c = StoreConfig::default();
        let geom = NorGeometry::c6(16);
        let mut st = mount(formatted(geom, &c), &c);
        for i in 0..12u64 {
            st.put(&format!("/m/f{i}.bin"), &text(i, 3000)).unwrap();
        }
        let want = snapshot(&mut st);
        let mut st = mount(st.into_flash(), &c);
        assert!(!st.stats().mount_read_every_record);
        // A live multi in a cold sector that is not its kind's newest.
        let cold = |st: &Store, s: u32| st.log.sectors.kind(s) == HeadKind::Cold;
        let newest_cold = (0..16)
            .filter(|&s| st.log.sectors.is_written(s) && cold(&st, s))
            .max_by_key(|&s| st.log.sectors.seq[s as usize])
            .unwrap();
        let locs: Vec<_> = (0..st.log.index.len())
            .map(|i| st.log.index.loc_at(i))
            .filter(|loc| loc.sector != newest_cold && cold(&st, loc.sector))
            .collect();
        let multi = locs
            .into_iter()
            .find(|&loc| {
                st.log.header_at(loc).unwrap().kind == crate::record_kind::RecordKind::Multi
            })
            .expect("a live multi");
        let from = multi.sector;
        let seq = st.log.sectors.seq[from as usize];
        let mut f = st.into_flash();
        // Equal seqs order by sector: the copy is newer than its original
        // and older than the head.
        let to = (from + 1..16)
            .find(|&s| f.sector_is_blank(s))
            .expect("a blank sector after it");
        copy_sector_as(&mut f, from, to, seq);
        // Clear one set bit of its payload, as a program could.
        let mut b = [0u8; 1];
        let at = (0..)
            .map(|k| to * 4096 + multi.offset + RECORD_HEADER_LEN + k)
            .find(|&a| {
                f.peek(a, &mut b);
                b[0] != 0
            })
            .unwrap();
        let bit = (0..8).find(|i| b[0] & (1 << i) != 0).unwrap();
        f.program(at, &[b[0] & !(1 << bit)]).unwrap();

        let mut st = mount(f, &c);
        assert!(st.stats().mount_read_every_record, "the fast pass gave up");
        assert_eq!(snapshot(&mut st), want);
    }

    /// Both mount walks — by the whole id index and by level scans — adopt
    /// the same records on every store a board holds (empty, c40, written
    /// out, panel-heavy, full), and an index that outgrows its cap part-way
    /// through pass 1 is dropped and the level walk mounts the same state.
    #[test]
    fn both_walks_adopt_the_same_records() {
        let c = StoreConfig::default();
        for (name, f) in crate::mount_cost_tests::scenarios(48) {
            let f = mount(f, &c).into_flash();
            crate::test_support::assert_walks_agree(&f, &c, &|| format!("{name}"));
            let mut st = mount(f, &c);
            assert!(st.stats().mount_indexed, "{name}");
            let want = snapshot(&mut st);
            // A cap the records on flash pass part-way through pass 1.
            let small = StoreConfig {
                mount_index_max: 12 * 64,
                ..c.clone()
            };
            let mut st = mount(st.into_flash(), &small);
            if name != "empty" {
                assert!(!st.stats().mount_indexed, "{name}: a 64-slot table held it");
            }
            assert_eq!(snapshot(&mut st), want, "{name}");
        }
    }

    fn index_and_live(st: &Store) -> (Vec<(u64, u32, u32)>, Vec<u16>) {
        let ix = &st.log.index;
        let entries = (0..ix.len())
            .map(|i| (ix.id_at(i).0, ix.loc_at(i).sector, ix.loc_at(i).offset))
            .collect();
        (entries, st.log.sectors.live.clone())
    }

    /// Program sector `from`'s records into blank sector `to` under its own
    /// header with sector seq `seq`.
    fn copy_sector_as(f: &mut NorFlashSim, from: u32, to: u32, seq: u32) {
        let size = f.geometry().sector_size;
        let mut h = [0u8; SECTOR_HEADER_LEN as usize];
        f.read(from * size, &mut h).unwrap();
        let SectorRead::Trusted { header, .. } = SectorHeader::decode(&h, size) else {
            panic!("sector {from} has no header");
        };
        let mut body = vec![0u8; (size - SECTOR_HEADER_LEN) as usize];
        f.read(from * size + SECTOR_HEADER_LEN, &mut body).unwrap();
        let header = SectorHeader { seq, ..header };
        f.program(to * size, &header.encode(size)).unwrap();
        f.program(to * size + SECTOR_HEADER_LEN, &body).unwrap();
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
