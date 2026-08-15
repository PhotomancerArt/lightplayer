//! The real `LibraryHost`: per-project OPFS stores under typed Web Locks.
//!
//! This is the M4b storage-concurrency model's edge half (the typed locks
//! live in `lpa_fs_opfs::library_locks`; the vocabulary and ordering rule
//! in `lpa_studio_core::app::library::library_host`):
//!
//! - **Catalog transactions** ([`LibraryHost::catalog`]): try the target
//!   project's lock first when the op is structural (refusal = "open in
//!   another tab"), then the catalog lock (short retry, then `Busy`);
//!   mount the whole store fresh, apply the op synchronously, **flush
//!   fully before releasing**, broadcast `"changed"`.
//! - **Project open** ([`LibraryHost::open_project`]): resolve the key
//!   from a fresh snapshot, acquire the project's exclusive lock — polling
//!   briefly, because a refusal here is usually this tab's own momentary
//!   hold — **re-verify the key still resolves to the same uid under the
//!   lock** (a rename in another tab can race the unlocked read; retry
//!   once), then mount the package and history subtrees as their own
//!   memory-primary stores with write-behind flushers. The held lock is
//!   what makes write-behind correct: one writer per subtree. The open
//!   returns an `OpenReceipt`: the caller's open is not finished when this
//!   one is, and an uncommitted receipt runs [`OpenRegistry::release_open`]
//!   rather than leaving the lock held for the page's lifetime.
//! - **Snapshots** ([`LibraryHost::catalog_snapshot`]): fresh read-only
//!   mounts (no flusher) skipping history payloads; no locks — whole-file
//!   atomic writes make torn files impossible, torn *sets* merely stale.
//!
//! Browsers without Web Locks (non-secure contexts) proceed unguarded
//! rather than losing persistence — M2's behavior, kept deliberately.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use gloo_timers::future::TimeoutFuture;
use lpa_fs_opfs::{
    HISTORY_DIR, LibraryLock, LibraryLockGuard, LpFsOpfs, PACKAGES_DIR, held_project_uids,
    list_child_dirs, open_dir, open_library_root, open_library_subdir, remove_path, try_acquire,
    try_acquire_polling,
};
use lpa_studio_core::app::library::{
    CatalogOp, CatalogOutcome, LibraryHost, LibraryHostError, LibraryStore, LocalBoxFuture,
    OpenReceipt, OpenedProject, apply_catalog_op,
};
use lpfs::{LpFs, LpPath};

use crate::cloud::sync::sync_queue::SyncTrigger;

/// Flush cadence for open-project stores.
const FLUSH_INTERVAL_MS: u32 = 100;

/// Catalog-lock acquisition: holds last tens of ms, so a short retry loop
/// beats surfacing `Busy` on the first collision.
const CATALOG_RETRIES: usize = 5;
const CATALOG_RETRY_DELAY_MS: u32 = 50;

/// Project-lock acquisition on the OPEN path — the same ladder shape as
/// the catalog's, one notch longer.
///
/// Release travels through the lock manager asynchronously, so a lock this
/// tab let go a moment ago (a sim-crash recovery reopening the project it
/// just released, a sync trip finishing) still refuses the next instant
/// shot. Half a second of polling turns those benign races into a slightly
/// slower open instead of a wrong "open in another tab"; a lock a *real*
/// other tab holds outlasts the ladder and refuses as before.
const OPEN_RETRIES: usize = 10;
const OPEN_RETRY_DELAY_MS: u32 = 50;

/// How long an open waits out this tab's own cloud sync trip before
/// concluding somebody else holds the project. A trip is a network round
/// trip, so this is seconds rather than the catalog's tens of ms — and a
/// slow open beats a wrong "open in another tab".
const SYNC_HANDOFF_RETRIES: usize = 30;
const SYNC_HANDOFF_DELAY_MS: u32 = 100;

/// BroadcastChannel name for cross-tab library-change pings.
pub const LIBRARY_CHANNEL: &str = "lp-library";

/// One open project's edge state: the held lock, the two mounted stores,
/// and the shared stop flag their flush loops watch.
struct OpenProjectStores {
    /// `None` when Web Locks are unavailable (unguarded mode).
    guard: Option<LibraryLockGuard>,
    package: LpFsOpfs,
    history: LpFsOpfs,
    stop_flushers: Rc<Cell<bool>>,
}

