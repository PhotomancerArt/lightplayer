//! The driver: browser timers, OPFS mounts, and `FetchCloudPort` on one
//! side; [`SyncQueue`] and [`run_trip`] on the other.
//!
//! One per tab, created on first use and never torn down. It owns no UI and
//! reports nothing: the only visible consequence of this module working is
//! that a project the user never explicitly shared already has a working
//! link, and the only visible consequence of it failing is that the link
//! lags (see the crate's P5 for the affordances that do speak).
//!
//! # Where the triggers come from
//!
//! - **Every catalog transaction** that produced a package
//!   (`library_host_opfs::catalog`) — create, duplicate, import, seed,
//!   generate, device-adopt, upgrade, rename. One request per transaction,
//!   which is what keeps a single create from publishing twice even though
//!   several op variants funnel through `install_package`.
//! - **Every save** (`library_host_opfs::notify_saved`), debounced.
//! - **Sign-in**, which sweeps the whole library.
//! - **A coarse timer**, which is the retry path for everything the service
//!   was not reachable for.
//!
//! # Locks
//!
//! A trip needs the project's two subtrees, and exactly one writer may hold
//! them. When this tab has the project open the driver borrows that open's
//! live handles (its flusher persists the binding); otherwise it snapshots
//! the project under its lock and lets the lock go before the trip starts.
//! A project another tab holds is skipped — that tab's own driver has it.
//!
//! **The project lock is never held across the network** (D1;
//! `docs/adr/2026-07-08-per-project-library-locking.md`, amended). It
//! guards local OPFS consistency, and a refusal of it is a user-facing
//! claim ("open in another tab") that a round trip has no business making
//! true. So the trip is three phases: snapshot under the lock, publish
//! from the snapshot with no lock at all, bank what came back under a
//! second short hold. A project that changed while the publish was in
//! flight simply earns another trip — `SyncQueue::request` has always
//! treated a trip as one attempt against a consistent snapshot, and
//! `sync_queue::work_arriving_mid_flight_earns_another_trip` pins it.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

use gloo_timers::future::TimeoutFuture;
use lpa_cloud_client::LocalProject;
use lpa_studio_core::app::library::{LibraryStore, PackageSummary};
use lpc_history::PrefixedUid;

use super::sidecar_producer::read_identity;
use super::sync_queue::{DueProject, SyncQueue, SyncTrigger, TripResult};
use super::sync_status::{self, SyncOutcomeKind};
use super::sync_trip::{TripReport, classify, run_trip};
use crate::cloud::FetchCloudPort;
use crate::local_store::opfs_library_host;

/// How often the driver re-attempts everything it is still tracking. Coarse
/// on purpose: this is the offline-recovery path, not the save path.
const SWEEP_INTERVAL_MS: u32 = 60_000;

/// How long the sign-in sweep waits for the OPFS library host to exist.
/// Sign-in and the startup probe are independent tasks and either can land
/// first; only the sweep cares, and only once per session.
const HOST_WAIT_RETRIES: usize = 20;
const HOST_WAIT_DELAY_MS: u32 = 250;

/// Note that a project wants a trip to the service.
///
/// Fire-and-forget and cheap: signed out it does nothing at all, and signed
/// in it records the request and arms a pump. Never awaits, never touches
/// OPFS, safe to call from inside a catalog transaction.
pub fn note(uid: &str, trigger: SyncTrigger) {
    let engine = engine();
    if !engine.signed_in.get() {
        return;
    }
    let delay = engine.queue.borrow_mut().request(uid, trigger, now_ms());
    engine.pump_after(delay as u32);
}

/// Lift a uid's denied-push latch (P6): a pull observed an access or
/// membership change that could make the server answer differently, so the
/// project is offered again — as a save, which is what the latch swallowed.
/// A uid that was never latched is a no-op.
pub fn clear_denied(uid: &str) {
    let engine = engine();
    if !engine.queue.borrow_mut().clear_denied(uid) {
        return;
    }
    note(uid, SyncTrigger::Saved);
}

/// Tell the driver whether there is an account to sync with.
///
/// The false→true edge is the sign-in sweep (D7): every project in the
/// library is offered to the service, and the ones it has never seen are
/// published. Going the other way forgets the queue outright — the next
/// account's first pump must not inherit this one's work.
pub fn set_signed_in(signed_in: bool) {
    let engine = engine();
    sync_status::record(|board| board.record_signed_in(signed_in));
    if engine.signed_in.replace(signed_in) == signed_in {
        return;
    }
    if signed_in {
        let engine = Rc::clone(&engine);
        wasm_bindgen_futures::spawn_local(async move { engine.sweep().await });
    } else {
        engine.queue.borrow_mut().clear();
    }
}

