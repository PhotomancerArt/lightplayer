//! The largest allocatable block, found by asking the allocator.

/// The largest size in `0..=upper` for which `fits` says yes, assuming `fits`
/// is monotone (if a size fits, every smaller one does): a binary search to
/// the exact byte. The chips' `largest_free_block` probes the heap with it
/// (`fits` = a raw allocation that is freed at once).
///
/// Exact, not rounded: the gates compare it against round floors (a project
/// load's 64 KiB, a read's 16 KiB), and an answer rounded down by even one
/// byte turned a whole 65,536 B hole into "65,535 < 65,536, refused"
/// (`docs/defects/2026-10-06-the-largest-block-probe-reads-a-64-kib-hole-as-65535.md`).
/// The cost is a few more probes: about log2(`upper`).
pub fn largest_fitting(upper: usize, mut fits: impl FnMut(usize) -> bool) -> usize {
    // Invariant: `lo` fits (0 always does), `hi` does not.
    let mut lo = 0usize;
    let mut hi = upper.saturating_add(1);
    while hi - lo > 1 {
        let mid = lo + (hi - lo) / 2;
        if fits(mid) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_whole_64_kib_hole_reads_as_65536() {
        for upper in [65_536, 70_000, 200_000, 301_536] {
            assert_eq!(largest_fitting(upper, |size| size <= 65_536), 65_536);
        }
    }

    #[test]
    fn every_size_is_found_exactly() {
        for largest in [0usize, 1, 3, 4, 15, 16, 17, 16_383, 16_384, 49_232] {
            assert_eq!(
                largest_fitting(100_000, |size| size <= largest),
                largest,
                "{largest}"
            );
        }
    }

    #[test]
    fn the_upper_bound_itself_can_fit() {
        assert_eq!(largest_fitting(4_096, |_| true), 4_096);
        assert_eq!(largest_fitting(0, |_| true), 0);
    }
}
