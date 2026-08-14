//! The `LibraryHost` seam: how the sans-IO core reaches the local library.
//!
//! Catalog transactions and project open/close await platform things —
//! Web Locks, OPFS mounts, flushes — that the core must not own
//! (`docs/adr/2026-07-06-sans-io-core.md`). The platform edge injects an
//! implementation of [`LibraryHost`]; its futures are runtime-neutral
//! (no spawns, no executor-flavored sleeps), so any edge can drive them —
//! the `ClientIo` precedent. Tests inject [`MemoryLibraryHost`], whose
//! futures are immediately ready (tests count as edges).
//!
//! The lock protocol the real host implements (P3 of the per-project
//! locking plan; the typed locks live in `lpa-fs-opfs::library_locks`):
//! [`CatalogOp`]s run under the short-lived catalog lock and flush fully
//! before releasing; [`LibraryHost::open_project`] resolves the key,
//! acquires the project's exclusive lock, then **re-verifies** the
//! slug→uid mapping under the lock (an unlocked catalog read can race a
//! rename in another tab). Hosts do not duplicate CRUD logic: they
//! mount/lock/flush around the same sync [`LibraryStore`] calls —
//! [`apply_catalog_op`] and [`open_project_via_store`] are that shared
//! middle.

use std::cell::RefCell;
use std::pin::Pin;
use std::rc::Rc;

use lpc_history::PrefixedUid;
use lpfs::LpFs;

use super::library_store::{LibraryError, LibraryStore, PackageSummary};
use super::package_meta::PackageProvenance;
use crate::app::places::RegisteredDevice;

/// Single-threaded boxed future, the shape every seam method returns.
/// (`Rc`-world, matching the actor; not `Send`.)
pub type LocalBoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;

