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
//!
//! What it does hold, per tab, is the little it needs to keep the coarse
//! sweep cheap: which uids it has already settled with the service since
//! their last trigger (so a tick does not re-push an up-to-date library —
//! an up-to-date push still costs two round trips and a snapshot), which
//! are latched off by a denial, and which were refused and are waiting out
//! a backoff. Everything else in the library is fair game for the tick —
//! that is how a project whose only trip was dropped, or that was installed
//! before the account was known, still gets its first publish
//! (`docs/defects/2026-08-28-auto-publish-outcomes-invisible.md`).

use std::collections::{BTreeMap, BTreeSet};

/// Rapid saves collapse into one push (the `asset_editor` debounce cadence).
pub const SAVE_DEBOUNCE_MS: f64 = 2_000.0;

/// The coarse retry cadence. A failed trip is not queued anywhere durable;
/// it simply stays in the map until this comes around.
pub const RETRY_DELAY_MS: f64 = 60_000.0;

/// How long a refused trip waits before the coarse sweep may offer the
/// project again. Long, because a refusal is an answer that repeating soon
/// will not change — but not forever, because "the situation changed" has
/// more causes than a save or a sign-in (a library host that was not there
/// yet, a lock held during the open that followed the create). An explicit
/// trigger (save, rename) pre-empts it.
pub const REFUSED_BACKOFF_MS: f64 = 5.0 * 60_000.0;

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
    /// repeating will not fix (not ours, malformed slug, archived), or the
    /// local state could not be read. Backed off, not dropped: the next
    /// save or sign-in asks again at once, and the coarse sweep asks again
    /// after [`REFUSED_BACKOFF_MS`] in case the situation changed.
    Refused,
    /// The service knows the project and this caller may not write it — a
    /// visitor's push against a `View` link, or a session that expired
    /// mid-trip. Terminal **and latched** (P6): unlike [`Self::Refused`],
    /// later saves must not keep asking a server that will keep saying no.
    /// The latch clears only when something that could change the answer is
    /// observed — an access/membership change on a pull, or a sign-in.
    Denied,
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
    /// Uids whose last trip was [`TripResult::Denied`]: requests for them
    /// are suppressed until [`Self::clear_denied`] (an observed access
    /// change) or [`Self::clear`] (sign-out / account switch).
    denied: BTreeSet<String>,
    /// Uids whose last trip settled and that nothing has asked about since.
    /// The coarse sweep leaves these alone — the service and the library
    /// agreed, and any later change fires its own trigger — which is what
    /// keeps a tick over a large, quiet library free of traffic.
    settled: BTreeSet<String>,
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
        // A denied uid stays quiet whatever the trigger: the server said
        // "not yours to write" and repeating the question is churn. The
        // latch is lifted by an observed change, never by another save.
        if self.denied.contains(uid) {
            return delay;
        }
        self.settled.remove(uid);
        let entry = self.entries.entry(uid.to_string()).or_default();
        entry.due_at = now + delay;
        entry.restate |= trigger.restates();
        entry.pending = true;
        entry.backing_off = false;
        delay
    }

    /// The coarse sweep, re-derived from the library: `library` is every
    /// project uid the library holds right now.
    ///
    /// Offers everything the queue has no current verdict on — a project
    /// installed before the account was known, one whose only trip was
    /// dropped, one this tab has simply never looked at — and re-arms the
    /// retries it is tracking, without touching the ones that settled, the
    /// ones a denial latched, or the ones still waiting out a refusal's
    /// backoff. Forgets settled verdicts for projects the library no
    /// longer has.
    pub fn sweep<'a>(&mut self, library: impl IntoIterator<Item = &'a str>, now: f64) {
        let library: BTreeSet<&str> = library.into_iter().collect();
        self.settled.retain(|uid| library.contains(uid.as_str()));
        for uid in library {
            if self.denied.contains(uid) || self.settled.contains(uid) {
                continue;
            }
            match self.entries.get_mut(uid) {
                Some(entry) => {
                    if !entry.in_flight && !entry.backing_off {
                        entry.pending = true;
                        entry.due_at = now;
                    }
                }
                None => {
                    self.request(uid, SyncTrigger::Swept, now);
                }
            }
        }
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
            TripResult::Settled => {
                if !entry.pending {
                    self.settled.insert(uid.to_string());
                }
            }
            TripResult::Retry => {
                entry.pending = true;
                entry.due_at = now + RETRY_DELAY_MS;
            }
            TripResult::Refused => {
                // A refusal that repeating soon will not fix: wait out the
                // backoff before the sweep may ask again — unless work
                // arrived while the trip was running, which asks on its own
                // schedule and stands.
                entry.restate = false;
                if !entry.pending {
                    entry.pending = true;
                    entry.due_at = now + REFUSED_BACKOFF_MS;
                    entry.backing_off = true;
                }
            }
            TripResult::Denied => {
                // Latch: drop the entry outright — including work that
                // arrived mid-flight, which the same answer awaits — and
                // suppress every request until the latch is cleared.
                entry.pending = false;
                self.denied.insert(uid.to_string());
            }
        }
        if !entry.pending {
            self.entries.remove(uid);
        }
    }

    /// Whether a uid's pushes are currently latched off by a denial.
    pub fn is_denied(&self, uid: &str) -> bool {
        self.denied.contains(uid)
    }

    /// Lift a uid's denial latch — called when a pull observed an access
    /// or membership change that could make the server's answer different.
    /// Returns whether there was a latch to lift, so the caller knows to
    /// re-offer the project.
    pub fn clear_denied(&mut self, uid: &str) -> bool {
        self.denied.remove(uid)
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

    /// Forget everything (sign-out): the cloud is not ours to converge on
    /// anymore, and a stale queue would push the next account's first pump.
    /// Denials and settled verdicts go too — they were answers to a caller
    /// that no longer exists, and the next account deserves its own first
    /// answer.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.denied.clear();
        self.settled.clear();
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
    /// The wait is a refusal's backoff: the sweep must not shorten it (an
    /// explicit trigger may).
    backing_off: bool,
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

    /// The bug this pins (`docs/defects/2026-08-28-auto-publish-outcomes-invisible.md`,
    /// 2026-09-06 finding): a freshly generated project's install-time trip
    /// is its ONLY trip, and a refusal used to forget the entry outright —
    /// the coarse timer only re-armed what was still tracked, so the
    /// project never reached the cloud until a save happened to land.
    /// Now a refusal backs off, and the sweep offers the project again
    /// once the backoff is over.
    #[test]
    fn a_refused_project_is_offered_again_after_the_backoff() {
        let mut queue = SyncQueue::new();
        queue.request("prj1", SyncTrigger::Installed, 0.0);
        queue.take_due(0.0);
        queue.finish("prj1", TripResult::Refused, 0.0);
        assert!(!queue.is_idle(), "still tracked");

        queue.sweep(["prj1"], 60_000.0);
        assert!(
            queue.take_due(60_000.0).is_empty(),
            "the sweep does not shorten a refusal's backoff"
        );
        queue.sweep(["prj1"], REFUSED_BACKOFF_MS);
        assert_eq!(queue.take_due(REFUSED_BACKOFF_MS).len(), 1);
    }

    /// …but a save does not wait five minutes for it: an explicit trigger
    /// pre-empts the backoff, the way it pre-empts a retry.
    #[test]
    fn a_save_pre_empts_a_refusals_backoff() {
        let mut queue = SyncQueue::new();
        queue.request("prj1", SyncTrigger::Installed, 0.0);
        queue.take_due(0.0);
        queue.finish("prj1", TripResult::Refused, 0.0);

        queue.request("prj1", SyncTrigger::Saved, 1_000.0);
        assert_eq!(queue.take_due(3_000.0).len(), 1);
    }

    /// The other half of the same bug: a project installed while the
    /// account was still unknown (`note` is a no-op signed out) is in the
    /// library and in no queue. The sweep re-derives from the library, so
    /// the next tick offers it.
    #[test]
    fn the_sweep_offers_a_project_the_queue_has_never_seen() {
        let mut queue = SyncQueue::new();
        queue.sweep(["prj1"], 0.0);
        assert_eq!(
            queue.take_due(0.0),
            vec![DueProject {
                uid: "prj1".to_string(),
                restate: false,
            }]
        );
    }

    /// What keeps the tick cheap: a project whose trip settled is not
    /// offered again by the sweep — an up-to-date push still costs two round
    /// trips — until something asks about it.
    #[test]
    fn the_sweep_leaves_settled_projects_alone() {
        let mut queue = SyncQueue::new();
        queue.request("prj1", SyncTrigger::Installed, 0.0);
        queue.take_due(0.0);
        queue.finish("prj1", TripResult::Settled, 0.0);

        queue.sweep(["prj1"], 60_000.0);
        assert!(queue.take_due(60_000.0).is_empty(), "settled stays settled");

        queue.request("prj1", SyncTrigger::Saved, 61_000.0);
        assert_eq!(queue.take_due(63_000.0).len(), 1, "a save asks again");
        queue.finish("prj1", TripResult::Retry, 63_000.0);
        queue.sweep(["prj1"], 64_000.0);
        assert_eq!(queue.take_due(64_000.0).len(), 1, "a retry is re-armed");
    }

    /// A settled verdict follows the project out of the library: if the uid
    /// comes back (re-imported), it is a new project to the sweep.
    #[test]
    fn the_sweep_forgets_verdicts_for_projects_the_library_lost() {
        let mut queue = SyncQueue::new();
        queue.request("prj1", SyncTrigger::Installed, 0.0);
        queue.take_due(0.0);
        queue.finish("prj1", TripResult::Settled, 0.0);

        queue.sweep(["prj2"], 1.0);
        assert_eq!(queue.take_due(1.0)[0].uid, "prj2");
        queue.sweep(["prj1", "prj2"], 2.0);
        assert_eq!(queue.take_due(2.0)[0].uid, "prj1");
    }

    /// A save that lands while the trip is in flight must not be swallowed:
    /// the trip is one attempt against the snapshot it started from.
    ///
    /// Load-bearing since D1 (P2): the driver now publishes from a copy
    /// taken under the project lock and released before the network, so a
    /// project *can* change mid-publish where it could not before. This is
    /// the whole of what happens when it does — the trip that is running
    /// stands, and the change earns its own.
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
    fn the_sweep_skips_projects_already_in_flight() {
        let mut queue = SyncQueue::new();
        queue.request("prj1", SyncTrigger::Installed, 0.0);
        queue.request("prj2", SyncTrigger::Installed, 0.0);
        queue.take_due(0.0);
        queue.finish("prj1", TripResult::Retry, 0.0);
        // prj2's trip is still running.

        queue.sweep(["prj1", "prj2"], 10.0);
        assert_eq!(
            queue.take_due(10.0),
            vec![DueProject {
                uid: "prj1".to_string(),
                restate: false,
            }]
        );
    }

    /// The P6 latch: a denied push stops the conversation. Saves keep
    /// landing locally, but none of them asks the server again.
    #[test]
    fn a_denied_push_latches_and_later_saves_stay_quiet() {
        let mut queue = SyncQueue::new();
        queue.request("prj1", SyncTrigger::Saved, 0.0);
        queue.take_due(2_000.0);
        queue.finish("prj1", TripResult::Denied, 2_000.0);
        assert!(queue.is_idle());
        assert!(queue.is_denied("prj1"));

        queue.request("prj1", SyncTrigger::Saved, 3_000.0);
        queue.request("prj1", SyncTrigger::Swept, 4_000.0);
        queue.sweep(["prj1"], 5_000.0);
        assert!(queue.take_due(1e9).is_empty(), "the latch holds");
    }

    /// Lifting the latch (an access change observed on a pull) lets the
    /// next request through — and only lifting it does.
    #[test]
    fn clearing_a_denial_reopens_the_door() {
        let mut queue = SyncQueue::new();
        queue.request("prj1", SyncTrigger::Saved, 0.0);
        queue.take_due(2_000.0);
        queue.finish("prj1", TripResult::Denied, 2_000.0);

        assert!(queue.clear_denied("prj1"));
        assert!(!queue.is_denied("prj1"));
        assert!(!queue.clear_denied("prj1"), "already lifted");

        queue.request("prj1", SyncTrigger::Saved, 3_000.0);
        assert_eq!(queue.take_due(5_000.0).len(), 1);
    }

    /// A denial latched mid-flight swallows the mid-flight work too: the
    /// arrived save would get the same answer.
    #[test]
    fn work_arriving_mid_denied_flight_is_dropped() {
        let mut queue = SyncQueue::new();
        queue.request("prj1", SyncTrigger::Saved, 0.0);
        queue.take_due(2_000.0);
        queue.request("prj1", SyncTrigger::Saved, 2_100.0);
        queue.finish("prj1", TripResult::Denied, 2_500.0);
        assert!(queue.is_idle());
        assert!(queue.take_due(1e9).is_empty());
    }

    /// Sign-out forgets denials with everything else — the next account's
    /// pushes deserve their own first answer.
    #[test]
    fn clearing_forgets_denials_too() {
        let mut queue = SyncQueue::new();
        queue.request("prj1", SyncTrigger::Saved, 0.0);
        queue.take_due(2_000.0);
        queue.finish("prj1", TripResult::Denied, 2_000.0);
        queue.clear();
        assert!(!queue.is_denied("prj1"));
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