/// The open projects of this tab, plus the ping channel — everything a
/// teardown touches, behind one `Rc` so an [`OpenReceipt`] can carry it
/// (see [`OpenRegistry::release_open`]). Releasing a project's lock is
/// exactly as interesting to other tabs' badges as taking it, which is
/// why the channel lives here and not beside it.
struct OpenRegistry {
    open: RefCell<HashMap<String, OpenProjectStores>>,
    /// Sender side of the cross-tab ping channel (`None` if the browser
    /// lacks BroadcastChannel; pings are best-effort).
    channel: Option<web_sys::BroadcastChannel>,
}

impl OpenRegistry {
    fn broadcast_changed(&self) {
        if let Some(channel) = &self.channel {
            let _ = channel.post_message(&wasm_bindgen::JsValue::from_str("changed"));
        }
    }

    /// **The** teardown of one open: take it out of the registry, stop its
    /// flushers, flush both stores, release the project lock, tell the
    /// other tabs. Idempotent — a uid this tab does not hold open is a
    /// no-op, which is what makes it safe to call from both endings.
    ///
    /// Both endings run through here: [`LibraryHost::close_project`] awaits
    /// it, and an [`OpenReceipt`] dropped uncommitted (a failed open; a
    /// superseded one, later) spawns it. There is deliberately no second
    /// copy of this sequence anywhere.
    async fn release_open(&self, uid: &str) {
        let state = self.open.borrow_mut().remove(uid);
        let Some(state) = state else {
            return;
        };
        state.stop_flushers.set(true);
        if let Err(e) = state.package.flush().await {
            log::warn!("close flush (package): {e}");
        }
        if let Err(e) = state.history.flush().await {
            log::warn!("close flush (history): {e}");
        }
        if let Some(guard) = state.guard {
            guard.release();
        }
        // other tabs' "open in another tab" badges clear promptly
        self.broadcast_changed();
    }
}

/// Start [`OpenRegistry::release_open`] without waiting for it — what an
/// abandoned [`OpenReceipt`] can do, `Drop` being synchronous while the
/// teardown flushes.
fn spawn_release_open(registry: Rc<OpenRegistry>, uid: String) {
    wasm_bindgen_futures::spawn_local(async move {
        registry.release_open(&uid).await;
    });
}

/// The OPFS-backed [`LibraryHost`]. One per tab, attached at startup.
pub struct OpfsLibraryHost {
    registry: Rc<OpenRegistry>,
    /// Project uids this tab's cloud driver is holding the project lock for
    /// (see [`Self::mount_for_sync`]). Shared with the live [`SyncMount`]s,
    /// which clear their own entry when they drop.
    syncing: Rc<RefCell<HashSet<String>>>,
}

impl OpfsLibraryHost {
    pub fn new() -> Self {
        let channel = web_sys::BroadcastChannel::new(LIBRARY_CHANNEL)
            .map_err(|e| log::warn!("BroadcastChannel unavailable: {e:?}"))
            .ok();
        Self {
            registry: Rc::new(OpenRegistry {
                open: RefCell::new(HashMap::new()),
                channel,
            }),
            syncing: Rc::new(RefCell::new(HashSet::new())),
        }
    }

