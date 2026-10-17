//! Choosing the sector GC collects next.

use crate::flash::Flash;
use crate::gc_copy::opened_since;
use crate::record_log::RecordLog;
use crate::sector_header::{HeadKind, SECTOR_HEADER_LEN};
use crate::store_config::GcPolicy;

/// The best victim: a written, non-head, non-retired sector with live
/// records *and* garbage (a sector with no live record is already free),
/// that this GC pass did not open (`pass_from`: the first sector sequence
/// the pass opened; what it opened holds its own packed copies).
///
/// When no sector has garbage, a sector whose only waste is the unused tail
/// it was left with when it stopped being a head (compaction, which the
/// packing bound assumed): first the smallest whose live records all fit
/// what the cold head has left (collecting it frees a sector without
/// opening one), else the smallest of all (it opens a head, whose room the
/// next one can use). Each sector is collected at most once a pass, so a
/// pass ends; the caller starts another while passes still win room.
///
/// Needs exact live bytes (right after a mark, or GC's own copies since).
pub fn choose_victim<F: Flash>(
    log: &RecordLog<F>,
    policy: GcPolicy,
    pass_from: u32,
) -> Option<u32> {
    let cap = u64::from(log.sector_capacity());
    let now = u64::from(log.next_sector_seq);
    let head_room = u64::from(log.head_remaining(HeadKind::Cold));
    let mut best: Option<(u64, u32)> = None;
    let mut best_tail: Option<(u64, u32)> = None;
    let mut smallest: Option<(u64, u32)> = None;
    for s in 0..log.sector_count {
        if log.is_head(s)
            || !log.sectors.is_written(s)
            || log.sectors.is_retired(s)
            || opened_since(log, s, pass_from)
        {
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
        let score = victim_score(
            policy,
            used - live,
            live,
            cap,
            now,
            log.sectors.seq[s as usize],
        );
        if best.is_none_or(|(b, _)| score > b) {
            best = Some((score, s));
        }
    }
    best.or(best_tail).or(smallest).map(|(_, sector)| sector)
}

/// An idle step's victim ([`crate::TreeStore::collect_step`]): by the same
/// policy, a written, non-head, non-retired sector with live records and at
/// least `min_garbage` bytes of garbage — never one whose only waste is its
/// tail, so each idle collection wins garbage back and the steps end.
/// Needs exact live bytes.
pub fn choose_idle_victim<F: Flash>(
    log: &RecordLog<F>,
    policy: GcPolicy,
    min_garbage: u32,
) -> Option<u32> {
    let cap = u64::from(log.sector_capacity());
    let now = u64::from(log.next_sector_seq);
    let mut best: Option<(u64, u32)> = None;
    for s in 0..log.sector_count {
        if log.is_head(s) || !log.sectors.is_written(s) || log.sectors.is_retired(s) {
            continue;
        }
        let live = u64::from(log.sectors.live[s as usize]);
        let used = u64::from(log.sectors.end[s as usize]) - u64::from(SECTOR_HEADER_LEN);
        if live == 0 || used < live + u64::from(min_garbage.max(1)) {
            continue;
        }
        let score = victim_score(
            policy,
            used - live,
            live,
            cap,
            now,
            log.sectors.seq[s as usize],
        );
        if best.is_none_or(|(b, _)| score > b) {
            best = Some((score, s));
        }
    }
    best.map(|(_, sector)| sector)
}

/// A garbage sector's score under `policy` (larger = collect first).
fn victim_score(policy: GcPolicy, garbage: u64, live: u64, cap: u64, now: u64, seq: u32) -> u64 {
    match policy {
        GcPolicy::Greedy => garbage,
        GcPolicy::CostBenefit => {
            let age = now.saturating_sub(u64::from(seq)) + 1;
            (cap.saturating_sub(live) * age * 1024) / (cap + live)
        }
    }
}
