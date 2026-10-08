//! Choosing the sector GC collects next.

use crate::flash::Flash;
use crate::record_log::RecordLog;
use crate::sector_header::SECTOR_HEADER_LEN;
use crate::sector_table::SectorUse;
use crate::store_config::GcPolicy;

/// The best victim: a written, non-head sector with live records *and*
/// garbage (a sector with no live record is already free).
pub fn choose_victim<F: Flash>(log: &RecordLog<F>, policy: GcPolicy) -> Option<u32> {
    let cap = u64::from(log.sector_capacity());
    let now = u64::from(log.next_sector_seq);
    let mut best: Option<(u64, u32)> = None;
    for s in 0..log.sector_count {
        if log.is_head(s) {
            continue;
        }
        let SectorUse::Written { header, end } = log.sectors.uses[s as usize] else {
            continue;
        };
        let live = u64::from(log.sectors.live_bytes[s as usize]);
        let used = u64::from(end - SECTOR_HEADER_LEN);
        if live == 0 || used <= live {
            continue;
        }
        let score = match policy {
            GcPolicy::Greedy => used - live,
            GcPolicy::CostBenefit => {
                let age = now.saturating_sub(u64::from(header.seq)) + 1;
                (cap.saturating_sub(live) * age * 1024) / (cap + live)
            }
        };
        if best.is_none_or(|(b, _)| score > b) {
            best = Some((score, s));
        }
    }
    best.map(|(_, s)| s)
}