    /// Best-effort flush of every open project store — the `pagehide`
    /// handler. Async IO during pagehide may not complete; this shrinks
    /// the write-behind loss window (≤ ~flush interval + write time),
    /// nothing more.
    pub fn flush_open_projects_best_effort(&self) {
        for state in self.registry.open.borrow().values() {
            let package = state.package.clone();
            let history = state.history.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let _ = package.flush().await;
                let _ = history.flush().await;
            });
        }
    }

    fn broadcast_changed(&self) {
        self.registry.broadcast_changed();
    }

    /// Borrow (or mount) one project's two subtrees for a cloud sync trip.
    ///
    /// The cloud driver writes `/cloud-binding.json` into the history root,
    /// so it needs the same single-writer guarantee every other write path
    /// has:
    ///
    /// - **Open in this tab** — hand back the open's own live handles. Its
    ///   flusher persists whatever the trip writes, and taking the project
    ///   lock here would deadlock against the lock the open already holds.
    /// - **Not open anywhere** — take the project lock for the trip and
    ///   mount both subtrees fresh; [`SyncMount::release`] flushes before it
    ///   lets go, exactly as a catalog transaction does.
    /// - **Open in another tab** — `Ok(None)`. That tab runs its own driver
    ///   and owns this project's trips; this one skips it.
    ///
    /// `slug` is the package directory's name. The caller usually has it
    /// from the roster it just read; `None` costs a snapshot mount to
    /// resolve.
    pub(crate) async fn mount_for_sync(
        &self,
        uid: &str,
        slug: Option<&str>,
    ) -> Result<Option<SyncMount>, LibraryHostError> {
        if let Some(state) = self.registry.open.borrow().get(uid) {
            return Ok(Some(SyncMount {
                guard: None,
                held: None,
                package: state.package.clone(),
                history: state.history.clone(),
                borrowed: true,
            }));
        }
        // Registered as ours BEFORE the acquire, not after it: an open
        // polling `syncing` in the gap would see no trip, skip the handoff
        // wait entirely, and be refused instantly by the lock this is about
        // to take. Dropping `held` un-registers, so a refusal (that project
        // really is another tab's) costs one poll interval at worst.
        //
        // Only the mount that *added* the uid carries the guard: the visitor
        // pull loop and the sync driver can both reach for one project, and
        // the second one to arrive must not clear a registration the first
        // is still standing behind.
        let held = self
            .syncing
            .borrow_mut()
            .insert(uid.to_string())
            .then(|| HeldForSync {
                uid: uid.to_string(),
                syncing: Rc::clone(&self.syncing),
            });
        let guard = match acquire(&LibraryLock::Project(uid.to_string())).await {
            Acquired::Held(guard) => Some(guard),
            Acquired::Unguarded => None,
            Acquired::Refused => return Ok(None),
        };
        let slug = match slug {
            Some(slug) => slug.to_string(),
            None => resolve_key_snapshot(uid).await?.1,
        };
        let package_dir = open_library_subdir(&format!("{PACKAGES_DIR}/{slug}"), false)
            .await
            .map_err(|e| LibraryHostError::Host(format!("open package dir: {e}")))?;
        let history_dir = open_library_subdir(&format!("{HISTORY_DIR}/{uid}"), true)
            .await
            .map_err(|e| LibraryHostError::Host(format!("open history dir: {e}")))?;
        Ok(Some(SyncMount {
            guard,
            held,
            package: LpFsOpfs::mount(package_dir)
                .await
                .map_err(|e| LibraryHostError::Host(format!("mount package: {e}")))?,
            history: LpFsOpfs::mount(history_dir)
                .await
                .map_err(|e| LibraryHostError::Host(format!("mount history: {e}")))?,
            borrowed: false,
        }))
    }

    /// Wait out this tab's own cloud sync trip on `uid`, if there is one.
    ///
    /// The driver holds the project lock across a network round trip, and a
    /// user who just created a project and clicked into it must not be told
    /// it is "open in another tab" — the hold is this tab's, and it ends.
    /// Bounded: past the bound the ordinary refusal path takes over.
    async fn await_sync_handoff(&self, uid: &str) {
        for _ in 0..SYNC_HANDOFF_RETRIES {
            if !self.syncing.borrow().contains(uid) {
                return;
            }
            TimeoutFuture::new(SYNC_HANDOFF_DELAY_MS).await;
        }
        log::warn!("open of {uid} gave up waiting for its cloud sync trip");
    }

    /// Acquire the catalog lock with a short retry, mapping exhaustion to
    /// `Busy`. `Ok(None)` = Web Locks unavailable, proceed unguarded.
    async fn acquire_catalog(&self) -> Result<Option<LibraryLockGuard>, LibraryHostError> {
        for _ in 0..CATALOG_RETRIES {
            match acquire(&LibraryLock::Catalog).await {
                Acquired::Held(guard) => return Ok(Some(guard)),
                Acquired::Unguarded => return Ok(None),
                Acquired::Refused => TimeoutFuture::new(CATALOG_RETRY_DELAY_MS).await,
            }
        }
        Err(LibraryHostError::Busy(
            "the catalog lock stayed held elsewhere".to_string(),
        ))
    }
}

impl Default for OpfsLibraryHost {
    fn default() -> Self {
        Self::new()
    }
}

