//! Choosing the sector GC collects next.

use crate::flash::Flash;
use crate::record_log::RecordLog;
use crate::store_config::GcPolicy;

/// The best victim: a written, non-head, non-retired sector with live
/// records *and* garbage (a sector with no live record is already free).
/// Needs exact live bytes (right after a mark, or GC's own copies since).
pub fn choose_victim<F: Flash>(log: &RecordLog<F>, policy: GcPolicy) -> Option<u32> {
    let cap = u64::from(log.sector_capacity());
    let now = u64::from(log.next_sector_seq);
    let mut best: Option<(u64, u32)> = None;
    for s in 0..log.sector_count {
        if log.is_head(s) || !log.sectors.is_written(s) || log.sectors.is_retired(s) {
            continue;
        }
        // Reclaimable = garbage *and* the unused tail of a sector that
        // stopped being a head (a record did not fit): compaction is what
        // the packing bound promised, so GC must be able to win tails too.
        let live = u64::from(log.sectors.live[s as usize]);
        if live == 0 || live >= cap {
            continue;
        }
        let score = match policy {
            GcPolicy::Greedy => cap - live,
            GcPolicy::CostBenefit => {
                let seq = u64::from(log.sectors.seq[s as usize]);
                let age = now.saturating_sub(seq) + 1;
                (cap.saturating_sub(live) * age * 1024) / (cap + live)
            }
        };
        if best.is_none_or(|(b, _)| score > b) {
            best = Some((score, s));
        }
    }
    best.map(|(_, s)| s)
}
