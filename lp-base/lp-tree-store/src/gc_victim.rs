//! Choosing the sector GC collects next.

use crate::flash::Flash;
use crate::record_log::RecordLog;
use crate::sector_header::{HeadKind, SECTOR_HEADER_LEN};
use crate::store_config::GcPolicy;

/// The best victim: a written, non-head, non-retired sector with live
/// records *and* garbage (a sector with no live record is already free).
///
/// When no sector has garbage, a sector whose only waste is the unused tail
/// it was left with when it stopped being a head (compaction, which the
/// packing bound assumed): first the smallest whose live records all fit
/// what the cold head has left (collecting it frees a sector without
/// opening one), else the smallest of all (it opens a head, whose room the
/// next one can use). Copying tail-only sectors can gain nothing when
/// records are near `record_max` — every sector ends with the same tail —
/// so the caller stops GC when collections stop freeing sectors.
///
/// Needs exact live bytes (right after a mark, or GC's own copies since).
pub fn choose_victim<F: Flash>(log: &RecordLog<F>, policy: GcPolicy) -> Option<u32> {
    let cap = u64::from(log.sector_capacity());
    let now = u64::from(log.next_sector_seq);
    let head_room = u64::from(log.head_remaining(HeadKind::Cold));
    let mut best: Option<(u64, u32)> = None;
    let mut best_tail: Option<(u64, u32)> = None;
    let mut smallest: Option<(u64, u32)> = None;
    for s in 0..log.sector_count {
        if log.is_head(s) || !log.sectors.is_written(s) || log.sectors.is_retired(s) {
            continue;
        }
        let live = u64::from(log.sectors.live[s as usize]);
        let used = u64::from(log.sectors.end[s as usize]) - u64::from(SECTOR_HEADER_LEN);
        if live == 0 {
            continue;
        }
        if used <= live {
            if live <= head_room && best_tail.is_none_or(|(l, _)| live < l) {
                best_tail = Some((live, s));
            }
            if live < cap && smallest.is_none_or(|(l, _)| live < l) {
                smallest = Some((live, s));
            }
            continue;
        }
        let score = match policy {
            GcPolicy::Greedy => used - live,
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
    best.or(best_tail).or(smallest).map(|(_, s)| s)
}