impl LibraryHost for OpfsLibraryHost {
    fn catalog_snapshot(
        &self,
    ) -> LocalBoxFuture<'_, Result<Rc<RefCell<dyn LpFs>>, LibraryHostError>> {
        Box::pin(async move {
            let snapshot = mount_snapshot(skip_history_payloads).await?;
            Ok(rc_fs(snapshot))
        })
    }

    fn catalog(
        &self,
        op: CatalogOp,
    ) -> LocalBoxFuture<'_, Result<CatalogOutcome, LibraryHostError>> {
        Box::pin(async move {
            // Project before Catalog (the ordering rule): structural ops
            // targeting a project take its lock first — a refusal is the
            // "open in another tab" answer, before anything mutates.
            let _project_guard = match structural_target_uid(&op) {
                Some(uid) => {
                    if self.registry.open.borrow().contains_key(uid) {
                        return Err(LibraryHostError::OpenInThisTab {
                            uid: uid.to_string(),
                        });
                    }
                    match acquire(&LibraryLock::Project(uid.to_string())).await {
                        Acquired::Held(guard) => Some(guard),
                        Acquired::Unguarded => None,
                        Acquired::Refused => {
                            return Err(LibraryHostError::OpenElsewhere {
                                key: uid.to_string(),
                            });
                        }
                    }
                }
                None => None,
            };
            let _catalog_guard = self.acquire_catalog().await?;

            // fresh full mount (history payloads too: Duplicate reads
            // source bytes; fine at current scale)
            let store_fs = mount_root_store().await?;
            let store = writable_store(&store_fs);
            let trigger = sync_trigger_for(&op);
            let result = apply_catalog_op(&store, op, now_secs());

            // flush fully BEFORE the guards release (drop order below)
            store_fs
                .flush()
                .await
                .map_err(|e| LibraryHostError::Host(format!("catalog flush: {e}")))?;
            prune_directory_husks(&store_fs).await;
            self.broadcast_changed();
            // Auto-publish (vision D2): a transaction that produced a package
            // has changed what the cloud should hold about it. Exactly one
            // request per transaction, which is what keeps a create from
            // publishing twice even though several op variants funnel through
            // `install_package`. Fire-and-forget, after the flush and outside
            // every lock.
            if let Ok(outcome) = &result
                && let Some(summary) = &outcome.summary
                && let Some(trigger) = trigger
            {
                crate::cloud::sync::sync_engine::note(&summary.uid.to_string(), trigger);
            }
            result
        })
    }

    fn open_project<'a>(
        &'a self,
        key: &'a str,
    ) -> LocalBoxFuture<'a, Result<OpenedProject, LibraryHostError>> {
        Box::pin(async move {
            // resolve → lock → RE-VERIFY: the first resolve is lock-free,
            // so a rename in another tab can race it; one retry absorbs
            // exactly that race.
            for _attempt in 0..2 {
                let (uid, _slug) = resolve_key_snapshot(key).await?;
                if self.registry.open.borrow().contains_key(&uid) {
                    return Err(LibraryHostError::OpenInThisTab { uid });
                }
                // Parsed before anything is acquired: a malformed uid is a
                // refusal, not a lock to unwind.
                let parsed_uid = uid
                    .parse()
                    .map_err(|e| LibraryHostError::Host(format!("uid {uid:?}: {e}")))?;
                self.await_sync_handoff(&uid).await;
                let guard = match acquire_for_open(&LibraryLock::Project(uid.clone())).await {
                    Acquired::Held(guard) => Some(guard),
                    Acquired::Unguarded => None,
                    Acquired::Refused => {
                        return Err(LibraryHostError::OpenElsewhere {
                            key: key.to_string(),
                        });
                    }
                };
                let (verified_uid, slug) = resolve_key_snapshot(key).await?;
                if verified_uid != uid {
                    // a rename raced the unlocked read; drop the wrong
                    // lock and retry once
                    drop(guard);
                    continue;
                }

                let package_dir = open_library_subdir(&format!("{PACKAGES_DIR}/{slug}"), false)
                    .await
                    .map_err(|e| LibraryHostError::Host(format!("open package dir: {e}")))?;
                let history_dir = open_library_subdir(&format!("{HISTORY_DIR}/{uid}"), true)
                    .await
                    .map_err(|e| LibraryHostError::Host(format!("open history dir: {e}")))?;
                let package = LpFsOpfs::mount(package_dir)
                    .await
                    .map_err(|e| LibraryHostError::Host(format!("mount package: {e}")))?;
                let history = LpFsOpfs::mount(history_dir)
                    .await
                    .map_err(|e| LibraryHostError::Host(format!("mount history: {e}")))?;

                let stop_flushers = Rc::new(Cell::new(false));
                spawn_flusher(package.clone(), Rc::clone(&stop_flushers));
                spawn_flusher(history.clone(), Rc::clone(&stop_flushers));
                self.registry.open.borrow_mut().insert(
                    uid.clone(),
                    OpenProjectStores {
                        guard,
                        package: package.clone(),
                        history: history.clone(),
                        stop_flushers,
                    },
                );

                // From here the lock, the registration and two flush loops
                // are live and only the caller knows whether the open ever
                // finishes — so it leaves holding the undo.
                let registry = Rc::clone(&self.registry);
                let abandoned = uid.clone();
                let receipt = OpenReceipt::new(move || spawn_release_open(registry, abandoned));
                return Ok(OpenedProject {
                    uid: parsed_uid,
                    slug,
                    package_fs: rc_fs(package),
                    history_fs: rc_fs(history),
                    receipt,
                });
            }
            Err(LibraryHostError::Busy(
                "a rename raced this open twice; try again".to_string(),
            ))
        })
    }

    fn close_project<'a>(&'a self, uid: &'a str) -> LocalBoxFuture<'a, ()> {
        // the same teardown an abandoned open runs; idempotent either way
        Box::pin(async move { self.registry.release_open(uid).await })
    }

    fn open_elsewhere_uids(&self) -> LocalBoxFuture<'_, Vec<String>> {
        Box::pin(async move {
            let mut held = held_project_uids().await;
            let open = self.registry.open.borrow();
            held.retain(|uid| !open.contains_key(uid));
            held
        })
    }

    fn notify_saved(&self, uid: &str) {
        self.broadcast_changed();
        // Push-on-save, debounced by the driver. Nothing here awaits: a save
        // is never allowed to wait on the network.
        crate::cloud::sync::sync_engine::note(uid, SyncTrigger::Saved);
    }
}

