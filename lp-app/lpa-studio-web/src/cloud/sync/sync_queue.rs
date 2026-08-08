//! When a project is due for a trip to the service — the whole scheduling
//! policy, with no IO in it.
//!
//! `lpa-cloud-client` deliberately has no retry loop and no scheduler: every
//! operation is one attempt with a typed outcome. This is the other half —
//! the bookkeeping that turns "a project changed" into "go now / go in two
//! seconds / go again in a minute", written as a plain state machine so it
//! can be tested without a browser, a clock, or a service.
//!
//! The queue holds no history and is never persisted. OPFS is the truth
//! (binding present? heads behind?), so a lost queue costs at most one
//! deferred trip: the next save, the next sign-in, or the coarse sweep
//! re-derives everything worth doing.

use std::collections::BTreeMap;

/// Rapid saves collapse into one push (the `asset_editor` debounce cadence).
pub const SAVE_DEBOUNCE_MS: f64 = 2_000.0;

/// The coarse retry cadence. A failed trip is not queued anywhere durable;
/// it simply stays in the map until this comes around.
pub const RETRY_DELAY_MS: f64 = 60_000.0;

/// Why a project wants a trip to the service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncTrigger {
    /// A catalog transaction created or altered the project (create,
    /// duplicate, import, device adopt, upgrade…). Goes immediately: the
    /// first publish is what makes the address in the bar real (D2/D3).
    Installed,
    /// The project was renamed — the cloud slug must be restated, so the
    /// trip re-publishes rather than merely pushing.
    Renamed,
    /// A save landed in the library copy. Debounced.
    Saved,
    /// The sign-in sweep, or the coarse retry timer.
    Swept,
}

impl SyncTrigger {
    /// How long after the trigger the trip may run.
    fn delay_ms(self) -> f64 {
        match self {
            SyncTrigger::Saved => SAVE_DEBOUNCE_MS,
            SyncTrigger::Installed | SyncTrigger::Renamed | SyncTrigger::Swept => 0.0,
        }
    }

    /// Whether this trigger means the project's cloud identity (its slug)
    /// has to be restated, not just its content pushed.
    fn restates(self) -> bool {
        matches!(self, SyncTrigger::Renamed)
    }
}

/// How a trip ended, as far as scheduling cares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TripResult {
    /// The service and the local copy agree now.
    Settled,
    /// Something retryable got in the way (offline, a gateway). Try again
    /// on the coarse timer.
    Retry,
    /// The service considered the request and refused in a way that
    /// repeating will not fix (not ours, malformed slug, archived). Drop it
    /// — the next save or sign-in will ask again if the situation changed.
    Refused,
}

/// One project that is due, and what its trip must do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DueProject {
    pub uid: String,
    /// Re-publish (restating the slug) rather than push.
    pub restate: bool,
}

/// Per-uid scheduling state for the auto-publish driver.
#[derive(Debug, Default)]
pub struct SyncQueue {
    entries: BTreeMap<String, Entry>,
}

impl SyncQueue {
    pub fn new() -> Self {
        Self::default()
    }

    /// Note that `uid` wants a trip, and answer how long the caller should
    /// wait before pumping (the debounce arm).
    ///
    /// Re-requesting a uid whose trip is already in flight does not cancel
    /// it — the trip is one attempt against a consistent snapshot — it marks
    /// the entry so the pump comes back for it afterwards.
    pub fn request(&mut self, uid: &str, trigger: SyncTrigger, now: f64) -> f64 {
        let delay = trigger.delay_ms();
        let entry = self.entries.entry(uid.to_string()).or_default();
        entry.due_at = now + delay;
        entry.restate |= trigger.restates();
        entry.pending = true;
        delay
    }

    /// The projects whose wait is over, marked in flight.
    ///
    /// Deliberately not concurrency-capped here: the driver runs the
    /// returned list sequentially, which is the cap.
    pub fn take_due(&mut self, now: f64) -> Vec<DueProject> {
        let mut due = Vec::new();
        for (uid, entry) in self.entries.iter_mut() {
            if entry.in_flight || !entry.pending || entry.due_at > now {
                continue;
            }
            entry.in_flight = true;
            entry.pending = false;
            due.push(DueProject {
                uid: uid.clone(),
                // The restate obligation travels with the trip; a rename
                // arriving mid-flight re-sets it on the entry.
                restate: core::mem::take(&mut entry.restate),
            });
        }
        due
    }

    /// Record how a trip ended and re-arm (or forget) the project.
    pub fn finish(&mut self, uid: &str, result: TripResult, now: f64) {
        let Some(entry) = self.entries.get_mut(uid) else {
            return;
        };
        entry.in_flight = false;
        match result {
            TripResult::Settled => {}
            TripResult::Retry => {
                entry.pending = true;
                entry.due_at = now + RETRY_DELAY_MS;
            }
            TripResult::Refused => {
                // A refusal that repeating will not fix: forget the request,
                // but keep anything that arrived while the trip was running.
                entry.restate = false;
            }
        }
        if !entry.pending {
            self.entries.remove(uid);
        }
    }

    /// Everything still waiting or in flight — the driver's "is there work"
    /// question, and what the coarse timer re-arms.
    pub fn tracked_uids(&self) -> Vec<String> {
        self.entries.keys().cloned().collect()
    }

    /// Whether anything is outstanding at all.
    pub fn is_idle(&self) -> bool {
        self.entries.is_empty()
    }

    /// Re-arm every tracked project for an immediate attempt — the coarse
    /// sweep, which is how a failed trip eventually gets its retry.
    pub fn arm_all(&mut self, now: f64) {
        for entry in self.entries.values_mut() {
            if !entry.in_flight {
                entry.pending = true;
                entry.due_at = now;
            }
        }
    }