/// One tab's auto-publish driver.
struct SyncEngine {
    queue: RefCell<SyncQueue>,
    signed_in: Cell<bool>,
    /// One pump at a time — the trips inside it run sequentially, which is
    /// the whole concurrency cap (libraries are small).
    pumping: Cell<bool>,
    /// A pump fired while a pass was running. The pass owns the re-check:
    /// it takes another look at the queue before clearing `pumping`, so the
    /// work that armed the swallowed pump is not stranded until the sweep.
    wanted: Cell<bool>,
}

impl SyncEngine {
    fn new() -> Self {
        Self {
            queue: RefCell::new(SyncQueue::new()),
            signed_in: Cell::new(false),
            pumping: Cell::new(false),
            wanted: Cell::new(false),
        }
    }

    /// Run the pump `delay_ms` from now, off the caller's critical path.
    fn pump_after(self: &Rc<Self>, delay_ms: u32) {
        let engine = Rc::clone(self);
        wasm_bindgen_futures::spawn_local(async move {
            if delay_ms > 0 {
                TimeoutFuture::new(delay_ms).await;
            }
            engine.pump().await;
        });
    }

    /// Take everything that is due and run it, one project at a time.
    ///
    /// Anything that arrives mid-pass arms its own pump, but that pump may
    /// fire while this pass is still on an earlier trip — and it cannot run
    /// concurrently, so it marks `wanted` and leaves. The mark makes this
    /// pass go around again: a swallowed pump only ever fires at or after
    /// its entry's due time, so the re-check is guaranteed to pick the
    /// entry up. Failures still wait for the sweep timer — `finish` re-arms
    /// them a minute out, so the loop never spins on them.
    async fn pump(self: &Rc<Self>) {
        if !self.signed_in.get() {
            return;
        }
        if self.pumping.replace(true) {
            self.wanted.set(true);
            return;
        }
        loop {
            self.wanted.set(false);
            let due = self.queue.borrow_mut().take_due(now_ms());
            if !due.is_empty() {
                let library = roster().await;
                for project in due {
                    let result = self.run_one(&project, &library).await;
                    self.queue
                        .borrow_mut()
                        .finish(&project.uid, result, now_ms());
                }
            }
            if !self.wanted.get() {
                break;
            }
        }
        self.pumping.set(false);
    }

    /// Offer every project in the library to the service (sign-in, D7).
    ///
    /// Unconditional rather than filtered on "has no binding": the trip
    /// already decides publish-vs-push from the binding, and offering the
    /// bound ones too is how a push that failed while offline eventually
    /// lands.
    async fn sweep(self: &Rc<Self>) {
        if !self.signed_in.get() {
            return;
        }
        // The library host is built by an independent startup task, and a
        // `whoami` that answers first would otherwise sweep an empty roster
        // and consider the account done until the next save.
        if !await_library_host().await {
            sync_status::record(|board| board.record_sweep(0, true, now_ms()));
            return;
        }
        // Read the library BEFORE touching the queue: a `RefMut` may not be
        // held across an await — a save landing mid-read would panic on the
        // second borrow.
        let library = roster().await;
        let now = now_ms();
        sync_status::record(|board| board.record_sweep(library.len(), false, now));
        {
            let mut queue = self.queue.borrow_mut();
            for uid in library.keys() {
                queue.request(uid, SyncTrigger::Swept, now);
            }
        }
        self.pump().await;
    }

    /// The coarse retry loop. Started once, runs for the life of the tab.
    async fn sweep_forever(self: Rc<Self>) {
        loop {
            TimeoutFuture::new(SWEEP_INTERVAL_MS).await;
            if !self.signed_in.get() {
                continue;
            }
            self.queue.borrow_mut().arm_all(now_ms());
            self.pump().await;
        }
    }