/// One catalog mutation, executed by the host as a locked
/// flush-before-release transaction. This explicit vocabulary IS the
/// model: home ops map onto these, and the ADR documents them.
#[derive(Clone, Debug)]
pub enum CatalogOp {
    /// Create a project and install it.
    ///
    /// `files` absent is the historical blank create: the store writes the
    /// minimal manifest and root module itself. `files` present installs a
    /// generated template verbatim — `install_package` writes a manifest
    /// only when the incoming files lack one, so a template's authored
    /// `project.json` (with its `kind`/`exports`) survives untouched.
    Create {
        name: String,
        files: Option<Vec<(String, Vec<u8>)>>,
    },
    /// Rename = slug edit = directory move (catalog structure). The final
    /// slug (slugified, collision-suffixed) rides the outcome summary.
    Rename {
        uid: String,
        new_slug: String,
    },
    /// Fork at head; the copy re-stamps the source's label.
    Duplicate {
        uid: String,
    },
    Delete {
        uid: String,
    },
    ImportZip {
        file_name: String,
        bytes: Vec<u8>,
    },
    /// Install from a pasted `lp.package` share envelope.
    ImportJson {
        text: String,
    },
    /// Seed-once: find by `SeededFrom` provenance or install the embedded
    /// example — atomic under the catalog lock (two fresh tabs racing the
    /// same example must produce ONE package).
    EnsureExampleSeeded {
        id: String,
    },
    /// Generate a first project for a catalog board (the setup flow's
    /// provision step, P03/P06) and install it. Creation-shaped: it touches
    /// no existing project, and unlike `EnsureExampleSeeded` it is NOT
    /// seed-once — every provision makes its own package.
    GenerateForBoard {
        board_id: String,
    },
    UpsertRegisteredDevice(RegisteredDevice),
    /// Rename a registered device (D34). Registry-only — the identity
    /// write-back to a live device is the studio controller's.
    RenameRegisteredDevice {
        uid: String,
        name: String,
    },
    /// Remove a device from the registry (D34 hygiene). Idempotent.
    ForgetRegisteredDevice {
        uid: String,
    },
    /// Lazy re-key at sighting (device identity design §4): a board that
    /// was remembered under a stamped uid has resolved to a derived one,
    /// so its row moves — or merges, when both rows exist. Runs BEFORE
    /// the sighting upsert, under the same catalog lock every other
    /// registry write takes. Idempotent: nothing under `old_uid` is a
    /// no-op.
    RekeyRegisteredDevice {
        old_uid: String,
        new_uid: String,
        /// Canonical origin string ([`HardwareId`](crate::app::places::HardwareId)'s
        /// `Display`) recorded on the surviving row.
        hardware_id: String,
    },
    /// Connect-as-pull (D8) for a project NOT open in this tab: bank the
    /// observed device copy into that project's history (no-op when the
    /// hash is known) and refresh the registry entry. The host takes the
    /// project's lock first (structural ordering) — refusal means the
    /// project is open in another tab and the observation is skipped by
    /// the caller (that tab owns the history subtree).
    RecordDeviceObservation {
        project_uid: String,
        device: RegisteredDevice,
        observed: lpc_history::ContentHash,
        files: Vec<(String, Vec<u8>)>,
    },
    /// Adopt a device's unknown project as a new library package (D11),
    /// keeping its on-device uid; registry entry recorded.
    AdoptDevicePackage {
        device: RegisteredDevice,
        files: Vec<(String, Vec<u8>)>,
    },
    /// Diverged verb (D11): a banked observed version becomes the
    /// project's new head.
    AdoptObservedVersion {
        project_uid: String,
        observed: lpc_history::ContentHash,
    },
    /// Diverged verb (D11): fork a banked observed version into a new
    /// project named after the device (D9).
    ForkObservedVersion {
        project_uid: String,
        observed: lpc_history::ContentHash,
        device_name: String,
    },
    /// Migrate a package's own bytes to the current project format and
    /// save the result (P5's Upgrade verb; the same body P3 runs on open).
    ///
    /// The migrated bytes are born HERE, in the library — never on the
    /// device (D14 / ADR 2026-07-05 decision 5). Idempotent: a package
    /// already at the current format is a no-op, and the outcome's
    /// `upgraded_from` is `None`.
    UpgradePackageFormat {
        project_uid: String,
    },
    /// Record a completed push: history `Pushed` event + device
    /// association.
    RecordPush {
        project_uid: String,
        device: RegisteredDevice,
        version: lpc_history::ContentHash,
    },
    /// Install a project whose content AND history already exist in full —
    /// a cloud tracking copy (`open_shared`, P6) or a locally forked line —
    /// both file sets verbatim, nothing minted, nothing re-saved.
    ///
    /// This is deliberately NOT [`CatalogOp::ImportZip`]'s shape: imports
    /// re-mint uids and start a fresh history, while a synced install's
    /// whole point is that the uid in the package manifest is preserved
    /// (D17) and the event log arrived with the files (the project's real
    /// history, origin and all). The provenance rides along for the
    /// `.lp/meta.json` sidecar and for the host's publish-trigger decision
    /// (a fork wants auto-publish; a tracking copy is the *service's* copy
    /// and must not be offered back).
    InstallSyncedProject {
        /// The display label the dated slug derives from.
        name: String,
        /// The working copy, verbatim — must include a manifest carrying
        /// the project's uid.
        package_files: Vec<(String, Vec<u8>)>,
        /// The history root, verbatim — must include a non-empty event log.
        history_files: Vec<(String, Vec<u8>)>,
        provenance: super::package_meta::PackageProvenance,
    },
}

/// What a catalog transaction produced.
#[derive(Clone, Debug, Default)]
pub struct CatalogOutcome {
    /// The touched/created package, where the op has one (everything but
    /// `Delete` and the registry ops).
    pub summary: Option<PackageSummary>,
    /// For the import ops: the older format the incoming files were
    /// migrated from before they were installed. `None` when they arrived
    /// current — the ordinary case — so the import notice can say "imported
    /// and upgraded from format 4" only when that is what happened.
    pub upgraded_from: Option<u32>,
}

/// A project opened for writing: per-project fs handles whose backing
/// store the host owns (mount + flusher + the held project lock).
pub struct OpenedProject {
    pub uid: PrefixedUid,
    pub slug: String,
    pub package_fs: Rc<RefCell<dyn LpFs>>,
    pub history_fs: Rc<RefCell<dyn LpFs>>,
}

