//! Pure pool policies: least-loaded worker choice, LRU eviction, which
//! pending leases may start deploying, which pool member boots next, and
//! when a dead pool worker may be re-booted.
//!
//! Kept browser-free so the decisions the host makes under budget pressure
//! are unit-testable natively.

/// How many times a DEAD pool worker may be re-booted before the pool
/// member is given up on for the life of the page.
///
/// A boot failure is usually transient — a wasm fetch that lost its race
/// with two sibling boots, a WebGPU init that came up during a hiccup —
/// so the pool recovers rather than staying dark until a reload. It is
/// bounded so a genuinely broken environment (no WebGPU, a 404 on the
/// engine) fails honestly instead of flapping forever.
pub const DEAD_WORKER_RETRY_BUDGET: usize = 3;

/// Cooldown before each retry, indexed by retries already spent: a quick
/// second chance, then long enough apart that a broken environment costs
/// almost nothing while an idle page stays idle (retries are taken on
/// demand, never on a timer).
pub const DEAD_WORKER_RETRY_BACKOFF_MS: [f64; DEAD_WORKER_RETRY_BUDGET] =
    [2_000.0, 8_000.0, 30_000.0];

/// Whether the host may START background preview work this tick.
///
/// The hold is a *deferral*, not a cancellation: nothing in flight is
/// touched and nothing queues up behind the gate — every decision it
/// declines is simply taken again on the next tick, so the pool resumes by
/// itself the moment the gate opens.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StartGate {
    /// Nothing is competing for the engine: start what the budgets allow.
    Open,
    /// A user-initiated open is in flight and owns the engine's
    /// fetch/compile budget: start nothing new until it settles.
    HoldForUserOpen,
}

impl StartGate {
    /// The gate a scheduler tick runs under, read off the page's
    /// open-in-flight signal ([`crate::user_open_in_flight`]).
    pub fn from_user_open(user_open_in_flight: bool) -> Self {
        if user_open_in_flight {
            Self::HoldForUserOpen
        } else {
            Self::Open
        }
    }

    fn holds(self) -> bool {
        matches!(self, Self::HoldForUserOpen)
    }
}

/// One pool entry as the boot scheduler sees it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BootSlotState {
    /// Never started: waiting its turn in the staggered boot queue.
    Pending,
    /// A boot (first start, respawn, or dead-worker retry) is in flight.
    Booting,
    /// Booted and serving leases.
    Ready,
    /// Boot failed; whether it may try again is [`dead_worker_next`]'s
    /// business, not this scheduler's.
    Dead,
}

/// Whether the pool may start ANY boot this tick.
///
/// One boot at a time, page-wide: the pool members all fetch and compile
/// the *same* multi-MB engine wasm, so starting them together turns one
/// download into N racing downloads, each slower than the single one would
/// have been — which is exactly what made a boot budget expire on a cold,
/// throttled load. Serialized, the second boot also finds the first one's
/// bytes already in the HTTP cache.
pub fn pool_may_start_boot(states: &[BootSlotState], gate: StartGate) -> bool {
    !gate.holds() && !states.contains(&BootSlotState::Booting)
}

/// Which pool member starts booting this tick, if any: the lowest-indexed
/// entry that has never started, once [`pool_may_start_boot`] allows it.
///
/// A `Dead` entry does not block its successors — its own recovery is
/// demand-driven and cooled down ([`dead_worker_next`]) — so a pool whose
/// first member failed still fills in behind it.
pub fn choose_boot_start(states: &[BootSlotState], gate: StartGate) -> Option<usize> {
    if !pool_may_start_boot(states, gate) {
        return None;
    }
    states
        .iter()
        .position(|state| *state == BootSlotState::Pending)
}

/// What a dead pool worker's entry should do next.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DeadWorkerNext {
    /// Budget remains and this attempt's cooldown has elapsed: re-boot.
    Reboot,
    /// Budget remains, but the cooldown is still running: leave it dead
    /// and reconsider on a later tick.
    Cooling,
    /// Every retry has been spent — this worker is final for the page.
    Spent,
}

