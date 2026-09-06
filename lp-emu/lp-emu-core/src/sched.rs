//! A discrete-event scheduler over guest cycles.
//!
//! Guest time in an SoC emulator is a schedule, not a clock: a timer alarm,
//! a UART character time, an RMT symbol boundary are all "wake me at cycle
//! N". Plan PD5 makes that explicit — **wall clock never enters the
//! machine** — so this scheduler is the only source of "later" and two runs
//! of the same image with the same scripted host input produce the same pop
//! order, byte for byte.
//!
//! Architecture-neutral on purpose: it knows cycles, not RISC-V, not MMIO.
//! An [`EventId`] is an opaque `u32` whose meaning belongs to whoever
//! scheduled it (the ESP bus splits it into a peripheral index and a local
//! event number; see `lp_emu_esp_common::bus`).
//!
//! # Determinism
//!
//! Every `schedule_*` call takes the next value of a monotone sequence
//! counter, and the heap orders by `(deadline, seq)`. Two events at the same
//! cycle therefore pop in the order they were scheduled — FIFO, never
//! "whatever the heap happened to do".
//!
//! # Cancellation
//!
//! [`Scheduler::cancel`] is lazy: it bumps a per-id epoch, and entries whose
//! epoch is stale are dropped when [`Scheduler::pop_due`] reaches them. That
//! makes cancel `O(log n)` instead of a heap rebuild, at the price of
//! [`Scheduler::next_deadline`] having to look past tombstones — which it
//! does by scanning, because the pending set is a handful of alarms, not a
//! data structure worth optimizing.
//!
//! Note the semantics this gives: `cancel(id)` cancels **every** pending
//! occurrence of `id`, which is what a peripheral wants ("stop my alarm"),
//! not "cancel the one I scheduled most recently".

use alloc::collections::{BTreeMap, BinaryHeap};
use core::cmp::Reverse;

/// Guest cycles. The unit the whole machine agrees on; a `CycleModel`
/// converts instructions to it, peripherals convert their own clocks to it.
pub type Cycles = u64;

/// An opaque event tag. Its meaning belongs to whoever scheduled it.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct EventId(pub u32);

/// One heap entry. Ordered by `(deadline, seq)`; `epoch` is the tombstone.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
struct Entry {
    at: Cycles,
    seq: u64,
    id: EventId,
    epoch: u64,
}

/// A deterministic discrete-event queue over guest cycles.
#[derive(Debug, Default)]
pub struct Scheduler {
    heap: BinaryHeap<Reverse<Entry>>,
    /// Current epoch per id. An entry is live iff its epoch matches; absent
    /// means epoch 0, so an id that was never cancelled needs no map entry.
    epoch: BTreeMap<EventId, u64>,
    seq: u64,
}