/// Library host failure, split so the UI can be friendly about the
/// multi-tab refusals.
#[derive(Debug, Clone)]
pub enum LibraryHostError {
    /// The project lock is held by another tab.
    OpenElsewhere {
        key: String,
    },
    /// Guarded same-tab self-conflict (unreachable via the UI — an open
    /// quiesces the previous open first, releasing its same-tab lock,
    /// and the sim's detached run holds no project store open — but
    /// hosts guard it).
    OpenInThisTab {
        uid: String,
    },
    NotFound(String),
    /// Catalog lock retry exhausted.
    Busy(String),
    /// The library declined the request, and the message is the whole
    /// answer — an import whose project format is below the floor or ahead
    /// of this build. Surfaces verbatim, without a "library:" prefix.
    Refused(String),
    /// Everything else, message for the log.
    Host(String),
}

impl core::fmt::Display for LibraryHostError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            LibraryHostError::OpenElsewhere { key } => {
                write!(f, "{key} is open in another tab")
            }
            LibraryHostError::OpenInThisTab { uid } => {
                write!(f, "{uid} is open in this tab")
            }
            LibraryHostError::NotFound(key) => write!(f, "not found: {key}"),
            LibraryHostError::Busy(m) => write!(f, "library busy: {m}"),
            LibraryHostError::Refused(m) => write!(f, "{m}"),
            LibraryHostError::Host(m) => write!(f, "library host: {m}"),
        }
    }
}

impl From<LibraryError> for LibraryHostError {
    fn from(e: LibraryError) -> Self {
        match e {
            LibraryError::NotFound(key) => LibraryHostError::NotFound(key),
            other => LibraryHostError::Host(other.to_string()),
        }
    }
}

/// Host failures surface to the UI with friendly wording for the
/// multi-tab refusals (M4b: refuse kindly, never corrupt).
impl From<LibraryHostError> for crate::UiError {
    fn from(error: LibraryHostError) -> Self {
        match error {
            LibraryHostError::OpenElsewhere { .. } => crate::UiError::UnsupportedAction(
                "This project is open in another tab — close it there first".to_string(),
            ),
            LibraryHostError::OpenInThisTab { .. } => crate::UiError::UnsupportedAction(
                "This project is open in this tab — close it before changing it".to_string(),
            ),
            LibraryHostError::Busy(_) => crate::UiError::UnsupportedAction(
                "The library is busy in another tab — try again in a moment".to_string(),
            ),
            LibraryHostError::Refused(message) => crate::UiError::UnsupportedAction(message),
            LibraryHostError::NotFound(key) => {
                crate::UiError::MissingSession(format!("library: not found: {key}"))
            }
            LibraryHostError::Host(message) => {
                crate::UiError::MissingSession(format!("library: {message}"))
            }
        }
    }
}

/// The platform seam to the local library. See the module docs for the
/// locking contract each method implies.
pub trait LibraryHost {
    /// Fresh read-only snapshot of the library (manifests, meta, event
    /// logs; history payloads excluded). Feeds the gallery; takes no locks.
    fn catalog_snapshot(
        &self,
    ) -> LocalBoxFuture<'_, Result<Rc<RefCell<dyn LpFs>>, LibraryHostError>>;

    /// Run one catalog mutation as a locked transaction.
    fn catalog(
        &self,
        op: CatalogOp,
    ) -> LocalBoxFuture<'_, Result<CatalogOutcome, LibraryHostError>>;

    /// Open a project for writing. `key` is a slug or `prj…` uid; the
    /// host resolves it, acquires the project lock, and re-verifies the
    /// mapping under the lock.
    fn open_project<'a>(
        &'a self,
        key: &'a str,
    ) -> LocalBoxFuture<'a, Result<OpenedProject, LibraryHostError>>;

    /// Final flush + release of the project lock. Idempotent — closing a
    /// project this tab does not hold open is a no-op.
    fn close_project<'a>(&'a self, uid: &'a str) -> LocalBoxFuture<'a, ()>;

    /// `prj…` uids locked by OTHER tabs (own holds filtered out) — the
    /// gallery's "open in another tab" badges.
    fn open_elsewhere_uids(&self) -> LocalBoxFuture<'_, Vec<String>>;

    /// A save landed in the library copy of `uid` — hosts broadcast this
    /// so other tabs' galleries refresh. Fire-and-forget.
    fn notify_saved(&self, _uid: &str) {}
}

