//! Will a write fit? Decided before anything of it is written.

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

/// The pre-write bound: do the live records plus the new ones (`bytes`, all
/// of them) fit `usable` sectors less the reserve? A byte sum, optimistic
/// on purpose (records never span a sector, so real packing leaves a tail in
/// each, and GC copies in victim order): a write this rejects cannot fit;
/// one it accepts may still end in `NoSpace` after GC, before any of its
/// records.
///
/// It must never reject what a layout holds: a write that fits leaves the
/// reserve free, so all its bytes sit in the other sectors. (It once also
/// took a sector off for the second head, so after a power cut it refused
/// re-runs of steps the layout had held — a cut leaves the live set, and so
/// this bound, as it was; the 2026-10-09 defect.)
pub fn fits_after_compaction(bytes: u64, capacity: u32, usable: u32, reserve: u32) -> bool {
    let budget = usable.saturating_sub(reserve);
    bytes <= u64::from(budget) * u64::from(capacity)
}

#[cfg(test)]
mod tests {
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
        // 7 sectors of 4,072 B after the reserve.
        assert!(fits_after_compaction(20 * 1000, 4072, 10, 3));
        assert!(fits_after_compaction(7 * 4072, 4072, 10, 3));
        assert!(!fits_after_compaction(7 * 4072 + 1, 4072, 10, 3));
        assert!(!fits_after_compaction(40 * 1000, 4072, 10, 3));
    }
}
