//! `LibraryStore`: package CRUD + history integration over the mounted store.

use std::cell::RefCell;
use std::rc::Rc;

use lpc_history::{
    ContentHash, EventKind, EventLog, HistoryEvent, PrefixedUid, ProjectHistory, SnapshotStore,
};
use lpc_model::{AsLpPath, LpPath};
use lpfs::{FsError, LpFs};

use super::package_manifest::{self, ManifestFields};
use super::package_meta::{self, PackageMeta, PackageProvenance};
use super::package_slug::unique_slug;
use super::{HISTORY_DIR, PACKAGES_DIR};

/// Library operation failure.
#[derive(Debug, Clone)]
pub enum LibraryError {
    Fs(String),
    Manifest(String),
    Meta(String),
    History(String),
    NotFound(String),
}

impl core::fmt::Display for LibraryError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            LibraryError::Fs(m) => write!(f, "library fs: {m}"),
            LibraryError::Manifest(m) => write!(f, "manifest: {m}"),
            LibraryError::Meta(m) => write!(f, "meta: {m}"),
            LibraryError::History(m) => write!(f, "history: {m}"),
            LibraryError::NotFound(m) => write!(f, "not found: {m}"),
        }
    }
}

impl From<FsError> for LibraryError {
    fn from(e: FsError) -> Self {
        LibraryError::Fs(e.to_string())
    }
}

/// One library package, as the gallery will list it.
#[derive(Debug, Clone, PartialEq)]
pub struct PackageSummary {
    pub uid: PrefixedUid,
    pub name: String,
    pub kind: String,
    pub slug: String,
}

/// An opened package: chrooted views + replayed history.
pub struct PackageHandle {
    pub uid: PrefixedUid,
    pub slug: String,
    pub package_fs: Rc<RefCell<dyn LpFs>>,
    pub history_fs: Rc<RefCell<dyn LpFs>>,
    pub history: ProjectHistory,
}

impl PackageHandle {
    /// Snapshot the package and record a `Saved` event — unless the content
    /// hash equals the current head (no-op guard: no event spam).
    pub fn record_save(&mut self, at: f64) -> Result<Option<ContentHash>, LibraryError> {
        let hash = {
            let history_fs = self.history_fs.borrow();
            let snapshots = SnapshotStore::new(&*history_fs);
            let package_fs = self.package_fs.borrow();
            let (hash, _) = snapshots
                .put_package(&*package_fs)
                .map_err(|e| LibraryError::History(e.to_string()))?;
            hash
        };
        if self.history.head() == Some(hash) {
            return Ok(None);
        }
        let event = self.history.record_save(hash, at);
        let history_fs = self.history_fs.borrow();
        EventLog::new(&*history_fs)
            .append(&event)
            .map_err(|e| LibraryError::History(e.to_string()))?;
        Ok(Some(hash))
    }

    /// Apply one pulled file update: `Some(bytes)` upserts, `None` deletes
    /// (a tombstone for a file the library never had is tolerated).
    pub fn apply_update(&self, path: &LpPath, content: Option<&[u8]>) -> Result<(), LibraryError> {
        let package_fs = self.package_fs.borrow();
        match content {
            Some(bytes) => package_fs.write_file(path, bytes)?,
            None => match package_fs.delete_file(path) {
                Ok(()) | Err(FsError::NotFound(_)) => {}
                Err(e) => return Err(e.into()),
            },
        }
        Ok(())
    }

    /// All package files as (relative path, bytes) — the push payload.
    pub fn read_all_files(&self) -> Result<Vec<(String, Vec<u8>)>, LibraryError> {
        let package_fs = self.package_fs.borrow();
        let mut files = Vec::new();
        let entries = match package_fs.list_dir("/".as_path(), true) {
            Ok(entries) => entries,
            Err(FsError::NotFound(_)) => Vec::new(),
            Err(e) => return Err(e.into()),
        };
        for entry in entries {
            if package_fs.is_dir(entry.as_path()).unwrap_or(false) {
                continue;
            }
            let bytes = package_fs.read_file(entry.as_path())?;
            files.push((entry.as_str().trim_start_matches('/').to_string(), bytes));
        }
        files.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(files)
    }

