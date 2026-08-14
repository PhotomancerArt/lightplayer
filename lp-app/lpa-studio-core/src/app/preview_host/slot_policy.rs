//! Pure pool policies: least-loaded worker choice, LRU eviction, and
//! which pending leases may start deploying.
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

/// Where a slot lands when its runtime is torn out from under it (LRU
/// eviction or a worker recycle) while it is neither released nor
/// terminal.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DetachedSlotNext {
    /// Still visible: re-lease immediately. Parking a visible slot
    /// `Suspended` would freeze it forever — `set_visible(true)` only
    /// fires on a visibility EDGE, and a slot the consumer already
    /// reports visible never gets another one.
    Redeploy,
    /// Not visible: park `Suspended` until the next visibility edge.
    Park,
}

/// Decide a detached slot's future from its visibility. Both detach
/// paths (eviction and recycle) must agree on this — they diverged
/// once, and the evicted-while-visible half froze thumbs permanently.
pub fn detached_slot_next(visible: bool) -> DetachedSlotNext {
    if visible {
        DetachedSlotNext::Redeploy
    } else {
        DetachedSlotNext::Park
    }
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

/// Which pending slots may start deploying this tick.
///
/// Starting every pending lease at once is what makes a freshly loaded
/// gallery flicker: a dozen deploys pile onto a two-worker pool, each
/// one's budget sized for a lease that now waits behind eleven others, and
/// the timeouts that follow tear canvases down. Bounding in-flight deploys
/// turns that stampede into a queue — cards fill in one after another,
/// slower per card and far faster to a stable page.
///
/// `candidates` are `(slot_id, visible)` for the slots that passed the
/// caller's pending gate, `deploying_now` counts slots already mid-deploy
/// across the whole pool, and `max_concurrent_deploys` is the cap (the
/// pool size: one in-flight deploy per worker). Visible slots go first —
/// they are what the user is looking at — then ascending slot id, which is
/// lease order, so the queue is stable and nothing starves: a slot not
/// chosen this tick stays pending and is reconsidered on the next one.
pub fn choose_starts(
    mut candidates: Vec<(u64, bool)>,
    deploying_now: usize,
    max_concurrent_deploys: usize,
) -> Vec<u64> {
    let room = max_concurrent_deploys.saturating_sub(deploying_now);
    if room == 0 {
        return Vec::new();
    }
    candidates.sort_by_key(|&(slot_id, visible)| (!visible, slot_id));
    candidates.truncate(room);
    candidates.into_iter().map(|(slot_id, _)| slot_id).collect()
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
    fn detached_visible_slot_redeploys_instead_of_parking() {
        // Regression: an LRU-evicted slot that was still visible used to
        // park Suspended and freeze forever (no visibility edge left to
        // resume it).
        assert_eq!(detached_slot_next(true), DetachedSlotNext::Redeploy);
    }

    #[test]
    fn detached_invisible_slot_parks_until_its_visibility_edge() {
        assert_eq!(detached_slot_next(false), DetachedSlotNext::Park);
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

    #[test]
    fn starts_take_visible_slots_before_invisible_ones() {
        let candidates = vec![(1, false), (2, false), (3, true), (4, true)];
        assert_eq!(choose_starts(candidates, 0, 3), vec![3, 4, 1]);
    }

    #[test]
    fn starts_within_a_visibility_class_follow_lease_order() {
        let candidates = vec![(9, true), (2, true), (5, true)];
        assert_eq!(choose_starts(candidates, 0, 2), vec![2, 5]);
    }

    #[test]
    fn starts_leave_room_for_the_deploys_already_in_flight() {
        let candidates = vec![(1, true), (2, true), (3, true)];
        assert_eq!(choose_starts(candidates.clone(), 1, 2), vec![1]);
        assert_eq!(choose_starts(candidates, 2, 2), Vec::<u64>::new());
    }

    #[test]
    fn starts_stop_at_a_saturated_pool() {
        // More in flight than the cap allows (the cap shrank, or a recycle
        // left extra deploys behind): no new starts, and no underflow.
        let candidates = vec![(1, true)];
        assert_eq!(choose_starts(candidates, 5, 2), Vec::<u64>::new());
    }

    #[test]
    fn starts_with_nothing_pending_are_empty() {
        assert_eq!(choose_starts(Vec::new(), 0, 2), Vec::<u64>::new());
    }

    #[test]
    fn starts_are_capped_below_what_is_pending() {
        let candidates = vec![(1, true), (2, true), (3, true), (4, true)];
        assert_eq!(choose_starts(candidates, 0, 2), vec![1, 2]);
    }
}