/// Apply one [`CatalogOp`] through a [`LibraryStore`] — the sync middle
/// every host wraps in its own locking/mounting. `now` is the host's
/// wall clock (hosts are edges; they own time).
pub fn apply_catalog_op(
    store: &LibraryStore,
    op: CatalogOp,
    now: f64,
) -> Result<CatalogOutcome, LibraryHostError> {
    // Set by the import ops when the incoming files had to be migrated
    // forward before they could be installed.
    let mut upgraded_from = None;
    let summary = match op {
        CatalogOp::Create { name, files } => Some(match files {
            None => store.create(&name, now)?,
            Some(files) => store.install_package(&name, &files, PackageProvenance::Created, now)?,
        }),
        CatalogOp::Rename { uid, new_slug } => {
            let uid = parse_uid(&uid)?;
            store.rename(uid, &new_slug)?;
            Some(summary_for(store, uid)?)
        }
        CatalogOp::Duplicate { uid } => Some(store.duplicate(parse_uid(&uid)?, now)?),
        CatalogOp::Delete { uid } => {
            store.delete(parse_uid(&uid)?)?;
            None
        }
        CatalogOp::ImportZip { file_name, bytes } => {
            let outcome = super::package_zip::import_zip(store, &bytes, now)
                .map_err(|error| import_refusal(&error, format!("could not import {file_name}")))?;
            upgraded_from = outcome.upgraded_from;
            Some(outcome.summary)
        }
        CatalogOp::ImportJson { text } => {
            let outcome = super::package_zip::import_json(store, &text, now).map_err(|error| {
                import_refusal(&error, String::from("could not paste this project"))
            })?;
            upgraded_from = outcome.upgraded_from;
            Some(outcome.summary)
        }
        CatalogOp::EnsureExampleSeeded { id } => Some(ensure_example_seeded(store, &id, now)?),
        CatalogOp::GenerateForBoard { board_id } => {
            Some(generate_for_board(store, &board_id, now)?)
        }
        CatalogOp::UpsertRegisteredDevice(device) => {
            // merge semantics: sight-only upserts (association None) must
            // not erase what was last pushed
            crate::app::places::device_session::upsert_device_merged(store, device)
                .map_err(|e| LibraryHostError::Host(e.to_string()))?;
            None
        }
        CatalogOp::RenameRegisteredDevice { uid, name } => {
            crate::app::places::DeviceRegistry::new(store.fs_handle())
                .rename(&uid, &name)
                .map_err(LibraryHostError::from)?;
            None
        }
        CatalogOp::ForgetRegisteredDevice { uid } => {
            crate::app::places::DeviceRegistry::new(store.fs_handle())
                .forget(&uid)
                .map_err(LibraryHostError::from)?;
            None
        }
        CatalogOp::RekeyRegisteredDevice {
            old_uid,
            new_uid,
            hardware_id,
        } => {
            crate::app::places::DeviceRegistry::new(store.fs_handle())
                .rekey_or_merge(&old_uid, &new_uid, &hardware_id)
                .map_err(LibraryHostError::from)?;
            None
        }
        CatalogOp::RecordDeviceObservation {
            project_uid,
            device,
            observed,
            files,
        } => {
            crate::app::places::device_session::record_device_observation(
                store,
                &project_uid,
                &device,
                observed,
                &files,
                now,
            )?;
            Some(summary_for(store, parse_uid(&project_uid)?)?)
        }
        CatalogOp::AdoptDevicePackage { device, files } => Some(
            crate::app::places::device_session::adopt_device_package(store, &device, &files, now)?,
        ),
        CatalogOp::AdoptObservedVersion {
            project_uid,
            observed,
        } => {
            crate::app::places::device_session::adopt_observed_version(
                store,
                &project_uid,
                observed,
                now,
            )?;
            Some(summary_for(store, parse_uid(&project_uid)?)?)
        }
        CatalogOp::ForkObservedVersion {
            project_uid,
            observed,
            device_name,
        } => Some(crate::app::places::device_session::fork_observed_version(
            store,
            &project_uid,
            observed,
            &device_name,
            now,
        )?),
        CatalogOp::UpgradePackageFormat { project_uid } => {
            let uid = parse_uid(&project_uid)?;
            let mut handle = store.open(uid)?;
            // A format refusal is already a full user-facing sentence
            // (what was found, what was expected, the remedy), so it
            // travels as `Refused` and reaches the card verbatim.
            match super::package_upgrade::migrate_handle_to_current(&mut handle, now) {
                Ok(report) => upgraded_from = report.map(|report| report.from),
                Err(LibraryError::Format(message)) => {
                    return Err(LibraryHostError::Refused(message));
                }
                Err(other) => return Err(other.into()),
            }
            Some(summary_for(store, uid)?)
        }
        CatalogOp::RecordPush {
            project_uid,
            device,
            version,
        } => {
            crate::app::places::device_session::record_push(
                store,
                &project_uid,
                &device,
                version,
                now,
            )?;
            Some(summary_for(store, parse_uid(&project_uid)?)?)
        }
        CatalogOp::InstallSyncedProject {
            name,
            package_files,
            history_files,
            provenance,
        } => Some(store.install_synced(&name, &package_files, &history_files, provenance, now)?),
    };
    Ok(CatalogOutcome {
        summary,
        upgraded_from,
    })
}

