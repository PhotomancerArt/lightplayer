//! Pure pool policies: least-loaded worker choice and LRU eviction.
//!
//! Kept browser-free so the decisions the host makes under budget pressure
//! are unit-testable natively.

/// One live slot as the eviction policy sees it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EvictionCandidate {
    /// Caller-side identifier echoed back by [`choose_eviction`].
    pub slot_id: u64,
    /// Whether the consumer currently reports the slot visible.
    pub visible: bool,
    /// Last activity stamp (lease, visibility, present completion), in the
    /// caller's clock — smaller is older.
    pub last_active_ms: f64,
}

/// Pick the slot to evict when the live-slot cap is hit: the
/// least-recently-active candidate, preferring invisible slots over
/// visible ones (a visible slot is only evicted when every live slot is
/// visible). `None` when there is nothing to evict.
pub fn choose_eviction(candidates: &[EvictionCandidate]) -> Option<u64> {
    let pick = |visible: bool| {
        candidates
            .iter()
            .filter(|candidate| candidate.visible == visible)
            .min_by(|a, b| a.last_active_ms.total_cmp(&b.last_active_ms))
    };
    pick(false).or_else(|| pick(true)).map(|c| c.slot_id)
}

/// Pick the least-loaded available worker. `loads[i]` is `Some(assigned
/// slot count)` for a usable worker and `None` for one that is dead or
/// still booting. Ties resolve to the lowest index.
pub fn choose_worker(loads: &[Option<usize>]) -> Option<usize> {
    loads
        .iter()
        .enumerate()
        .filter_map(|(index, load)| load.map(|load| (index, load)))
        .min_by_key(|&(index, load)| (load, index))
        .map(|(index, _)| index)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(slot_id: u64, visible: bool, last_active_ms: f64) -> EvictionCandidate {
        EvictionCandidate {
            slot_id,
            visible,
            last_active_ms,
        }
    }

    #[test]
    fn eviction_prefers_the_oldest_invisible_slot() {
        let candidates = [
            candidate(1, true, 10.0),
            candidate(2, false, 500.0),
            candidate(3, false, 200.0),
            candidate(4, true, 5.0),
        ];
        assert_eq!(choose_eviction(&candidates), Some(3));
    }

    #[test]
    fn eviction_falls_back_to_the_oldest_visible_slot() {
        let candidates = [
            candidate(1, true, 300.0),
            candidate(2, true, 100.0),
            candidate(3, true, 200.0),
        ];
        assert_eq!(choose_eviction(&candidates), Some(2));
    }

    #[test]
    fn eviction_with_no_candidates_is_none() {
        assert_eq!(choose_eviction(&[]), None);
    }

    #[test]
    fn worker_choice_takes_the_least_loaded_available_worker() {
        assert_eq!(choose_worker(&[Some(3), Some(1)]), Some(1));
        assert_eq!(choose_worker(&[None, Some(4)]), Some(1));
        assert_eq!(choose_worker(&[None, None]), None);
        assert_eq!(choose_worker(&[]), None);
    }

    #[test]
    fn worker_choice_breaks_ties_toward_the_lowest_index() {
        assert_eq!(choose_worker(&[Some(2), Some(2)]), Some(0));
    }
}
