//! Connect-is-a-pull (D8): what Studio learns — and banks — when it
//! attaches to a real device.
//!
//! On attach the studio pulls the device's project copy, reads its
//! identity (`/.lp/device.json`) and manifest, and relates the content to
//! the library:
//!
//! - **Known uid, known hash** → nothing to store (the library already
//!   knows this version); a `Connected` observation is still recorded.
//! - **Known uid, unknown hash (diverged)** → the copy is BANKED: bytes
//!   snapshotted into that project's history and marked as a device
//!   observation, so push is never destructive and keep-both/fork always
//!   has the bytes (D8/D11).
//! - **Unknown uid** → the device's project is ADOPTED as a new library
//!   package (keeping its on-device uid) with pulled provenance.
//!
//! Persistence honors the M4b locking model: history subtrees belong to
//! whoever holds the project open. The studio controller routes an
//! observation for the project open in THIS tab through the active
//! `PackageHandle`; everything else runs as a catalog transaction
//! ([`crate::app::library::CatalogOp::RecordDeviceObservation`] /
//! [`CatalogOp::AdoptDevicePackage`](crate::app::library::CatalogOp)).
//! A project open in ANOTHER tab cannot be banked from here — the
//! observation is skipped with a log line (the classification still
//! reaches the UI; the lock, not the badge, is the truth).

use lpc_history::{ContentHash, EventLog, PrefixedUid, SnapshotStore, SyncRelation};
use lpfs::{LpFs, LpFsMemory, LpPath};

use crate::app::library::{LibraryError, LibraryStore, PackageHandle, PackageProvenance};
use crate::app::places::{DEVICE_IDENTITY_PATH, DeviceIdentity, DeviceRegistry, RegisteredDevice};
use crate::app::server::studio_server_client::StudioServerClient;
use crate::{UiError, UiLogDraft};

/// What Studio knows about the attached device's contents, computed at
/// attach time and held while the device stays connected.
#[derive(Clone, Debug, PartialEq)]
pub struct DeviceSyncState {
    /// Stamped identity, when `/.lp/device.json` exists on the device.
    pub identity: Option<DeviceIdentity>,
    pub content: DeviceContent,
}

/// The device's project contents, related to the library (D11).
#[derive(Clone, Debug, PartialEq)]
pub enum DeviceContent {
    /// The device's project storage is empty.
    Empty,
    /// A project whose uid the library knows.
    Known {
        project_uid: String,
        /// Library slug at pull time (display only).
        slug: String,
        observed: ContentHash,
        relation: SyncRelation,
    },
    /// A project the library did not know — adopted at connect.
    Adopted {
        project_uid: String,
        slug: String,
        observed: ContentHash,
    },
    /// A project the library doesn't know, on a device with no stamped
    /// identity: adoption waits for provisioning to stamp a `dev_` uid
    /// (the deploy wizard re-pulls after stamping). Classification only —
    /// nothing is persisted for anonymous hardware.
    PendingIdentity { observed: ContentHash },
    /// Files exist but the manifest is missing/unparseable. Not fatal:
    /// the link stays usable (flash/erase still work).
    Unreadable { detail: String },
}

/// The raw pull: every file in the device's project storage, plus the
/// pieces read out of it. Pure wire work — no library access.
pub struct PulledDeviceCopy {
    /// Relative path → bytes (tombstones dropped; a full pull has none).
    pub files: Vec<(String, Vec<u8>)>,
    /// Canonical content hash reported by the device (lph1, /.lp excluded).
    pub observed: ContentHash,
    /// `project.json` parsed as JSON (uid/name may still be absent).
    pub has_manifest: bool,
    pub manifest_uid: Option<String>,
    pub manifest_name: Option<String>,
    pub identity: Option<DeviceIdentity>,
    pub logs: Vec<UiLogDraft>,
}