/// Wrap an import failure for the UI.
///
/// A format refusal is already a full user-facing sentence naming what was
/// found, what was expected and a remedy ([`LibraryError::Format`]), so it
/// travels as its own [`LibraryHostError::Refused`] and reaches the user as
/// an actionable message rather than as library plumbing. Everything else
/// keeps the generic "could not import X: …" shape.
fn import_refusal(error: &LibraryError, context: String) -> LibraryHostError {
    match error {
        LibraryError::Format(message) => LibraryHostError::Refused(format!("{context}: {message}")),
        other => LibraryHostError::Host(format!("{context}: {other}")),
    }
}

/// The seed-once transaction body: the package seeded from example `id`,
/// installing the embedded files on first use.
fn ensure_example_seeded(
    store: &LibraryStore,
    id: &str,
    now: f64,
) -> Result<PackageSummary, LibraryHostError> {
    if let Some(existing) = store.find_seeded_from(id)? {
        return Ok(existing);
    }
    let example = crate::app::home::embedded_example(id)
        .ok_or_else(|| LibraryHostError::NotFound(format!("unknown example {id}")))?;
    Ok(store.install_package(
        example.name,
        &example.files(),
        PackageProvenance::SeededFrom {
            source: id.to_string(),
        },
        now,
    )?)
}

/// Install a freshly generated first project for `board_id`.
///
/// Provenance is [`PackageProvenance::Created`]: the package is authored
/// here and now, not seeded from a bundled package (its meteor effect is a
/// vendored copy, which the effect module's own provenance records).
fn generate_for_board(
    store: &LibraryStore,
    board_id: &str,
    now: f64,
) -> Result<PackageSummary, LibraryHostError> {
    let generated = crate::app::home::generate_board_project(board_id)
        .map_err(|error| LibraryHostError::Host(error.to_string()))?;
    Ok(store.install_package(
        &generated.name,
        &generated.files,
        PackageProvenance::Created,
        now,
    )?)
}

/// Resolve + open a project through a [`LibraryStore`] — the sync middle
/// of every host's `open_project` (wrapped in resolve → lock → re-verify
/// by the real host).
pub fn open_project_via_store(
    store: &LibraryStore,
    key: &str,
) -> Result<OpenedProject, LibraryHostError> {
    let uid = store.resolve_key(key)?;
    let handle = store.open(uid)?;
    Ok(OpenedProject {
        uid: handle.uid,
        slug: handle.slug,
        package_fs: handle.package_fs,
        history_fs: handle.history_fs,
    })
}

