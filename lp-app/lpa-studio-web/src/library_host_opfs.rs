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
//!   from a fresh snapshot, acquire the project's exclusive lock,
//!   **re-verify the key still resolves to the same uid under the lock**
//!   (a rename in another tab can race the unlocked read; retry once),
//!   then mount the package and history subtrees as their own
//!   memory-primary stores with write-behind flushers. The held lock is
//!   what makes write-behind correct: one writer per subtree.
//! - **Snapshots** ([`LibraryHost::catalog_snapshot`]): fresh read-only
//!   mounts (no flusher) skipping history payloads; no locks — whole-file
//!   atomic writes make torn files impossible, torn *sets* merely stale.
//!
//! Browsers without Web Locks (non-secure contexts) proceed unguarded
//! rather than losing persistence — M2's behavior, kept deliberately.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use gloo_timers::future::TimeoutFuture;
use lpa_fs_opfs::{
    HISTORY_DIR, LibraryLock, LibraryLockGuard, LpFsOpfs, PACKAGES_DIR, held_project_uids,
    list_child_dirs, open_dir, open_library_root, open_library_subdir, remove_path, try_acquire,
};
use lpa_studio_core::app::library::{
    CatalogOp, CatalogOutcome, LibraryHost, LibraryHostError, LibraryStore, LocalBoxFuture,
    OpenedProject, apply_catalog_op,
};
use lpfs::{LpFs, LpPath};

/// Flush cadence for open-project stores.
const FLUSH_INTERVAL_MS: u32 = 100;

/// Catalog-lock acquisition: holds last tens of ms, so a short retry loop
/// beats surfacing `Busy` on the first collision.
const CATALOG_RETRIES: usize = 5;
const CATALOG_RETRY_DELAY_MS: u32 = 50;

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

/// The OPFS-backed [`LibraryHost`]. One per tab, attached at startup.
pub struct OpfsLibraryHost {
    open: RefCell<HashMap<String, OpenProjectStores>>,
    /// Sender side of the cross-tab ping channel (`None` if the browser
    /// lacks BroadcastChannel; pings are best-effort).
    channel: Option<web_sys::BroadcastChannel>,
}

impl OpfsLibraryHost {
    pub fn new() -> Self {
        let channel = web_sys::BroadcastChannel::new(LIBRARY_CHANNEL)
            .map_err(|e| log::warn!("BroadcastChannel unavailable: {e:?}"))
            .ok();
        Self {
            open: RefCell::new(HashMap::new()),
            channel,
        }
    }

    /// Best-effort flush of every open project store — the `pagehide`
    /// handler. Async IO during pagehide may not complete; this shrinks
    /// the write-behind loss window (≤ ~flush interval + write time),
    /// nothing more.
    pub fn flush_open_projects_best_effort(&self) {
        for state in self.open.borrow().values() {
            let package = state.package.clone();
            let history = state.history.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let _ = package.flush().await;
                let _ = history.flush().await;
            });
        }
    }

    fn broadcast_changed(&self) {
        if let Some(channel) = &self.channel {
            let _ = channel.post_message(&wasm_bindgen::JsValue::from_str("changed"));
        }
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
                    if self.open.borrow().contains_key(uid) {
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
            let result = apply_catalog_op(&store, op, now_secs());

            // flush fully BEFORE the guards release (drop order below)
            store_fs
                .flush()
                .await
                .map_err(|e| LibraryHostError::Host(format!("catalog flush: {e}")))?;
            prune_directory_husks(&store_fs).await;
            self.broadcast_changed();
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
                if self.open.borrow().contains_key(&uid) {
                    return Err(LibraryHostError::OpenInThisTab { uid });
                }
                let guard = match acquire(&LibraryLock::Project(uid.clone())).await {
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
                self.open.borrow_mut().insert(
                    uid.clone(),
                    OpenProjectStores {
                        guard,
                        package: package.clone(),
                        history: history.clone(),
                        stop_flushers,
                    },
                );

                let uid = uid
                    .parse()
                    .map_err(|e| LibraryHostError::Host(format!("uid {uid:?}: {e}")))?;
                return Ok(OpenedProject {
                    uid,
                    slug,
                    package_fs: rc_fs(package),
                    history_fs: rc_fs(history),
                });
            }
            Err(LibraryHostError::Busy(
                "a rename raced this open twice; try again".to_string(),
            ))
        })
    }

    fn close_project<'a>(&'a self, uid: &'a str) -> LocalBoxFuture<'a, ()> {
        Box::pin(async move {
            // idempotent: closing a project this tab doesn't hold is a no-op
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
        })
    }

    fn open_elsewhere_uids(&self) -> LocalBoxFuture<'_, Vec<String>> {
        Box::pin(async move {
            let mut held = held_project_uids().await;
            let open = self.open.borrow();
            held.retain(|uid| !open.contains_key(uid));
            held
        })
    }

    fn notify_saved(&self, _uid: &str) {
        self.broadcast_changed();
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
    match try_acquire(lock).await {
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
        | CatalogOp::Delete { uid } => Some(uid),
        CatalogOp::Create { .. }
        | CatalogOp::ImportZip { .. }
        | CatalogOp::EnsureExampleSeeded { .. }
        | CatalogOp::UpsertRegisteredDevice(_) => None,
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
fn local_slug_stamp() -> String {
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

fn random_bytes() -> [u8; 16] {
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