/// Decide a dead worker's future from the retries it has already spent
/// and how long it has been dead (`now_ms` and `dead_since_ms` in the
/// caller's clock).
///
/// This is the whole "never a retry flap" guarantee, made explicit: the
/// budget bounds the attempts and [`DEAD_WORKER_RETRY_BACKOFF_MS`] spaces
/// them out. A clock that moved backwards reads as still cooling.
pub fn dead_worker_next(retries_used: usize, dead_since_ms: f64, now_ms: f64) -> DeadWorkerNext {
    let Some(cooldown_ms) = DEAD_WORKER_RETRY_BACKOFF_MS.get(retries_used) else {
        return DeadWorkerNext::Spent;
    };
    if now_ms - dead_since_ms >= *cooldown_ms {
        DeadWorkerNext::Reboot
    } else {
        DeadWorkerNext::Cooling
    }
}

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
/// across the whole pool, `max_concurrent_deploys` is the cap (the pool
/// size: one in-flight deploy per worker), and `gate` is the tick's
/// open-priority verdict — a user-initiated open starts no background
/// deploys at all, and the deploys already running are left to finish.
/// Visible slots go first — they are what the user is looking at — then
/// ascending slot id, which is lease order, so the queue is stable and
/// nothing starves: a slot not chosen this tick stays pending and is
/// reconsidered on the next one.
pub fn choose_starts(
    mut candidates: Vec<(u64, bool)>,
    deploying_now: usize,
    max_concurrent_deploys: usize,
    gate: StartGate,
) -> Vec<u64> {
    if gate.holds() {
        return Vec::new();
    }
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
        assert_eq!(
            choose_starts(candidates, 0, 3, StartGate::Open),
            vec![3, 4, 1]
        );
    }

    #[test]
    fn starts_within_a_visibility_class_follow_lease_order() {
        let candidates = vec![(9, true), (2, true), (5, true)];
        assert_eq!(choose_starts(candidates, 0, 2, StartGate::Open), vec![2, 5]);
    }

    #[test]
    fn starts_leave_room_for_the_deploys_already_in_flight() {
        let candidates = vec![(1, true), (2, true), (3, true)];
        assert_eq!(
            choose_starts(candidates.clone(), 1, 2, StartGate::Open),
            vec![1]
        );
        assert_eq!(
            choose_starts(candidates, 2, 2, StartGate::Open),
            Vec::<u64>::new()
        );
    }

    #[test]
    fn starts_stop_at_a_saturated_pool() {
        // More in flight than the cap allows (the cap shrank, or a recycle
        // left extra deploys behind): no new starts, and no underflow.
        let candidates = vec![(1, true)];
        assert_eq!(
            choose_starts(candidates, 5, 2, StartGate::Open),
            Vec::<u64>::new()
        );
    }

    #[test]
    fn starts_with_nothing_pending_are_empty() {
        assert_eq!(
            choose_starts(Vec::new(), 0, 2, StartGate::Open),
            Vec::<u64>::new()
        );
    }

    #[test]
    fn starts_are_capped_below_what_is_pending() {
        let candidates = vec![(1, true), (2, true), (3, true), (4, true)];
        assert_eq!(choose_starts(candidates, 0, 2, StartGate::Open), vec![1, 2]);
    }

    #[test]
    fn no_lease_starts_while_a_user_open_is_in_flight() {
        // Open-priority: an idle pool with room to spare still starts
        // nothing while the user's click is being served.
        let candidates = vec![(1, true), (2, false)];
        assert_eq!(
            choose_starts(candidates, 0, 4, StartGate::HoldForUserOpen),
            Vec::<u64>::new()
        );
    }

    #[test]
    fn lease_starts_resume_by_themselves_when_the_open_settles() {
        // Nothing was queued or cancelled by the hold: the same candidates
        // are chosen on the next tick, in the same order.
        let candidates = vec![(1, true), (2, false)];
        assert_eq!(
            choose_starts(candidates.clone(), 0, 4, StartGate::HoldForUserOpen),
            Vec::<u64>::new()
        );
        assert_eq!(choose_starts(candidates, 0, 4, StartGate::Open), vec![1, 2]);
    }

    #[test]
    fn the_start_gate_follows_the_open_in_flight_signal() {
        assert_eq!(StartGate::from_user_open(false), StartGate::Open);
        assert_eq!(StartGate::from_user_open(true), StartGate::HoldForUserOpen);
    }

    #[test]
    fn the_pool_boots_one_member_at_a_time() {
        use BootSlotState::{Booting, Pending, Ready};
        // Nothing in flight: the first member starts.
        assert_eq!(
            choose_boot_start(&[Pending, Pending], StartGate::Open),
            Some(0)
        );
        // While it boots, its successor waits — three racing fetches of the
        // same multi-MB engine wasm is the boot storm this phase removes.
        assert_eq!(
            choose_boot_start(&[Booting, Pending], StartGate::Open),
            None
        );
        // Ready (or failed) frees the queue for the next one.
        assert_eq!(
            choose_boot_start(&[Ready, Pending, Pending], StartGate::Open),
            Some(1)
        );
    }

    #[test]
    fn a_failed_pool_member_does_not_block_the_boots_behind_it() {
        use BootSlotState::{Dead, Pending};
        // A Dead entry's own recovery is demand-driven and cooled down; the
        // rest of the pool must not wait on it.
        assert_eq!(
            choose_boot_start(&[Dead, Pending], StartGate::Open),
            Some(1)
        );
    }

    #[test]
    fn a_fully_started_pool_starts_nothing_more() {
        use BootSlotState::{Dead, Ready};
        assert_eq!(choose_boot_start(&[Ready, Ready], StartGate::Open), None);
        assert_eq!(choose_boot_start(&[Ready, Dead], StartGate::Open), None);
        assert_eq!(choose_boot_start(&[], StartGate::Open), None);
    }

    #[test]
    fn no_boot_starts_while_a_user_open_is_in_flight() {
        use BootSlotState::{Dead, Pending};
        let states = [Pending, Pending];
        assert!(!pool_may_start_boot(&states, StartGate::HoldForUserOpen));
        assert_eq!(choose_boot_start(&states, StartGate::HoldForUserOpen), None);
        // The same hold covers a dead member's retry — a re-boot is a boot.
        assert!(!pool_may_start_boot(
            &[Dead, Dead],
            StartGate::HoldForUserOpen
        ));
    }

    #[test]
    fn boots_resume_from_where_they_stopped_when_the_open_settles() {
        use BootSlotState::{Pending, Ready};
        let states = [Ready, Pending];
        assert_eq!(choose_boot_start(&states, StartGate::HoldForUserOpen), None);
        assert_eq!(choose_boot_start(&states, StartGate::Open), Some(1));
        assert!(pool_may_start_boot(&states, StartGate::Open));
    }

    #[test]
    fn a_fresh_dead_worker_cools_before_its_first_retry() {
        let dead_at = 1_000.0;
        assert_eq!(
            dead_worker_next(0, dead_at, dead_at + 1.0),
            DeadWorkerNext::Cooling
        );
        assert_eq!(
            dead_worker_next(0, dead_at, dead_at + DEAD_WORKER_RETRY_BACKOFF_MS[0] - 1.0),
            DeadWorkerNext::Cooling
        );
    }

    #[test]
    fn a_dead_worker_reboots_once_its_cooldown_elapses() {
        let dead_at = 1_000.0;
        assert_eq!(
            dead_worker_next(0, dead_at, dead_at + DEAD_WORKER_RETRY_BACKOFF_MS[0]),
            DeadWorkerNext::Reboot
        );
    }

    #[test]
    fn each_retry_waits_longer_than_the_last() {
        // The Dead → cooldown → re-boot progression, walked rung by rung:
        // the same elapsed time that earned attempt n is not enough for
        // attempt n + 1.
        let dead_at = 1_000.0;
        for retries_used in 1..DEAD_WORKER_RETRY_BUDGET {
            let previous = DEAD_WORKER_RETRY_BACKOFF_MS[retries_used - 1];
            let current = DEAD_WORKER_RETRY_BACKOFF_MS[retries_used];
            assert!(current > previous);
            assert_eq!(
                dead_worker_next(retries_used, dead_at, dead_at + previous),
                DeadWorkerNext::Cooling
            );
            assert_eq!(
                dead_worker_next(retries_used, dead_at, dead_at + current),
                DeadWorkerNext::Reboot
            );
        }
    }

    #[test]
    fn a_dead_worker_that_spent_its_budget_never_reboots_again() {
        // However long it waits: budget exhausted is final for the page.
        assert_eq!(
            dead_worker_next(DEAD_WORKER_RETRY_BUDGET, 0.0, 1_000_000.0),
            DeadWorkerNext::Spent
        );
        assert_eq!(
            dead_worker_next(DEAD_WORKER_RETRY_BUDGET + 5, 0.0, f64::MAX),
            DeadWorkerNext::Spent
        );
    }

    #[test]
    fn a_clock_that_moved_backwards_keeps_the_worker_cooling() {
        assert_eq!(
            dead_worker_next(0, 10_000.0, 1_000.0),
            DeadWorkerNext::Cooling
        );
    }
}