fn parse_uid(uid: &str) -> Result<PrefixedUid, LibraryHostError> {
    uid.parse()
        .map_err(|e| LibraryHostError::Host(format!("invalid uid {uid:?}: {e}")))
}

fn summary_for(store: &LibraryStore, uid: PrefixedUid) -> Result<PackageSummary, LibraryHostError> {
    store
        .list()?
        .into_iter()
        .find(|summary| summary.uid == uid)
        .ok_or_else(|| LibraryHostError::NotFound(uid.to_string()))
}

/// Memory-backed host for tests and host builds: one shared `LpFsMemory`
/// plays OPFS, ops apply synchronously, every future is immediately
/// ready. Refusals are test-settable via [`Self::set_open_elsewhere`].
pub struct MemoryLibraryHost {
    store: LibraryStore,
    clock: Rc<dyn Fn() -> f64>,
    open_elsewhere: RefCell<Vec<String>>,
    closed: RefCell<Vec<String>>,
    saved_notifications: RefCell<Vec<String>>,
}

impl MemoryLibraryHost {
    /// Wrap an existing store (tests usually pre-install packages through
    /// it and keep their own clone for direct assertions).
    pub fn new(store: LibraryStore, clock: Rc<dyn Fn() -> f64>) -> Self {
        Self {
            store,
            clock,
            open_elsewhere: RefCell::new(Vec::new()),
            closed: RefCell::new(Vec::new()),
            saved_notifications: RefCell::new(Vec::new()),
        }
    }

    /// Mark project uids as held by "another tab": opens and structural
    /// catalog ops targeting them refuse with `OpenElsewhere`.
    pub fn set_open_elsewhere(&self, uids: Vec<String>) {
        *self.open_elsewhere.borrow_mut() = uids;
    }

    /// Uids passed to `close_project`, in order (lock-release assertions).
    pub fn closed_projects(&self) -> Vec<String> {
        self.closed.borrow().clone()
    }

    /// Uids passed to `notify_saved`, in order.
    pub fn saved_notifications(&self) -> Vec<String> {
        self.saved_notifications.borrow().clone()
    }

    fn refuses(&self, uid: &str) -> bool {
        self.open_elsewhere.borrow().iter().any(|held| held == uid)
    }
}

impl LibraryHost for MemoryLibraryHost {
    fn catalog_snapshot(
        &self,
    ) -> LocalBoxFuture<'_, Result<Rc<RefCell<dyn LpFs>>, LibraryHostError>> {
        Box::pin(core::future::ready(Ok(self.store.fs_handle())))
    }

    fn catalog(
        &self,
        op: CatalogOp,
    ) -> LocalBoxFuture<'_, Result<CatalogOutcome, LibraryHostError>> {
        // structural ops on a project "open in another tab" refuse the way
        // the real host's project-before-catalog try-lock does
        let refused = match &op {
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
            } => self.refuses(uid).then(|| uid.clone()),
            _ => None,
        };
        let result = match refused {
            Some(key) => Err(LibraryHostError::OpenElsewhere { key }),
            None => apply_catalog_op(&self.store, op, (self.clock)()),
        };
        Box::pin(core::future::ready(result))
    }

    fn open_project<'a>(
        &'a self,
        key: &'a str,
    ) -> LocalBoxFuture<'a, Result<OpenedProject, LibraryHostError>> {
        let result = (|| {
            let uid = self.store.resolve_key(key)?;
            if self.refuses(&uid.to_string()) {
                return Err(LibraryHostError::OpenElsewhere {
                    key: key.to_string(),
                });
            }
            open_project_via_store(&self.store, key)
        })();
        Box::pin(core::future::ready(result))
    }

    fn close_project<'a>(&'a self, uid: &'a str) -> LocalBoxFuture<'a, ()> {
        self.closed.borrow_mut().push(uid.to_string());
        Box::pin(core::future::ready(()))
    }

    fn open_elsewhere_uids(&self) -> LocalBoxFuture<'_, Vec<String>> {
        Box::pin(core::future::ready(self.open_elsewhere.borrow().clone()))
    }

    fn notify_saved(&self, uid: &str) {
        self.saved_notifications.borrow_mut().push(uid.to_string());
    }
}
