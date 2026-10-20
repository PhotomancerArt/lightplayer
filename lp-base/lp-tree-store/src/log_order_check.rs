//! Test-only checks of the two writer invariants a bounded reader (an index
//! checkpoint's roll-forward, a root search that reads no record headers)
//! stands on — FORMAT.md "What a writer guarantees", I4 and I5 — over raw
//! images, so the cut sweeps check them after every step and every cut:
//!
//! - **I4, log order** ([`check_log_order`]): between two images, every
//!   changed byte is a sector header changing (a kill, an erase, a sector
//!   opened with a seq newer than every one trusted before), or lies at or
//!   past the end of the records of the newest sector of its kind (an
//!   append, or a settle's kill of a record that failed to check). Nothing
//!   below a head's end, and nothing in a sector that is not a head, ever
//!   changes.
//! - **I5, root placement** ([`check_roots`]): every root is in a hot
//!   sector, root seqs never decrease in log order (sector seq, then
//!   offset), and the root a mount adopted is one of the two newest — so a
//!   reader finds it from the newest hot sectors alone.

extern crate std;

use alloc::format;
use alloc::vec;
use alloc::vec::Vec;

use lp_nor_sim::NorFlashSim;

use crate::record_header::{HeaderRead, RECORD_HEADER_LEN, RecordHeader};
use crate::record_kind::RecordKind;
use crate::root_record::RootRecord;
use crate::sector_header::{HeadKind, SECTOR_HEADER_LEN, SectorHeader, SectorRead};

/// Between `before` and `after` (the same flash, a step or a cut apart),
/// only appends at heads' ends and header changes (FORMAT.md I4).
pub fn check_log_order(before: &NorFlashSim, after: &NorFlashSim, ctx: &str) {
    let size = before.geometry().sector_size;
    let read = read_sectors(before);
    let trusted: Vec<u32> = read.iter().flatten().map(|s| s.header.seq).collect();
    if wrapped(&trusted) {
        // Seqs spanning half the counter or more: "newest" is unknowable
        // from seqs, and a reader reads the whole flash (FORMAT.md "A
        // faster reading"); I4 is a claim about stores that have not.
        return;
    }
    let newest = |kind: HeadKind| {
        read.iter()
            .enumerate()
            .filter_map(|(i, s)| s.as_ref().map(|s| (i as u32, s)))
            .filter(|(_, s)| s.header.kind == kind)
            .max_by_key(|(_, s)| s.header.seq)
            .map(|(i, _)| i)
    };
    let heads = [newest(HeadKind::Cold), newest(HeadKind::Hot)];
    for s in 0..before.geometry().sector_count {
        let (b, a) = (cells(before, s), cells(after, s));
        if b == a {
            continue;
        }
        let hdr = SECTOR_HEADER_LEN as usize;
        if b[..hdr] != a[..hdr] {
            // A kill, an erase (whole or torn), or a sector opened: a new
            // header must be newer than everything trusted before.
            let mut h = [0u8; SECTOR_HEADER_LEN as usize];
            h.copy_from_slice(&a[..hdr]);
            if let SectorRead::Trusted { header, .. } = SectorHeader::decode(&h, size) {
                assert!(
                    trusted.iter().all(|&t| newer(header.seq, t)),
                    "{ctx}: sector {s} opened with seq {} not newer than every trusted one ({trusted:?})",
                    header.seq
                );
            }
            continue;
        }
        let Some(sec) = &read[s as usize] else {
            // Not a store sector (blank, killed, torn): an erase in progress.
            continue;
        };
        assert_eq!(
            heads[sec.header.kind.index()],
            Some(s),
            "{ctx}: sector {s} ({:?}, seq {}) changed but is not the newest of its kind",
            sec.header.kind,
            sec.header.seq
        );
        let first = b.iter().zip(&a).position(|(x, y)| x != y).unwrap() as u32;
        assert!(
            first >= sec.end,
            "{ctx}: sector {s} changed at {first}, below its records' end {}",
            sec.end
        );
    }
}

/// Every root is in a hot sector, root seqs never decrease in log order,
/// and `adopted` (the seq a mount took) is one of the two greatest
/// (FORMAT.md I5). `flash` is read as a mount left it (settled).
pub fn check_roots(flash: &NorFlashSim, adopted: u64, ctx: &str) {
    let read = read_sectors(flash);
    let seqs: Vec<u32> = read.iter().flatten().map(|s| s.header.seq).collect();
    let in_order = !wrapped(&seqs);
    // (sector seq, offset, root seq), in log order.
    let mut roots: Vec<(u32, u32, u64)> = Vec::new();
    for (i, sec) in read.iter().enumerate() {
        let Some(sec) = sec else { continue };
        for (off, h, payload) in &sec.records {
            if h.kind != RecordKind::Root {
                continue;
            }
            let Some(root) = RootRecord::decode(payload) else {
                continue;
            };
            assert_eq!(
                sec.header.kind,
                HeadKind::Hot,
                "{ctx}: root {} in cold sector {i}",
                root.seq
            );
            roots.push((sec.header.seq, *off, root.seq));
        }
    }
    roots.sort_unstable();
    for w in roots.windows(2).filter(|_| in_order) {
        assert!(
            w[0].2 <= w[1].2,
            "{ctx}: root {} is later in the log than root {}",
            w[0].2,
            w[1].2
        );
    }
    let mut seqs: Vec<u64> = roots.iter().map(|r| r.2).collect();
    seqs.dedup();
    let newest_two = &seqs[seqs.len().saturating_sub(2)..];
    assert!(
        newest_two.contains(&adopted),
        "{ctx}: mount adopted root {adopted}, not one of the newest two {newest_two:?}"
    );
}