/// Which cloud-sync trigger a catalog transaction is.
///
/// Only [`CatalogOp::Rename`] restates the project's identity — it is the
/// one op that changes the display name, and therefore the slug half of the
/// share address. Everything else that produces a package changed its
/// content, which is a push — except a **tracking copy** landing from
/// `open_shared` (P6): that is the *service's* copy arriving here, and
/// offering it straight back would be a pointless push (and, for a
/// view-only visitor, an instant denial). A fork installed through the
/// same op IS new local work and publishes normally.
fn sync_trigger_for(op: &CatalogOp) -> Option<SyncTrigger> {
    use lpa_studio_core::app::library::PackageProvenance;
    match op {
        CatalogOp::Rename { .. } => Some(SyncTrigger::Renamed),
        CatalogOp::InstallSyncedProject {
            provenance: PackageProvenance::OpenedFromLink,
            ..
        } => None,
        _ => Some(SyncTrigger::Installed),
    }
}

/// One project's subtrees, held for the duration of a cloud sync trip.
///
/// See [`OpfsLibraryHost::mount_for_sync`] for how it was obtained; the
/// difference between the two cases is entirely in [`Self::release`].
pub(crate) struct SyncMount {
    /// The project lock, when this mount took one.
    guard: Option<LibraryLockGuard>,
    /// Registration in the host's `syncing` set, cleared on drop.
    held: Option<HeldForSync>,
    package: LpFsOpfs,
    history: LpFsOpfs,
    /// These are an open project's live handles, not ours: its flusher and
    /// its lock outlive the trip.
    borrowed: bool,
}

/// "This tab's driver is holding that project", as a drop guard.
///
/// A guard rather than a paired insert/remove because a trip that never
/// starts (a refused acquire) or fails mid-mount must not leave the uid
/// registered — an open would then wait the full handoff bound for a trip
/// that is not running.
struct HeldForSync {
    uid: String,
    syncing: Rc<RefCell<HashSet<String>>>,
}

impl Drop for HeldForSync {
    fn drop(&mut self) {
        self.syncing.borrow_mut().remove(&self.uid);
    }
}

impl SyncMount {
    pub(crate) fn package(&self) -> &LpFsOpfs {
        &self.package
    }

    pub(crate) fn history(&self) -> &LpFsOpfs {
        &self.history
    }