/// Pull the device's project copy over the wire (full pull from version
/// zero on the single project storage id).
pub async fn pull_device_copy(
    server: &mut StudioServerClient,
    storage_id: &str,
) -> Result<PulledDeviceCopy, UiError> {
    let mut logs = Vec::new();
    let pulled = match server
        .pull_changed_files(storage_id, lpc_model::FsVersion::new(0))
        .await
    {
        Ok(pulled) => pulled,
        // older firmware reports a never-pushed storage dir as an fs
        // error instead of an empty set — treat it as the empty device
        // it is (the current wire returns empty; this is the fallback)
        Err(error) if error.to_string().contains("no such file or directory") => {
            return Ok(PulledDeviceCopy {
                files: Vec::new(),
                observed: empty_package_hash(),
                has_manifest: false,
                manifest_uid: None,
                manifest_name: None,
                identity: None,
                logs,
            });
        }
        Err(error) => return Err(error),
    };
    logs.extend(pulled.logs);
    let files: Vec<(String, Vec<u8>)> = pulled
        .updates
        .into_iter()
        .filter_map(|update| {
            let content = update.content?;
            Some((update.path.trim_start_matches('/').to_string(), content))
        })
        .collect();

    let (observed_hex, hash_logs) = server.hash_package(storage_id).await?;
    logs.extend(hash_logs);
    let observed: ContentHash = observed_hex
        .parse()
        .map_err(|e| UiError::MissingSession(format!("device hash {observed_hex:?}: {e}")))?;

    let manifest = files
        .iter()
        .find(|(path, _)| path == "project.json")
        .and_then(|(_, bytes)| serde_json::from_slice::<serde_json::Value>(bytes).ok());
    let has_manifest = manifest.is_some();
    let manifest_uid = manifest
        .as_ref()
        .and_then(|m| m.get("uid"))
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let manifest_name = manifest
        .as_ref()
        .and_then(|m| m.get("name"))
        .and_then(|v| v.as_str())
        .map(str::to_string);

    let identity = files
        .iter()
        .find(|(path, _)| format!("/{path}") == DEVICE_IDENTITY_PATH)
        .and_then(|(_, bytes)| DeviceIdentity::from_json_bytes(bytes).ok());

    Ok(PulledDeviceCopy {
        files,
        observed,
        has_manifest,
        manifest_uid,
        manifest_name,
        identity,
        logs,
    })
}

/// The device's registry entry for this connect, merging what the pull
/// learned with the caller's clock. Association is preserved by the merge
/// in [`upsert_device_merged`], so callers pass `None` here.
pub fn registry_entry_for(identity: &DeviceIdentity, now: f64) -> RegisteredDevice {
    RegisteredDevice {
        uid: identity.uid.clone(),
        name: identity.name.clone(),
        last_seen_at: now,
        association: None,
    }
}

/// Upsert a device into the registry, preserving an existing association
/// when the incoming entry has none (associations change on push, not on
/// sight).
pub fn upsert_device_merged(
    store: &LibraryStore,
    mut device: RegisteredDevice,
) -> Result<(), LibraryError> {
    let registry = DeviceRegistry::new(store.fs_handle());
    if device.association.is_none() {
        if let Some(existing) = registry
            .list()?
            .into_iter()
            .find(|entry| entry.uid == device.uid)
        {
            device.association = existing.association;
        }
    }
    registry.upsert(device)
}

/// Bank a device observation into a project's history: snapshot the
/// pulled bytes when the hash is new (so keep-both/restore always has
/// them), then record the `Connected` event. Idempotent for known hashes
/// (no new snapshot; the observation event still appends).
///
/// `handle` must be the project's live handle — the store-side caller
/// opens one inside a catalog transaction; the studio controller passes
/// the ACTIVE handle when the project is open in this tab.
pub fn bank_observation_on_handle(
    handle: &mut PackageHandle,
    device: PrefixedUid,
    observed: ContentHash,
    files: &[(String, Vec<u8>)],
    now: f64,
) -> Result<(), LibraryError> {
    if !handle.history.knows(observed) {
        let staged = stage_package_files(files)?;
        let history_fs = handle.history_fs.borrow();
        let snapshots = SnapshotStore::new(&*history_fs);
        let (banked, _) = snapshots
            .put_package(&staged)
            .map_err(|e| LibraryError::History(e.to_string()))?;
        if banked != observed {
            // record anyway: the observation is real; the mismatch is
            // diagnostic (hash spec drift between device and studio)
            log::warn!("banked device copy hashes {banked}, device reported {observed}");
        }
    }
    let event = handle.history.record_connect(device, observed, now);
    let history_fs = handle.history_fs.borrow();
    EventLog::new(&*history_fs)
        .append(&event)
        .map_err(|e| LibraryError::History(e.to_string()))
}

/// Catalog-transaction body: bank an observation for a project that is
/// NOT open in this tab (the host holds the project + catalog locks).
pub fn record_device_observation(
    store: &LibraryStore,
    project_uid: &str,
    device: &RegisteredDevice,
    observed: ContentHash,
    files: &[(String, Vec<u8>)],
    now: f64,
) -> Result<(), LibraryError> {
    let uid: PrefixedUid = project_uid
        .parse()
        .map_err(|e| LibraryError::Manifest(format!("project uid {project_uid:?}: {e}")))?;
    let device_uid: PrefixedUid = device
        .uid
        .parse()
        .map_err(|e| LibraryError::Manifest(format!("device uid {:?}: {e}", device.uid)))?;
    let mut handle = store.open(uid)?;
    bank_observation_on_handle(&mut handle, device_uid, observed, files, now)?;
    upsert_device_merged(store, device.clone())
}