    /// Forget everything (sign-out): the cloud is not ours to converge on
    /// anymore, and a stale queue would push the next account's first pump.
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

#[derive(Debug, Default)]
struct Entry {
    /// Earliest time this uid may go to the service.
    due_at: f64,
    /// A trip is wanted (as opposed to an entry kept only because a trip is
    /// in flight).
    pending: bool,
    /// A trip is running right now.
    in_flight: bool,
    /// The next trip must restate the project's slug.
    restate: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_installed_project_is_due_immediately() {
        let mut queue = SyncQueue::new();
        assert_eq!(queue.request("prj1", SyncTrigger::Installed, 0.0), 0.0);
        assert_eq!(
            queue.take_due(0.0),
            vec![DueProject {
                uid: "prj1".to_string(),
                restate: false,
            }]
        );
    }

    /// The debounce: three saves in a burst produce one trip, and it runs
    /// two seconds after the LAST of them, not the first.
    #[test]
    fn rapid_saves_collapse_into_one_trip() {
        let mut queue = SyncQueue::new();
        queue.request("prj1", SyncTrigger::Saved, 0.0);
        queue.request("prj1", SyncTrigger::Saved, 500.0);
        queue.request("prj1", SyncTrigger::Saved, 900.0);

        assert!(
            queue.take_due(2_000.0).is_empty(),
            "still inside the window"
        );
        assert_eq!(queue.take_due(2_900.0).len(), 1);
        assert!(queue.take_due(9_999.0).is_empty(), "exactly one trip");
    }

    #[test]
    fn a_rename_asks_the_trip_to_restate_the_slug() {
        let mut queue = SyncQueue::new();
        queue.request("prj1", SyncTrigger::Renamed, 0.0);
        assert!(queue.take_due(0.0)[0].restate);
    }

    /// The restate obligation survives a save arriving on top of a rename —
    /// it is a property of the project, not of the newest trigger.
    #[test]
    fn a_save_after_a_rename_still_restates() {
        let mut queue = SyncQueue::new();
        queue.request("prj1", SyncTrigger::Renamed, 0.0);
        queue.request("prj1", SyncTrigger::Saved, 100.0);
        assert!(queue.take_due(0.0).is_empty(), "the save re-armed the wait");
        assert!(queue.take_due(2_100.0)[0].restate);
    }

    #[test]
    fn a_settled_trip_leaves_nothing_behind() {
        let mut queue = SyncQueue::new();
        queue.request("prj1", SyncTrigger::Installed, 0.0);
        queue.take_due(0.0);
        queue.finish("prj1", TripResult::Settled, 0.0);
        assert!(queue.is_idle());
    }

    /// A failed trip is retried on the coarse timer and nowhere sooner.
    #[test]
    fn a_retryable_failure_comes_back_on_the_coarse_timer() {
        let mut queue = SyncQueue::new();
        queue.request("prj1", SyncTrigger::Installed, 0.0);
        queue.take_due(0.0);
        queue.finish("prj1", TripResult::Retry, 0.0);

        assert!(queue.take_due(30_000.0).is_empty());
        assert_eq!(queue.take_due(60_000.0).len(), 1);
    }

    /// …and a save in the meantime does not wait a minute for it.
    #[test]
    fn a_save_pre_empts_a_pending_retry() {
        let mut queue = SyncQueue::new();
        queue.request("prj1", SyncTrigger::Installed, 0.0);
        queue.take_due(0.0);
        queue.finish("prj1", TripResult::Retry, 0.0);

        queue.request("prj1", SyncTrigger::Saved, 1_000.0);
        assert_eq!(queue.take_due(3_000.0).len(), 1);
    }

    #[test]
    fn a_terminal_refusal_is_forgotten() {
        let mut queue = SyncQueue::new();
        queue.request("prj1", SyncTrigger::Installed, 0.0);
        queue.take_due(0.0);
        queue.finish("prj1", TripResult::Refused, 0.0);
        assert!(queue.is_idle());
        assert!(queue.take_due(1e9).is_empty());
    }

    /// A save that lands while the trip is in flight must not be swallowed:
    /// the trip is one attempt against the snapshot it started from.
    #[test]
    fn work_arriving_mid_flight_earns_another_trip() {
        let mut queue = SyncQueue::new();
        queue.request("prj1", SyncTrigger::Installed, 0.0);
        assert_eq!(queue.take_due(0.0).len(), 1);

        queue.request("prj1", SyncTrigger::Saved, 100.0);
        assert!(queue.take_due(5_000.0).is_empty(), "not while in flight");

        queue.finish("prj1", TripResult::Settled, 5_000.0);
        assert_eq!(queue.take_due(5_000.0).len(), 1);
    }

    /// The sweep re-arms what is still tracked — the retry path a coarse
    /// timer drives — without disturbing a trip that is running.
    #[test]
    fn arming_all_skips_projects_already_in_flight() {
        let mut queue = SyncQueue::new();
        queue.request("prj1", SyncTrigger::Installed, 0.0);
        queue.request("prj2", SyncTrigger::Installed, 0.0);
        queue.take_due(0.0);
        queue.finish("prj1", TripResult::Retry, 0.0);
        // prj2's trip is still running.

        queue.arm_all(10.0);
        assert_eq!(
            queue.take_due(10.0),
            vec![DueProject {
                uid: "prj1".to_string(),
                restate: false,
            }]
        );
    }

    #[test]
    fn clearing_forgets_everything() {
        let mut queue = SyncQueue::new();
        queue.request("prj1", SyncTrigger::Saved, 0.0);
        queue.clear();
        assert!(queue.is_idle());
        assert!(queue.tracked_uids().is_empty());
    }
}