    /// Flush what the trip wrote and let the project go.
    ///
    /// A borrowed mount does neither: flushing would race the open's own
    /// flusher for no gain, and there is no lock here to release.
    pub(crate) async fn release(self) {
        if self.borrowed {
            return;
        }
        // Only the binding is ever written, and only on a successful trip;
        // a clean store makes this a no-op.
        if let Err(e) = self.history.flush().await {
            log::warn!("cloud sync flush (history): {e}");
        }
        if let Some(guard) = self.guard {
            guard.release();
        }
        // After the lock, so an open that was waiting on the registration
        // finds the lock already free when it stops waiting.
        drop(self.held);
    }
}

/// One `try_acquire` outcome, with the unguarded fallback made explicit.
enum Acquired {
    Held(LibraryLockGuard),
    /// Web Locks unavailable — proceed without the guard (M2 behavior).
    Unguarded,
    Refused,
}

async fn acquire(lock: &LibraryLock) -> Acquired {
    classify(try_acquire(lock).await)
}

/// [`acquire`] with the open path's policy: poll the ladder before calling
/// it a refusal (see [`OPEN_RETRIES`]). Everything else keeps the one-shot
/// — a structural catalog op asking "is this open elsewhere" wants the
/// instant answer, not a wait.
async fn acquire_for_open(lock: &LibraryLock) -> Acquired {
    classify(try_acquire_polling(lock, OPEN_RETRIES, OPEN_RETRY_DELAY_MS).await)
}

fn classify(outcome: Result<Option<LibraryLockGuard>, wasm_bindgen::JsValue>) -> Acquired {
    match outcome {
        Ok(Some(guard)) => Acquired::Held(guard),
        Ok(None) => Acquired::Refused,
        Err(e) => {
            log::warn!("web locks unavailable, proceeding unguarded: {e:?}");
            Acquired::Unguarded
        }
    }
}

/// The structural catalog ops take the target project's lock first;
/// creation-shaped ops (Create/Import/Seed) touch no existing project.
fn structural_target_uid(op: &CatalogOp) -> Option<&str> {
    match op {
        CatalogOp::Rename { uid, .. }
        | CatalogOp::Duplicate { uid }
        | CatalogOp::Delete { uid }
        | CatalogOp::RecordDeviceObservation {
            project_uid: uid, ..
        }
        | CatalogOp::AdoptObservedVersion {
            project_uid: uid, ..
        }
        | CatalogOp::ForkObservedVersion {
            project_uid: uid, ..
        }
        | CatalogOp::UpgradePackageFormat { project_uid: uid }
        | CatalogOp::RecordPush {
            project_uid: uid, ..
        } => Some(uid),
        CatalogOp::Create { .. }
        | CatalogOp::ImportZip { .. }
        | CatalogOp::ImportJson { .. }
        | CatalogOp::EnsureExampleSeeded { .. }
        | CatalogOp::GenerateForBoard { .. }
        | CatalogOp::UpsertRegisteredDevice(_)
        | CatalogOp::RenameRegisteredDevice { .. }
        | CatalogOp::RekeyRegisteredDevice { .. }
        | CatalogOp::ForgetRegisteredDevice { .. }
        | CatalogOp::AdoptDevicePackage { .. }
        // Creation-shaped: the synced install refuses a uid the library
        // already holds, so there is no existing project to lock.
        | CatalogOp::InstallSyncedProject { .. } => None,
    }
}

fn rc_fs(store: LpFsOpfs) -> Rc<RefCell<dyn LpFs>> {
    Rc::new(RefCell::new(store))
}

/// A mutating store over a transaction mount, with browser randomness and
/// the local wall-clock slug stamp injected.
fn writable_store(store_fs: &LpFsOpfs) -> LibraryStore {
    LibraryStore::new(
        rc_fs(store_fs.clone()),
        Rc::new(random_bytes),
        Rc::new(local_slug_stamp),
    )
}

async fn mount_root_store() -> Result<LpFsOpfs, LibraryHostError> {
    let root = open_library_root()
        .await
        .map_err(|e| LibraryHostError::Host(format!("library root: {e}")))?;
    LpFsOpfs::mount(root)
        .await
        .map_err(|e| LibraryHostError::Host(format!("mount: {e}")))
}