/// Catalog-transaction body: adopt a device's unknown project as a new
/// library package (D11), keeping its on-device uid so future connects
/// match. `/.lp/*` is stripped (identity belongs to the device; a fresh
/// provenance sidecar is written by the install).
pub fn adopt_device_package(
    store: &LibraryStore,
    device: &RegisteredDevice,
    files: &[(String, Vec<u8>)],
    now: f64,
) -> Result<crate::app::library::PackageSummary, LibraryError> {
    let device_uid: PrefixedUid = device
        .uid
        .parse()
        .map_err(|e| LibraryError::Manifest(format!("device uid {:?}: {e}", device.uid)))?;
    let package_files: Vec<(String, Vec<u8>)> = files
        .iter()
        .filter(|(path, _)| !path.starts_with(".lp/"))
        .cloned()
        .collect();
    let label = package_files
        .iter()
        .find(|(path, _)| path == "project.json")
        .and_then(|(_, bytes)| serde_json::from_slice::<serde_json::Value>(bytes).ok())
        .and_then(|m| m.get("name").and_then(|v| v.as_str()).map(str::to_string))
        .unwrap_or_else(|| device.name.clone());
    let summary = store.install_package(
        &label,
        &package_files,
        PackageProvenance::PulledFromDevice {
            device_uid: device.uid.clone(),
            device_name: device.name.clone(),
        },
        now,
    )?;
    // the install's first save IS the pulled content; the observation
    // event makes the device sighting explicit on the new line
    let mut handle = store.open(summary.uid)?;
    let observed = handle.content_hash()?;
    let event = handle.history.record_connect(device_uid, observed, now);
    {
        let history_fs = handle.history_fs.borrow();
        EventLog::new(&*history_fs)
            .append(&event)
            .map_err(|e| LibraryError::History(e.to_string()))?;
    }
    upsert_device_merged(store, device.clone())?;
    Ok(summary)
}

/// Diverged verb (D11) — ADOPT: make a banked observed version the
/// project's new head. The bytes were banked at connect; this
/// materializes them over the package content and records the save.
pub fn adopt_observed_version(
    store: &LibraryStore,
    project_uid: &str,
    observed: ContentHash,
    now: f64,
) -> Result<(), LibraryError> {
    let uid: PrefixedUid = project_uid
        .parse()
        .map_err(|e| LibraryError::Manifest(format!("project uid {project_uid:?}: {e}")))?;
    let mut handle = store.open(uid)?;
    if !handle.history.knows(observed) {
        return Err(LibraryError::History(format!(
            "version {observed} was never observed for this project"
        )));
    }
    replace_package_content_with_snapshot(&handle, observed)?;
    handle.record_save(now)?;
    Ok(())
}

/// Diverged verb (D11) — KEEP BOTH: fork a banked observed version into
/// a new project named after the device (D9). The original line is
/// untouched.
pub fn fork_observed_version(
    store: &LibraryStore,
    project_uid: &str,
    observed: ContentHash,
    device_name: &str,
    now: f64,
) -> Result<crate::app::library::PackageSummary, LibraryError> {
    let uid: PrefixedUid = project_uid
        .parse()
        .map_err(|e| LibraryError::Manifest(format!("project uid {project_uid:?}: {e}")))?;
    let parent = store.open(uid)?;
    if !parent.history.knows(observed) {
        return Err(LibraryError::History(format!(
            "version {observed} was never observed for this project"
        )));
    }
    let staged = LpFsMemory::new();
    {
        let history_fs = parent.history_fs.borrow();
        SnapshotStore::new(&*history_fs)
            .materialize(&observed, &staged)
            .map_err(|e| LibraryError::History(e.to_string()))?;
    }
    let mut files = Vec::new();
    for path in staged.list_dir(LpPath::new("/"), true)? {
        if staged.is_dir(&path).unwrap_or(false) {
            continue;
        }
        let bytes = staged.read_file(&path)?;
        files.push((path.as_str().trim_start_matches('/').to_string(), bytes));
    }
    store.install_files_with_fresh_uid(
        device_name,
        &files,
        PackageProvenance::ForkedFrom {
            parent_project: project_uid.to_string(),
            parent_version: observed.to_string(),
        },
        now,
    )
}