    /// Locally computed canonical package hash (push/pull verification).
    pub fn content_hash(&self) -> Result<ContentHash, LibraryError> {
        let package_fs = self.package_fs.borrow();
        lpc_history::hash_package(&*package_fs)
            .map(|(hash, _)| hash)
            .map_err(|e| LibraryError::History(e.to_string()))
    }
}

/// The library: package CRUD over a caller-supplied store.
///
/// Randomness is injected (`random` supplies uid bytes) per the sans-IO
/// discipline; timestamps arrive as arguments.
#[derive(Clone)]
pub struct LibraryStore {
    fs: Rc<RefCell<dyn LpFs>>,
    random: Rc<dyn Fn() -> [u8; 16]>,
}

impl LibraryStore {
    pub fn new(fs: Rc<RefCell<dyn LpFs>>, random: Rc<dyn Fn() -> [u8; 16]>) -> Self {
        Self { fs, random }
    }

    pub fn list(&self) -> Result<Vec<PackageSummary>, LibraryError> {
        let mut summaries = Vec::new();
        for slug in self.package_slugs()? {
            match self.read_summary(&slug) {
                Ok(summary) => summaries.push(summary),
                Err(e) => log::warn!("skipping package dir {slug}: {e}"),
            }
        }
        summaries.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(summaries)
    }

    /// Create a package from files (the primitive behind create/seed/import).
    ///
    /// Ensures a manifest exists (minimal one if `files` lacks it), applies
    /// `fallback_name` when the manifest has no name, mints the uid, writes
    /// the provenance sidecar, and initializes history (origin event + the
    /// initial save snapshot).
    pub fn install_package(
        &self,
        fallback_name: &str,
        files: &[(String, Vec<u8>)],
        provenance: PackageProvenance,
        now: f64,
    ) -> Result<PackageSummary, LibraryError> {
        let slug = unique_slug(fallback_name, &self.package_slugs()?);
        let package_fs = self.chroot_package(&slug)?;
        {
            let view = package_fs.borrow();
            for (relative, bytes) in files {
                let path = format!("/{}", relative.trim_start_matches('/'));
                view.write_file(path.as_str().as_path(), bytes)?;
            }
            if !view.file_exists(package_manifest::MANIFEST_PATH.as_path())? {
                let minimal = serde_json::json!({ "kind": "Project", "name": fallback_name });
                view.write_file(
                    package_manifest::MANIFEST_PATH.as_path(),
                    serde_json::to_vec_pretty(&minimal)
                        .map_err(|e| LibraryError::Manifest(e.to_string()))?
                        .as_slice(),
                )?;
            }
            let fields = package_manifest::read_manifest(&*view)?;
            if fields.name.is_none() {
                package_manifest::set_name(&*view, fallback_name)?;
            }
            package_manifest::ensure_uid(&*view, &(self.random)())?;
            package_meta::write_meta(
                &*view,
                &PackageMeta {
                    provenance: provenance.clone(),
                    created_at: now,
                },
            )?;
        }

        let summary = self.read_summary(&slug)?;
        // initialize history: origin from provenance, then the initial save
        let mut handle = self.open(summary.uid)?;
        handle.record_save(now)?;
        Ok(summary)
    }

    /// Create an empty project with a minimal manifest.
    pub fn create(&self, name: &str, now: f64) -> Result<PackageSummary, LibraryError> {
        self.install_package(name, &[], PackageProvenance::Created, now)
    }

    /// Duplicate = fork at head: independent copy with fork provenance.
    pub fn duplicate(
        &self,
        uid: PrefixedUid,
        new_name: &str,
        now: f64,
    ) -> Result<PackageSummary, LibraryError> {
        let source = self.open(uid)?;
        let head = source.history.head();
        let files: Vec<(String, Vec<u8>)> = source
            .read_all_files()?
            .into_iter()
            .filter(|(path, _)| path != ".lp/meta.json")
            .collect();
        let provenance = match head {
            Some(version) => PackageProvenance::ForkedFrom {
                parent_project: uid.to_string(),
                parent_version: version.to_string(),
            },
            None => PackageProvenance::Created,
        };
        // the copy must mint its own uid: drop the manifest's before install
        let package = self.install_files_with_fresh_uid(new_name, &files, provenance, now)?;
        Ok(package)
    }