impl Scheduler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Schedule `id` to become due at cycle `at`.
    ///
    /// An `at` in the past is not an error: it is due immediately, and the
    /// next [`pop_due`](Self::pop_due) returns it. Peripherals that compute
    /// a deadline from a guest-programmed period rely on that rather than
    /// on the caller clamping.
    pub fn schedule_at(&mut self, at: Cycles, id: EventId) {
        let seq = self.seq;
        self.seq += 1;
        let epoch = self.epoch.get(&id).copied().unwrap_or(0);
        self.heap.push(Reverse(Entry { at, seq, id, epoch }));
    }

    /// Schedule `id` `delta` cycles after `now`. Saturating, so a peripheral
    /// programmed with a nonsense period parks at the end of time instead of
    /// wrapping into the past.
    pub fn schedule_in(&mut self, now: Cycles, delta: Cycles, id: EventId) {
        self.schedule_at(now.saturating_add(delta), id);
    }

    /// Cancel every pending occurrence of `id`.
    ///
    /// Lazy: the heap entries stay until they are reached. Scheduling `id`
    /// again after a cancel is fine — the new entry carries the new epoch.
    pub fn cancel(&mut self, id: EventId) {
        *self.epoch.entry(id).or_insert(0) += 1;
    }

    /// The earliest live deadline, or `None` when nothing is pending.
    ///
    /// Exact, not "the top of the heap": a cancelled entry at the front
    /// would otherwise make a `wfi` wake up for nothing. `O(pending)` with
    /// a pending set of a few alarms.
    pub fn next_deadline(&self) -> Option<Cycles> {
        self.heap
            .iter()
            .filter(|Reverse(e)| self.is_live(e))
            .map(|Reverse(e)| e.at)
            .min()
    }

    /// Pop the earliest event due at or before `now`, or `None`.
    ///
    /// Ties break FIFO by schedule order. Cancelled entries are discarded on
    /// the way past; that is the only place the heap shrinks for them.
    pub fn pop_due(&mut self, now: Cycles) -> Option<EventId> {
        while let Some(Reverse(top)) = self.heap.peek() {
            if top.at > now {
                return None;
            }
            let Reverse(entry) = self.heap.pop().expect("peeked");
            if self.is_live(&entry) {
                return Some(entry.id);
            }
        }
        None
    }

    /// Number of heap entries, live and tombstoned. Diagnostics and tests;
    /// a machine should not need it.
    pub fn pending(&self) -> usize {
        self.heap.len()
    }

    /// Number of live (not cancelled) heap entries.
    pub fn live(&self) -> usize {
        self.heap
            .iter()
            .filter(|Reverse(e)| self.is_live(e))
            .count()
    }

    /// Drop tombstoned entries. Never required for correctness — a machine
    /// that cancels a far-future alarm every frame can call it to keep the
    /// heap from growing.
    pub fn prune(&mut self) {
        let live: BinaryHeap<Reverse<Entry>> = self
            .heap
            .drain()
            .filter(|Reverse(e)| {
                let cur = self.epoch.get(&e.id).copied().unwrap_or(0);
                e.epoch == cur
            })
            .collect();
        self.heap = live;
    }

    /// Forget everything. Used by machine reset and by snapshot load.
    pub fn clear(&mut self) {
        self.heap.clear();
        self.epoch.clear();
        self.seq = 0;
    }

    fn is_live(&self, entry: &Entry) -> bool {
        self.epoch.get(&entry.id).copied().unwrap_or(0) == entry.epoch
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    fn drain(sched: &mut Scheduler, now: Cycles) -> Vec<u32> {
        let mut out = Vec::new();
        while let Some(EventId(id)) = sched.pop_due(now) {
            out.push(id);
        }
        out
    }

    #[test]
    fn pops_in_deadline_order() {
        let mut s = Scheduler::new();
        s.schedule_at(30, EventId(3));
        s.schedule_at(10, EventId(1));
        s.schedule_at(20, EventId(2));
        assert_eq!(drain(&mut s, 100), [1, 2, 3]);
    }

    #[test]
    fn ties_break_fifo_by_schedule_order() {
        let mut s = Scheduler::new();
        for id in 0..8 {
            s.schedule_at(50, EventId(id));
        }
        assert_eq!(drain(&mut s, 50), [0, 1, 2, 3, 4, 5, 6, 7]);
    }

    #[test]
    fn pop_due_is_inclusive_of_now_and_stops_after() {
        let mut s = Scheduler::new();
        s.schedule_at(10, EventId(1));
        s.schedule_at(11, EventId(2));
        assert_eq!(s.pop_due(9), None);
        assert_eq!(s.pop_due(10), Some(EventId(1)));
        assert_eq!(s.pop_due(10), None);
        assert_eq!(s.pop_due(11), Some(EventId(2)));
        assert_eq!(s.pop_due(11), None);
    }

    #[test]
    fn schedule_in_is_relative_and_saturates() {
        let mut s = Scheduler::new();
        s.schedule_in(100, 25, EventId(1));
        assert_eq!(s.next_deadline(), Some(125));
        s.schedule_in(u64::MAX - 1, 1000, EventId(2));
        assert_eq!(s.pop_due(u64::MAX), Some(EventId(1)));
        assert_eq!(s.pop_due(u64::MAX), Some(EventId(2)));
    }

    #[test]
    fn a_past_deadline_is_due_immediately() {
        let mut s = Scheduler::new();
        s.schedule_at(5, EventId(7));
        assert_eq!(s.pop_due(1000), Some(EventId(7)));
    }

    #[test]
    fn cancel_drops_every_pending_occurrence() {
        let mut s = Scheduler::new();
        s.schedule_at(10, EventId(1));
        s.schedule_at(20, EventId(1));
        s.schedule_at(15, EventId(2));
        s.cancel(EventId(1));
        assert_eq!(drain(&mut s, 100), [2]);
    }

    #[test]
    fn cancel_then_reschedule_keeps_the_new_entry() {
        let mut s = Scheduler::new();
        s.schedule_at(10, EventId(1));
        s.cancel(EventId(1));
        s.schedule_at(30, EventId(1));
        assert_eq!(s.next_deadline(), Some(30));
        assert_eq!(drain(&mut s, 100), [1]);
    }

    #[test]
    fn next_deadline_looks_past_tombstones() {
        let mut s = Scheduler::new();
        s.schedule_at(10, EventId(1));
        s.schedule_at(40, EventId(2));
        s.cancel(EventId(1));
        // The cancelled entry is still at the front of the heap; a `wfi`
        // that trusted the raw top would wake at 10 for nothing.
        assert_eq!(s.next_deadline(), Some(40));
        assert_eq!(s.live(), 1);
        assert_eq!(s.pending(), 2);
    }

    #[test]
    fn prune_drops_tombstones_without_changing_order() {
        let mut s = Scheduler::new();
        s.schedule_at(10, EventId(1));
        s.schedule_at(20, EventId(2));
        s.schedule_at(20, EventId(3));
        s.cancel(EventId(1));
        s.prune();
        assert_eq!(s.pending(), 2);
        assert_eq!(drain(&mut s, 100), [2, 3]);
    }

    #[test]
    fn empty_scheduler_has_no_deadline() {
        let mut s = Scheduler::new();
        assert_eq!(s.next_deadline(), None);
        assert_eq!(s.pop_due(u64::MAX), None);
        s.schedule_at(1, EventId(1));
        s.cancel(EventId(1));
        assert_eq!(s.next_deadline(), None);
    }

    #[test]
    fn clear_forgets_everything() {
        let mut s = Scheduler::new();
        s.schedule_at(10, EventId(1));
        s.cancel(EventId(1));
        s.clear();
        s.schedule_at(10, EventId(1));
        assert_eq!(s.pop_due(10), Some(EventId(1)));
    }

    /// The property PD5 rests on: the same calls give the same pop order.
    #[test]
    fn identical_call_sequences_pop_identically() {
        fn run() -> Vec<u32> {
            let mut s = Scheduler::new();
            let mut out = Vec::new();
            for (at, id) in [(7, 4), (3, 1), (7, 2), (3, 9), (11, 0)] {
                s.schedule_at(at, EventId(id));
            }
            s.cancel(EventId(9));
            for now in [3, 7, 11] {
                while let Some(EventId(id)) = s.pop_due(now) {
                    out.push(id);
                }
            }
            out
        }
        assert_eq!(run(), run());
        assert_eq!(run(), [1, 4, 2, 0]);
    }
}