/// Record a completed push (catalog body): history `Pushed` event plus
/// the device association (what was last pushed — D11's behind/ahead
/// baseline).
pub fn record_push(
    store: &LibraryStore,
    project_uid: &str,
    device: &RegisteredDevice,
    version: ContentHash,
    now: f64,
) -> Result<(), LibraryError> {
    let uid: PrefixedUid = project_uid
        .parse()
        .map_err(|e| LibraryError::Manifest(format!("project uid {project_uid:?}: {e}")))?;
    let device_uid: PrefixedUid = device
        .uid
        .parse()
        .map_err(|e| LibraryError::Manifest(format!("device uid {:?}: {e}", device.uid)))?;
    let mut handle = store.open(uid)?;
    let event = handle
        .history
        .record_push(version, device_uid, now, None)
        .map_err(|e| LibraryError::History(e.to_string()))?;
    {
        let history_fs = handle.history_fs.borrow();
        EventLog::new(&*history_fs)
            .append(&event)
            .map_err(|e| LibraryError::History(e.to_string()))?;
    }
    let mut entry = device.clone();
    entry.association = Some(lpc_history::DeviceAssociation {
        device: device_uid,
        project: uid,
        version,
        at: now,
    });
    upsert_device_merged(store, entry)
}

/// Replace a package's content files with a banked snapshot (`/.lp/*`
/// sidecars survive — they are library metadata, not content).
fn replace_package_content_with_snapshot(
    handle: &PackageHandle,
    observed: ContentHash,
) -> Result<(), LibraryError> {
    let package_fs = handle.package_fs.borrow();
    for path in package_fs.list_dir(LpPath::new("/"), true)? {
        if package_fs.is_dir(&path).unwrap_or(false) || path.as_str().starts_with("/.lp/") {
            continue;
        }
        package_fs.delete_file(&path)?;
    }
    let history_fs = handle.history_fs.borrow();
    SnapshotStore::new(&*history_fs)
        .materialize(&observed, &*package_fs)
        .map_err(|e| LibraryError::History(e.to_string()))
}

/// The canonical hash of an empty package (a fresh device's storage).
fn empty_package_hash() -> ContentHash {
    lpc_history::hash_package(&LpFsMemory::new())
        .map(|(hash, _)| hash)
        .expect("hashing an empty package cannot fail")
}

