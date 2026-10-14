//! Will a write fit? Decided before anything of it is written.

use alloc::vec::Vec;

use crate::heap_sort::heap_sort_by;
use crate::sector_header::HeadKind;

/// New sectors the heads must open to append `records` in order — the exact
/// rule `RecordLog::append` follows.
pub fn sectors_needed(
    mut remaining: [u32; 2],
    records: impl Iterator<Item = (HeadKind, u32)>,
    capacity: u32,
) -> u32 {
    let mut opened = 0;
    for (head, len) in records {
        let r = &mut remaining[head.index()];
        if *r < len {
            opened += 1;
            *r = capacity;
        }
        *r = r.saturating_sub(len);
    }
    opened
}

/// Room a sector is counted with in [`admits`]: its capacity less this.
/// GC packs a sector to within a few dozen bytes of full, not to the byte
/// (records never span a sector, and the small records that fill the last
/// tails run out), so a write that needs every byte would fit some layouts
/// of its live set and not others — the one a power cut leaves, say (the
/// 2026-10-10 defect). Measured on lp-nor-sim (c13 at 48 sectors): 16 B
/// and up let every re-run fit where its step fitted; 64 B also halved the
/// erases a step costs near full.
pub const PACK_SLACK: u32 = 64;

/// The admission rule, asked before every write phase whatever the layout:
/// do the live bytes of each kind plus the write's (`bytes`, indexed by
/// [`HeadKind::index`]) fit `usable` sectors less the reserve, each counted
/// at its capacity less [`PACK_SLACK`], the hot bytes in sectors of their
/// own? It reads only the live set and the write, so a step and its re-run
/// after a cut get the same answer; GC then has a slack in every sector to
/// reach it with. Monotone in `bytes`: true on live upper bounds is true on
/// the exact ones.
pub fn admits(bytes: [u64; 2], capacity: u32, usable: u32, reserve: u32) -> bool {
    let room = u64::from(capacity.saturating_sub(PACK_SLACK));
    let budget = u64::from(usable.saturating_sub(reserve));
    let hot = bytes[HeadKind::Hot.index()].div_ceil(room);
    bytes[HeadKind::Cold.index()] <= budget.saturating_sub(hot) * room
}

/// In the packing bound's sizes: the record is a hot one. A record is at
/// most a sector's capacity (< 32 KiB), so the top bit is free.
pub const HOT_LEN: u16 = 0x8000;

/// The packing bound, asked when the layout does not have the room: could
/// the live records plus the new ones (every record's length in `sizes`,
/// [`HOT_LEN`] set for a hot one), packed first-fit-decreasing into
/// `usable` sectors less the reserve, all fit? Records never span a sector
/// and a sector holds one kind, so each kind packs into its own sectors and
/// each sector keeps the tail its records leave. A byte sum (P11) counted
/// those tails as room, the hot head's included, admitted writes no layout
/// could hold, and GC ground through compactions that could never succeed
/// (the 2026-10-10 defect).
///
/// It must not reject what a layout holds: a write that fits leaves the
/// reserve free, so all its records sit in the other sectors. (It once also
/// took a sector off for the second head, so after a power cut it refused
/// re-runs of steps the layout had held; the 2026-10-09 defect.)
/// First-fit-decreasing is not an exact packing, but over these sizes
/// (chunks up to `record_max`, and many small directories and roots to fill
/// the tails) it is as good as one.
pub fn fits_after_compaction(
    mut sizes: Vec<u16>,
    capacity: u32,
    usable: u32,
    reserve: u32,
) -> bool {
    heap_sort_by(&mut sizes, |a, b| a & !HOT_LEN > b & !HOT_LEN);
    let budget = usable.saturating_sub(reserve) as usize;
    // Each sector's room, its kind in the top bit (as in `sizes`).
    let mut bins: Vec<u32> = Vec::new();
    for s in sizes {
        let (len, kind) = (u32::from(s & !HOT_LEN), u32::from(s & HOT_LEN) << 16);
        match bins
            .iter_mut()
            .find(|b| **b & KIND_BIT == kind && **b - kind >= len)
        {
            Some(room) => *room -= len,
            None => {
                if bins.len() == budget || len > capacity {
                    return false;
                }
                bins.push(kind | (capacity - len));
            }
        }
    }
    true
}

const KIND_BIT: u32 = (HOT_LEN as u32) << 16;

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;

    #[test]
    fn placement_matches_append_rule() {
        let recs = [
            (HeadKind::Cold, 600),
            (HeadKind::Cold, 600),
            (HeadKind::Hot, 50),
        ];
        assert_eq!(sectors_needed([1000, 0], recs.into_iter(), 1000), 2);
        assert_eq!(sectors_needed([1200, 100], recs.into_iter(), 1000), 0);
        // 7 sectors of 4,072 B after the reserve: four 1,000 B records each.
        assert!(fits_after_compaction(vec![1000; 28], 4072, 10, 3));
        assert!(!fits_after_compaction(vec![1000; 29], 4072, 10, 3));
        // Three 1,024 B records a sector, whatever the byte sum says: the
        // 1,000 B tails take small records, not a fourth chunk.
        assert!(!fits_after_compaction(vec![1024; 22], 4072, 10, 3));
        assert!(fits_after_compaction(vec![1024; 21], 4072, 10, 3));
        let mut mixed = vec![1024u16; 21];
        mixed.extend([300u16; 21]);
        assert!(fits_after_compaction(mixed.clone(), 4072, 10, 3));
        mixed.push(1024);
        assert!(!fits_after_compaction(mixed, 4072, 10, 3));
        // The hot head's room is not the cold records': one hot root takes
        // a sector of its own.
        let mut kinds = vec![1000u16; 28];
        assert!(fits_after_compaction(kinds.clone(), 4072, 10, 3));
        kinds.push(50 | HOT_LEN);
        assert!(!fits_after_compaction(kinds.clone(), 4072, 10, 3));
        kinds.truncate(24);
        kinds.extend([50 | HOT_LEN, 60 | HOT_LEN]);
        assert!(fits_after_compaction(kinds, 4072, 10, 3));
        // A record no sector holds.
        assert!(!fits_after_compaction(vec![5000], 4072, 10, 3));
    }

    #[test]
    fn admission_keeps_a_slack_in_every_sector_and_the_hot_bytes_apart() {
        let room = u64::from(4072 - PACK_SLACK);
        // 7 sectors after the reserve; the hot bytes take one of their own.
        assert!(admits([6 * room, 1], 4072, 10, 3));
        assert!(!admits([6 * room + 1, 1], 4072, 10, 3));
        assert!(admits([7 * room, 0], 4072, 10, 3));
        assert!(!admits([7 * room, 1], 4072, 10, 3));
        assert!(admits([5 * room, room + 1], 4072, 10, 3));
        assert!(!admits([5 * room + 1, room + 1], 4072, 10, 3));
        // No room at all.
        assert!(!admits([1, 0], 4072, 3, 3));
    }
}