async fn mount_snapshot(skip_dir: impl Fn(&str) -> bool) -> Result<LpFsOpfs, LibraryHostError> {
    let root = open_library_root()
        .await
        .map_err(|e| LibraryHostError::Host(format!("library root: {e}")))?;
    LpFsOpfs::mount_filtered(root, skip_dir)
        .await
        .map_err(|e| LibraryHostError::Host(format!("snapshot mount: {e}")))
}

/// Gallery snapshots keep manifests, meta, and event logs; the content
/// payloads under `/history/<uid>/{blobs,trees}` never load.
fn skip_history_payloads(path: &str) -> bool {
    path.starts_with(&format!("{HISTORY_DIR}/"))
        && (path.ends_with("/blobs") || path.ends_with("/trees"))
}

/// Resolve a slug-or-uid key to `(uid, slug)` from a fresh snapshot that
/// skips history entirely (resolution only reads manifests).
async fn resolve_key_snapshot(key: &str) -> Result<(String, String), LibraryHostError> {
    let snapshot = mount_snapshot(|path| path == HISTORY_DIR).await?;
    let store = LibraryStore::read_only(rc_fs(snapshot));
    let uid = store.resolve_key(key).map_err(LibraryHostError::from)?;
    let slug = store
        .list()
        .map_err(LibraryHostError::from)?
        .into_iter()
        .find(|summary| summary.uid == uid)
        .map(|summary| summary.slug)
        .ok_or_else(|| LibraryHostError::NotFound(key.to_string()))?;
    Ok((uid.to_string(), slug))
}

fn spawn_flusher(store: LpFsOpfs, stop: Rc<Cell<bool>>) {
    wasm_bindgen_futures::spawn_local(async move {
        while !stop.get() {
            TimeoutFuture::new(FLUSH_INTERVAL_MS).await;
            if stop.get() {
                break;
            }
            if store.has_dirty() {
                if let Err(e) = store.flush().await {
                    log::warn!("opfs flush failed (will retry): {e}");
                }
            }
        }
    });
}

/// Remove empty package/history directory husks from OPFS. The flusher
/// removes files, never directories, so rename/delete leave empty dirs
/// behind (e.g. `/packages/<old-slug>/.lp/`). Harmless but crufty; the
/// end of a catalog transaction (still under the catalog lock) is the
/// safe place to sweep them: any dir with no files in the freshly
/// flushed mounted tree is a husk.
async fn prune_directory_husks(store: &LpFsOpfs) {
    let Ok(root) = open_library_root().await else {
        return;
    };
    for base in [PACKAGES_DIR, HISTORY_DIR] {
        let Ok(base_dir) = open_dir(&root, base, false).await else {
            continue;
        };
        let Ok(children) = list_child_dirs(&base_dir).await else {
            continue;
        };
        for child in children {
            let path = format!("{base}/{child}");
            let has_files = matches!(
                store.list_dir(LpPath::new(&path), true),
                Ok(entries) if !entries.is_empty()
            );
            if !has_files {
                if let Err(e) = remove_path(&root, LpPath::new(&path)).await {
                    log::warn!("husk prune of {path} failed: {e}");
                }
            }
        }
    }
}

/// Local wall-clock `YYYY-MM-DD-HHMM` for new-package slugs (the sans-IO
/// core takes this injected — it never reads a clock).
pub(crate) fn local_slug_stamp() -> String {
    let now = js_sys::Date::new_0();
    format!(
        "{:04}-{:02}-{:02}-{:02}{:02}",
        now.get_full_year(),
        now.get_month() + 1,
        now.get_date(),
        now.get_hours(),
        now.get_minutes(),
    )
}

/// Seconds since the Unix epoch — hosts are edges; they own time.
fn now_secs() -> f64 {
    js_sys::Date::now() / 1000.0
}

/// Crypto-quality bytes for uid minting — the library store's generator
/// here, and installed on the `StudioController` by the web shell for
/// `dev` device identities.
pub(crate) fn random_bytes() -> [u8; 16] {
    let mut bytes = [0u8; 16];
    let filled = web_sys::window()
        .and_then(|w| w.crypto().ok())
        .and_then(|c| c.get_random_values_with_u8_array(&mut bytes).ok())
        .is_some();
    if !filled {
        // last-resort fallback; uids only need uniqueness, not secrecy
        for b in bytes.iter_mut() {
            *b = (js_sys::Math::random() * 256.0) as u8;
        }
    }
    bytes
}