/// A trusted sector of an image, read as FORMAT.md's step 2 reads it.
struct ImageSector {
    header: SectorHeader,
    /// (offset, header, payload) of every record that checks.
    records: Vec<(u32, RecordHeader, Vec<u8>)>,
    /// Where reading stopped: the end of the records that check.
    end: u32,
}

fn read_sectors(f: &NorFlashSim) -> Vec<Option<ImageSector>> {
    let size = f.geometry().sector_size;
    (0..f.geometry().sector_count)
        .map(|s| {
            let c = cells(f, s);
            let mut h = [0u8; SECTOR_HEADER_LEN as usize];
            h.copy_from_slice(&c[..SECTOR_HEADER_LEN as usize]);
            let SectorRead::Trusted { header, .. } = SectorHeader::decode(&h, size) else {
                return None;
            };
            let mut records = Vec::new();
            let mut off = SECTOR_HEADER_LEN;
            while off + RECORD_HEADER_LEN <= size {
                let mut raw = [0u8; RECORD_HEADER_LEN as usize];
                raw.copy_from_slice(&c[off as usize..(off + RECORD_HEADER_LEN) as usize]);
                let len = match RecordHeader::parse(&raw) {
                    HeaderRead::Record(r) => u32::from(r.len),
                    HeaderRead::Unknown { len } => u32::from(len),
                    _ => break,
                };
                let at = off + RECORD_HEADER_LEN;
                if at + len > size {
                    break;
                }
                let payload = c[at as usize..(at + len) as usize].to_vec();
                if !RecordHeader::crc_ok(&raw, &payload) {
                    break;
                }
                if let HeaderRead::Record(r) = RecordHeader::parse(&raw) {
                    records.push((off, r, payload));
                }
                off = at + len;
            }
            Some(ImageSector {
                header,
                records,
                end: off,
            })
        })
        .collect()
}

fn cells(f: &NorFlashSim, s: u32) -> Vec<u8> {
    let size = f.geometry().sector_size;
    let mut c = vec![0u8; size as usize];
    f.peek(s * size, &mut c);
    c
}

/// Whether sector seq `a` is newer than `b` (wrapping, as the counter does).
fn newer(a: u32, b: u32) -> bool {
    (a.wrapping_sub(b) as i32) > 0
}

/// Whether trusted seqs span half the counter or more (as mount decides
/// it: newest minus oldest).
fn wrapped(seqs: &[u32]) -> bool {
    match (seqs.iter().min(), seqs.iter().max()) {
        (Some(a), Some(b)) => b - a >= 1 << 31,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StoreConfig;
    use crate::test_support::{formatted, mount};
    use lp_nor_sim::NorGeometry;

    /// The checks catch what they are for: a byte changed below a head's
    /// end, a byte appended to a sector that is not a head, and a root seq
    /// that goes backwards in the log.
    #[test]
    fn the_checks_catch_what_they_are_for() {
        let c = StoreConfig::default();
        let mut st = mount(formatted(NorGeometry::c6(16), &c), &c);
        for i in 0..14u8 {
            st.put(&format!("/f{i}.json"), &[b'a' + i; 900]).unwrap();
            st.put("/p/.lp/panel.json", &[b'0' + i; 50]).unwrap();
        }
        let seq = st.committed.as_ref().unwrap().root.seq;
        let before = st.into_flash();
        check_roots(&before, seq, "clean");
        let read = read_sectors(&before);
        let hot = (0..16)
            .filter(|&s| {
                read[s]
                    .as_ref()
                    .is_some_and(|x| x.header.kind == HeadKind::Hot)
            })
            .max_by_key(|&s| read[s].as_ref().unwrap().header.seq)
            .unwrap() as u32;
        let end = read[hot as usize].as_ref().unwrap().end;
        let caught =
            |f: &dyn Fn()| std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).is_err();
        // An append at the hot head's end is log order.
        let mut ok = before.clone();
        ok.program(hot * 4096 + end, &[0x00]).unwrap();
        check_log_order(&before, &ok, "append");
        // A byte cleared below the head's end is not.
        let mut below = before.clone();
        let hot_cells = cells(&before, hot);
        let set = (0..end)
            .rev()
            .find(|&o| hot_cells[o as usize] != 0)
            .unwrap();
        below.program(hot * 4096 + set, &[0x00]).unwrap();
        assert!(caught(&|| check_log_order(&before, &below, "below")));
        // A byte appended to an older sector (not a head) is not.
        let cold = (0..16)
            .filter(|&s| {
                read[s]
                    .as_ref()
                    .is_some_and(|x| x.header.kind == HeadKind::Cold)
            })
            .max_by_key(|&s| read[s].as_ref().unwrap().header.seq)
            .unwrap();
        let old = (0..16)
            .find(|&s| s != hot as usize && s != cold && read[s].is_some())
            .unwrap() as u32;
        let old_end = read[old as usize].as_ref().unwrap().end;
        let mut stale = before.clone();
        stale.program(old * 4096 + old_end, &[0x00]).unwrap();
        assert!(caught(&|| check_log_order(&before, &stale, "not a head")));
        // A mount that adopted anything but the newest roots is caught.
        assert!(caught(&|| check_roots(&before, seq - 2, "old root")));
    }
}