/// Stage pulled files as an in-memory package for snapshotting (`/.lp/*`
/// excluded — it is outside the lph1 hash anyway, and the identity file
/// belongs to the device, not the project).
fn stage_package_files(files: &[(String, Vec<u8>)]) -> Result<LpFsMemory, LibraryError> {
    let staged = LpFsMemory::new();
    for (relative, bytes) in files {
        if relative.starts_with(".lp/") {
            continue;
        }
        let path = format!("/{relative}");
        staged.write_file(LpPath::new(&path), bytes)?;
    }
    Ok(staged)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use lpc_history::{EventKind, UidPrefix};
    use lpfs::LpFsMemory;

    use super::*;

    fn store() -> LibraryStore {
        let counter = Rc::new(RefCell::new(0u8));
        LibraryStore::new(
            Rc::new(RefCell::new(LpFsMemory::new())),
            Rc::new(move || {
                *counter.borrow_mut() += 1;
                [*counter.borrow(); 16]
            }),
            Rc::new(|| "2026-07-10-0900".to_string()),
        )
    }

    fn device() -> RegisteredDevice {
        RegisteredDevice {
            uid: PrefixedUid::mint(UidPrefix::Device, &[7u8; 16]).to_string(),
            name: "Porch sign".to_string(),
            last_seen_at: 50.0,
            association: None,
        }
    }

    fn project_files(marker: &str) -> Vec<(String, Vec<u8>)> {
        vec![
            (
                "project.json".to_string(),
                format!(r#"{{"kind":"Project","name":"Demo {marker}","nodes":{{}}}}"#).into_bytes(),
            ),
            ("shader.glsl".to_string(), marker.as_bytes().to_vec()),
        ]
    }

    #[test]
    fn diverged_observation_banks_bytes_once_and_records_connect() {
        let store = store();
        let summary = store
            .install_package(
                "Demo",
                &project_files("v1"),
                PackageProvenance::Created,
                1.0,
            )
            .unwrap();
        let device = device();

        // a foreign device copy: same uid, different content
        let mut foreign = project_files("v-device");
        foreign.push((".lp/device.json".to_string(), b"{}".to_vec()));
        let staged = stage_package_files(&foreign).unwrap();
        let observed = lpc_history::hash_package(&staged).unwrap().0;

        record_device_observation(
            &store,
            &summary.uid.to_string(),
            &device,
            observed,
            &foreign,
            60.0,
        )
        .unwrap();

        let handle = store.open(summary.uid).unwrap();
        assert!(handle.history.knows(observed), "diverged copy is banked");
        assert_eq!(
            handle.history.classify(observed),
            SyncRelation::Diverged,
            "known-but-unsaved: valid fork parent, not on the line"
        );
        assert!(matches!(
            handle.history.events().last().unwrap().kind,
            EventKind::Connected { .. }
        ));
        // the banked bytes are restorable
        let history_fs = handle.history_fs.borrow();
        let snapshots = SnapshotStore::new(&*history_fs);
        let restored = LpFsMemory::new();
        snapshots.materialize(&observed, &restored).unwrap();
        assert_eq!(
            restored.read_file(LpPath::new("/shader.glsl")).unwrap(),
            b"v-device"
        );
        drop(history_fs);

        // reconnect with the same hash: no new snapshot work, event appends
        let events_before = handle.history.events().len();
        record_device_observation(
            &store,
            &summary.uid.to_string(),
            &device,
            observed,
            &foreign,
            70.0,
        )
        .unwrap();
        let handle = store.open(summary.uid).unwrap();
        assert_eq!(handle.history.events().len(), events_before + 1);

        // registry knows the device
        let listed = DeviceRegistry::new(store.fs_handle()).list().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "Porch sign");
    }

    #[test]
    fn at_head_observation_records_without_banking() {
        let store = store();
        let summary = store
            .install_package(
                "Demo",
                &project_files("v1"),
                PackageProvenance::Created,
                1.0,
            )
            .unwrap();
        let head = store.open(summary.uid).unwrap().history.head().unwrap();

        record_device_observation(
            &store,
            &summary.uid.to_string(),
            &device(),
            head,
            &project_files("v1"),
            60.0,
        )
        .unwrap();
        let handle = store.open(summary.uid).unwrap();
        assert_eq!(handle.history.classify(head), SyncRelation::AtHead);
    }

    #[test]
    fn adoption_keeps_the_device_uid_and_pulled_provenance() {
        let store = store();
        let device = device();

        let mut files = vec![
            (
                "project.json".to_string(),
                br#"{"kind":"Project","uid":"prj_zzzzzzzzzzzzzzzz","name":"Wild One","nodes":{}}"#
                    .to_vec(),
            ),
            ("shader.glsl".to_string(), b"wild".to_vec()),
            (".lp/device.json".to_string(), b"{}".to_vec()),
        ];
        files.sort();

        let summary = adopt_device_package(&store, &device, &files, 42.0).unwrap();
        assert_eq!(
            summary.uid.to_string(),
            "prj_zzzzzzzzzzzzzzzz",
            "adoption keeps the on-device uid so future connects match"
        );
        assert_eq!(summary.slug, "2026-07-10-0900-wild-one");

        let handle = store.open(summary.uid).unwrap();
        assert!(matches!(
            handle.history.events().first().unwrap().kind,
            EventKind::PulledFromDevice { .. }
        ));
        assert!(handle.history.head().is_some(), "pulled content is v1");
        assert!(matches!(
            handle.history.events().last().unwrap().kind,
            EventKind::Connected { .. }
        ));
        // device identity file did not become project content
        assert!(
            !handle
                .read_all_files()
                .unwrap()
                .iter()
                .any(|(path, _)| path == ".lp/device.json")
        );

        let listed = DeviceRegistry::new(store.fs_handle()).list().unwrap();
        assert_eq!(listed.len(), 1);
    }

    #[test]
    fn registry_merge_preserves_association() {
        let store = store();
        let registry = DeviceRegistry::new(store.fs_handle());
        let project = PrefixedUid::mint(UidPrefix::Project, &[1u8; 16]);
        let mut seeded = device();
        seeded.association = Some(lpc_history::DeviceAssociation {
            device: seeded.uid.parse().unwrap(),
            project,
            version: ContentHash::of(b"v"),
            at: 5.0,
        });
        registry.upsert(seeded.clone()).unwrap();

        // a sight-only upsert must not erase what was last pushed
        let mut sighting = device();
        sighting.last_seen_at = 99.0;
        upsert_device_merged(&store, sighting).unwrap();

        let listed = registry.list().unwrap();
        assert_eq!(listed[0].last_seen_at, 99.0);
        assert_eq!(listed[0].association, seeded.association);
    }
}
