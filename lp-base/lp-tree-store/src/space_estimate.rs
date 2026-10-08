//! Will a commit fit? Decided before anything of it is written.

use alloc::vec::Vec;

use crate::sector_header::HeadKind;
use crate::small_sort::sort_small_by;

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
        *r -= len;
    }
    opened
}

/// The pre-write bound: could every live record plus the commit's new
/// records, packed first-fit-decreasing into sectors, leave one sector for
/// the other head and the reserve free? Optimistic about packing on purpose
/// (GC copies in victim order, not size order): a commit this rejects cannot
/// fit; one it accepts may still end in `NoSpace` after GC, with nothing of
/// the commit written.
pub fn fits_after_compaction(
    mut sizes: Vec<u32>,
    capacity: u32,
    sector_count: u32,
    reserve: u32,
) -> bool {
    sort_small_by(&mut sizes, |a, b| a < b);
    let budget = sector_count.saturating_sub(reserve + 1) as usize;
    let mut bins: Vec<u32> = Vec::new();
    for len in sizes.into_iter().rev() {
        match bins.iter_mut().find(|room| **room >= len) {
            Some(room) => *room -= len,
            None => {
                if bins.len() == budget || len > capacity {
                    return false;
                }
                bins.push(capacity - len);
            }
        }
    }
    true
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
        assert!(fits_after_compaction(alloc::vec![1000; 20], 4076, 10, 3));
        assert!(!fits_after_compaction(alloc::vec![1000; 40], 4076, 10, 3));
    }
}