    pub fn rename(&self, uid: PrefixedUid, name: &str) -> Result<(), LibraryError> {
        let handle = self.open(uid)?;
        let view = handle.package_fs.borrow();
        package_manifest::set_name(&*view, name)
    }

    pub fn delete(&self, uid: PrefixedUid) -> Result<(), LibraryError> {
        let slug = self
            .slug_for_uid(uid)?
            .ok_or_else(|| LibraryError::NotFound(uid.to_string()))?;
        let fs = self.fs.borrow();
        fs.delete_dir(format!("{PACKAGES_DIR}/{slug}").as_str().as_path())?;
        match fs.delete_dir(format!("{HISTORY_DIR}/{uid}").as_str().as_path()) {
            Ok(()) | Err(FsError::NotFound(_)) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn open(&self, uid: PrefixedUid) -> Result<PackageHandle, LibraryError> {
        let slug = self
            .slug_for_uid(uid)?
            .ok_or_else(|| LibraryError::NotFound(uid.to_string()))?;
        let package_fs = self.chroot_package(&slug)?;
        let history_fs = {
            let fs = self.fs.borrow();
            fs.chroot(format!("{HISTORY_DIR}/{uid}").as_str().as_path())?
        };
        let history = {
            let view = history_fs.borrow();
            let log = EventLog::new(&*view);
            let events = log
                .read_all()
                .map_err(|e| LibraryError::History(e.to_string()))?;
            if events.is_empty() {
                let meta = {
                    let package_view = package_fs.borrow();
                    package_meta::read_meta(&*package_view)?
                };
                let origin = origin_event_for(meta);
                log.append(&origin)
                    .map_err(|e| LibraryError::History(e.to_string()))?;
                ProjectHistory::new(origin).map_err(|e| LibraryError::History(e.to_string()))?
            } else {
                ProjectHistory::from_events(events)
                    .map_err(|e| LibraryError::History(e.to_string()))?
            }
        };
        Ok(PackageHandle {
            uid,
            slug,
            package_fs,
            history_fs,
            history,
        })
    }

    /// Find a package by its provenance source (seed-once checks).
    pub fn find_seeded_from(&self, source: &str) -> Result<Option<PackageSummary>, LibraryError> {
        for slug in self.package_slugs()? {
            let package_fs = self.chroot_package(&slug)?;
            let meta = {
                let view = package_fs.borrow();
                package_meta::read_meta(&*view)?
            };
            if let Some(PackageMeta {
                provenance: PackageProvenance::SeededFrom { source: s },
                ..
            }) = meta
            {
                if s == source {
                    return Ok(Some(self.read_summary(&slug)?));
                }
            }
        }
        Ok(None)
    }

    pub(crate) fn install_files_with_fresh_uid(
        &self,
        name: &str,
        files: &[(String, Vec<u8>)],
        provenance: PackageProvenance,
        now: f64,
    ) -> Result<PackageSummary, LibraryError> {
        let mut files: Vec<(String, Vec<u8>)> = files.to_vec();
        if let Some((_, manifest_bytes)) = files.iter_mut().find(|(path, _)| path == "project.json")
        {
            let mut value: serde_json::Value = serde_json::from_slice(manifest_bytes)
                .map_err(|e| LibraryError::Manifest(e.to_string()))?;
            if let serde_json::Value::Object(map) = &mut value {
                map.remove("uid");
                map.insert(
                    "name".to_string(),
                    serde_json::Value::String(name.to_string()),
                );
            }
            *manifest_bytes = serde_json::to_vec_pretty(&value)
                .map_err(|e| LibraryError::Manifest(e.to_string()))?;
        }
        self.install_package(name, &files, provenance, now)
    }

    fn package_slugs(&self) -> Result<Vec<String>, LibraryError> {
        let fs = self.fs.borrow();
        let entries = match fs.list_dir(PACKAGES_DIR.as_path(), false) {
            Ok(entries) => entries,
            Err(FsError::NotFound(_)) => Vec::new(),
            Err(e) => return Err(e.into()),
        };
        let mut slugs = Vec::new();
        for entry in entries {
            if fs.is_dir(entry.as_path()).unwrap_or(false) {
                if let Some(slug) = entry.as_str().rsplit('/').next() {
                    if !slug.is_empty() {
                        slugs.push(slug.to_string());
                    }
                }
            }
        }
        slugs.sort();
        Ok(slugs)
    }

    fn read_summary(&self, slug: &str) -> Result<PackageSummary, LibraryError> {
        let package_fs = self.chroot_package(slug)?;
        let view = package_fs.borrow();
        let ManifestFields { uid, name, kind } = package_manifest::read_manifest(&*view)?;
        let uid = uid
            .ok_or_else(|| LibraryError::Manifest(format!("package {slug} has no uid")))?
            .parse()
            .map_err(|e| LibraryError::Manifest(format!("package {slug} uid: {e}")))?;
        Ok(PackageSummary {
            uid,
            name: name.unwrap_or_else(|| slug.to_string()),
            kind,
            slug: slug.to_string(),
        })
    }

    fn slug_for_uid(&self, uid: PrefixedUid) -> Result<Option<String>, LibraryError> {
        for slug in self.package_slugs()? {
            let package_fs = self.chroot_package(&slug)?;
            let view = package_fs.borrow();
            if let Ok(fields) = package_manifest::read_manifest(&*view) {
                if fields.uid.as_deref() == Some(uid.to_string().as_str()) {
                    return Ok(Some(slug));
                }
            }
        }
        Ok(None)
    }

    fn chroot_package(&self, slug: &str) -> Result<Rc<RefCell<dyn LpFs>>, LibraryError> {
        let fs = self.fs.borrow();
        Ok(fs.chroot(format!("{PACKAGES_DIR}/{slug}").as_str().as_path())?)
    }
}

fn origin_event_for(meta: Option<PackageMeta>) -> HistoryEvent {
    let (at, provenance) = meta.map_or((0.0, PackageProvenance::Created), |m| {
        (m.created_at, m.provenance)
    });
    let kind = match provenance {
        PackageProvenance::Created => EventKind::Created,
        PackageProvenance::SeededFrom { source } => EventKind::RemixedFrom {
            source,
            source_version: None,
        },
        PackageProvenance::ImportedZip { .. } => EventKind::ImportedZip,
        PackageProvenance::ForkedFrom {
            parent_project,
            parent_version,
        } => match (parent_project.parse(), parent_version.parse()) {
            (Ok(parent_project), Ok(parent_version)) => EventKind::ForkedFrom {
                parent_project,
                parent_version,
            },
            _ => {
                log::warn!("unparseable fork provenance; falling back to Created origin");
                EventKind::Created
            }
        },
    };
    HistoryEvent { at, kind }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpfs::LpFsMemory;

    fn store() -> LibraryStore {
        let counter = Rc::new(RefCell::new(0u8));
        LibraryStore::new(
            Rc::new(RefCell::new(LpFsMemory::new())),
            Rc::new(move || {
                *counter.borrow_mut() += 1;
                [*counter.borrow(); 16]
            }),
        )
    }

    fn demo_files() -> Vec<(String, Vec<u8>)> {
        vec![
            (
                "project.json".to_string(),
                br#"{"kind":"Project","name":"demo","nodes":{"clock":{"ref":"./clock.json"}}}"#
                    .to_vec(),
            ),
            ("clock.json".to_string(), br#"{"kind":"Clock"}"#.to_vec()),
            ("shader.glsl".to_string(), b"void main() {}".to_vec()),
        ]
    }

    #[test]
    fn create_mints_uid_sidecar_slug_and_history() {
        let store = store();
        let summary = store.create("My Project!", 1.0).unwrap();
        assert_eq!(summary.slug, "my-project");
        assert_eq!(summary.name, "My Project!");

        let handle = store.open(summary.uid).unwrap();
        assert!(handle.history.head().is_some(), "initial save recorded");
        let meta = package_meta::read_meta(&*handle.package_fs.borrow())
            .unwrap()
            .unwrap();
        assert_eq!(meta.provenance, PackageProvenance::Created);
    }

    #[test]
    fn install_keeps_manifest_name_and_mints_uid() {
        let store = store();
        let summary = store
            .install_package(
                "fallback",
                &demo_files(),
                PackageProvenance::SeededFrom {
                    source: "examples/basic".to_string(),
                },
                2.0,
            )
            .unwrap();
        assert_eq!(summary.name, "demo");
        assert!(store.find_seeded_from("examples/basic").unwrap().is_some());
        assert!(store.find_seeded_from("examples/other").unwrap().is_none());
    }

    #[test]
    fn duplicate_forks_at_head_with_fresh_uid() {
        let store = store();
        let original = store
            .install_package("demo", &demo_files(), PackageProvenance::Created, 1.0)
            .unwrap();
        let original_head = store.open(original.uid).unwrap().history.head().unwrap();

        let copy = store.duplicate(original.uid, "demo copy", 2.0).unwrap();
        assert_ne!(copy.uid, original.uid);
        assert_eq!(copy.slug, "demo-copy");

        let copy_handle = store.open(copy.uid).unwrap();
        // fork origin seeds the line with the parent head (v1); the copy's
        // own first save (with its new uid in the manifest) becomes v2 —
        // identity is part of content, so the heads honestly differ
        assert_eq!(copy_handle.history.version_number(original_head), Some(1));
        assert!(copy_handle.history.contains(original_head));
        let copy_head = copy_handle.history.head().unwrap();
        assert_ne!(copy_head, original_head);
        assert_eq!(copy_handle.history.version_number(copy_head), Some(2));
        // source untouched
        let source_files = store.open(original.uid).unwrap().read_all_files().unwrap();
        assert_eq!(source_files.len(), 4); // 3 demo files + sidecar
    }

    #[test]
    fn rename_patches_name_only() {
        let store = store();
        let summary = store
            .install_package("demo", &demo_files(), PackageProvenance::Created, 1.0)
            .unwrap();
        store.rename(summary.uid, "renamed").unwrap();
        let listed = store.list().unwrap();
        assert_eq!(listed[0].name, "renamed");
        assert_eq!(listed[0].uid, summary.uid);
        assert_eq!(listed[0].slug, "demo"); // dir unchanged
    }

    #[test]
    fn delete_removes_package_and_history() {
        let store = store();
        let summary = store.create("gone", 1.0).unwrap();
        store.delete(summary.uid).unwrap();
        assert!(store.list().unwrap().is_empty());
        assert!(store.open(summary.uid).is_err());
    }

    #[test]
    fn open_round_trips_history_across_store_instances() {
        let fs: Rc<RefCell<dyn LpFs>> = Rc::new(RefCell::new(LpFsMemory::new()));
        let store = LibraryStore::new(fs.clone(), Rc::new(|| [3u8; 16]));
        let summary = store
            .install_package("demo", &demo_files(), PackageProvenance::Created, 1.0)
            .unwrap();
        let mut handle = store.open(summary.uid).unwrap();
        handle
            .apply_update("/shader.glsl".as_path(), Some(b"void main() { /*2*/ }"))
            .unwrap();
        let saved = handle.record_save(2.0).unwrap();
        assert!(saved.is_some());
        // unchanged content: no-op
        assert!(handle.record_save(3.0).unwrap().is_none());

        let store2 = LibraryStore::new(fs, Rc::new(|| [4u8; 16]));
        let handle2 = store2.open(summary.uid).unwrap();
        assert_eq!(handle2.history.head(), handle.history.head());
        assert_eq!(
            handle2.history.events().len(),
            handle.history.events().len()
        );
    }

    #[test]
    fn record_save_restores_via_snapshot() {
        let store = store();
        let summary = store
            .install_package("demo", &demo_files(), PackageProvenance::Created, 1.0)
            .unwrap();
        let mut handle = store.open(summary.uid).unwrap();
        let v1 = handle.history.head().unwrap();
        handle
            .apply_update("/shader.glsl".as_path(), Some(b"v2"))
            .unwrap();
        handle.record_save(2.0).unwrap();

        // materialize v1 back out of the snapshot store
        let history_fs = handle.history_fs.borrow();
        let snapshots = SnapshotStore::new(&*history_fs);
        let restored = lpfs::LpFsMemory::new();
        snapshots.materialize(&v1, &restored).unwrap();
        assert_eq!(
            restored.read_file("/shader.glsl".as_path()).unwrap(),
            b"void main() {}"
        );
    }
}