    /// One project's trip: snapshot, publish, bank — in that order, and
    /// with the project lock alive only inside the first and the last (see
    /// the module's `# Locks`).
    async fn run_one(&self, due: &DueProject, roster: &Roster) -> TripResult {
        let entry = roster.get(&due.uid);
        let name = entry.map_or(due.uid.as_str(), |entry| entry.name.as_str());
        let conclude = |kind: SyncOutcomeKind, detail: &str| {
            sync_status::record(|board| {
                board.record_project(&due.uid, name, kind, detail, now_ms());
            });
        };
        let Some(host) = opfs_library_host() else {
            conclude(SyncOutcomeKind::Skipped, "no library host in this tab");
            return TripResult::Refused;
        };
        let Ok(uid) = due.uid.parse::<PrefixedUid>() else {
            log::warn!("cloud sync: {} is not a project uid", due.uid);
            conclude(SyncOutcomeKind::Refused, "not a project uid");
            return TripResult::Refused;
        };
        let mount = match host
            .mount_for_sync(&due.uid, entry.map(|entry| entry.slug.as_str()))
            .await
        {
            Ok(Some(mount)) => mount,
            // Another tab holds the project; its driver owns the trip.
            Ok(None) => {
                conclude(SyncOutcomeKind::Skipped, "open in another tab; that tab syncs it");
                return TripResult::Retry;
            }
            Err(error) => {
                log::warn!("cloud sync: cannot reach {}: {error}", due.uid);
                conclude(SyncOutcomeKind::Retrying, &format!("cannot read locally: {error}"));
                return TripResult::Retry;
            }
        };

        // No lock is held from here until `finish`: `mount_for_sync`
        // released it once the snapshot was in memory.
        let result = run_mounted(&mount, uid, name, due.restate).await;
        mount.finish().await;

        match result {
            Ok(report) => {
                log::debug!("cloud sync: {} {report:?}", due.uid);
                let (kind, detail) = describe(&report);
                conclude(kind, detail);
                TripResult::Settled
            }
            Err(error) => {
                let verdict = classify(&error);
                log::warn!("cloud sync: {} failed ({verdict:?}): {error}", due.uid);
                let kind = match verdict {
                    TripResult::Retry => SyncOutcomeKind::Retrying,
                    TripResult::Denied => SyncOutcomeKind::Denied,
                    TripResult::Settled | TripResult::Refused => SyncOutcomeKind::Refused,
                };
                conclude(kind, &error.to_string());
                verdict
            }
        }
    }
}

/// A settled report as a status row: the badge word and the sentence.
fn describe(report: &TripReport) -> (SyncOutcomeKind, &'static str) {
    match report {
        TripReport::Published(_) => (SyncOutcomeKind::Published, "record and content are up"),
        TripReport::Pushed(_) => (SyncOutcomeKind::Pushed, "content is up"),
        // The branch that used to vanish: no saved version means no trip,
        // no traffic, and — before the ledger — no trace.
        TripReport::Nothing => (
            SyncOutcomeKind::NothingSaved,
            "no saved version yet — nothing to publish",
        ),
    }
}

/// The trip itself, over mounted stores. Split out so the mount's borrow
/// ends before it is released.
async fn run_mounted(
    mount: &crate::library_host_opfs::SyncMount,
    uid: PrefixedUid,
    fallback_name: &str,
    restate: bool,
) -> Result<super::sync_trip::TripReport, lpa_cloud_client::SyncError> {
    let identity = read_identity(mount.package(), fallback_name)
        .map_err(|error| lpa_cloud_client::SyncError::Encode(error.to_string()))?;
    let project = LocalProject::new(uid, mount.package(), mount.history());
    run_trip(
        &FetchCloudPort::new(),
        &project,
        &identity.sidecar(),
        &identity.cloud_slug(),
        restate,
    )
    .await
}

/// What the driver needs to know about a project without mounting it: where
/// its package directory is, and what to call it if its manifest will not
/// say.
type Roster = BTreeMap<String, RosterEntry>;

struct RosterEntry {
    slug: String,
    name: String,
}

/// Wait, briefly, for the startup probe to build the library host.
/// `false` = it never came; the sweep is lost until the next page load.
async fn await_library_host() -> bool {
    for _ in 0..HOST_WAIT_RETRIES {
        if opfs_library_host().is_some() {
            return true;
        }
        TimeoutFuture::new(HOST_WAIT_DELAY_MS).await;
    }
    log::warn!("cloud sync: no library host to sweep");
    false
}

/// One catalog snapshot per pump, not one per project.
async fn roster() -> Roster {
    let Some(host) = opfs_library_host() else {
        return Roster::new();
    };
    let snapshot = match lpa_studio_core::app::library::LibraryHost::catalog_snapshot(&*host).await
    {
        Ok(snapshot) => snapshot,
        Err(error) => {
            log::warn!("cloud sync: cannot read the library roster: {error}");
            return Roster::new();
        }
    };
    let summaries = LibraryStore::read_only(snapshot).list().unwrap_or_default();
    summaries
        .into_iter()
        .map(
            |PackageSummary {
                 uid, slug, name, ..
             }| (uid.to_string(), RosterEntry { slug, name }),
        )
        .collect()
}

thread_local! {
    static ENGINE: RefCell<Option<Rc<SyncEngine>>> = const { RefCell::new(None) };
}

/// The tab's driver, started on first use.
fn engine() -> Rc<SyncEngine> {
    ENGINE.with(|slot| {
        if let Some(engine) = slot.borrow().as_ref() {
            return Rc::clone(engine);
        }
        let engine = Rc::new(SyncEngine::new());
        *slot.borrow_mut() = Some(Rc::clone(&engine));
        wasm_bindgen_futures::spawn_local(Rc::clone(&engine).sweep_forever());
        engine
    })
}

fn now_ms() -> f64 {
    js_sys::Date::now()
}
