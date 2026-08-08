//! `LibraryStore`: package CRUD + history integration over the mounted store.

use std::cell::RefCell;
use std::rc::Rc;

use lpa_upgrade::FormatClass;
use lpc_history::{
    ContentHash, EventKind, EventLog, HistoryEvent, PrefixedUid, ProjectHistory, SnapshotStore,
    UidPrefix,
};
use lpc_model::{AsLpPath, LpPath};
use lpfs::{FsError, LpFs};

use super::package_format::{self, PackageHealth};
use super::package_manifest::{self, ManifestFields};
use super::package_meta::{self, PackageMeta, PackageProvenance};
use super::package_slug::{dated_slug, slugify, strip_date_prefix, unique_slug};
use super::{HISTORY_DIR, PACKAGES_DIR};

/// Library operation failure.
#[derive(Debug, Clone)]
pub enum LibraryError {
    Fs(String),
    Manifest(String),
    Meta(String),
    History(String),
    NotFound(String),
    /// An incoming package is at a format this build will not install:
    /// below the upgrade floor, from a newer LightPlayer, or unreadable.
    ///
    /// Carries [`lpa_upgrade::FormatClass::describe`] verbatim, which
    /// already names what was found, what was expected, and a remedy — so
    /// this one is printed bare rather than behind a category prefix. It
    /// exists so an import refusal reaches the user as a visible error
    /// instead of installing bytes that fail later, node by node.
    Format(String),
}

impl core::fmt::Display for LibraryError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            LibraryError::Fs(m) => write!(f, "library fs: {m}"),
            LibraryError::Manifest(m) => write!(f, "manifest: {m}"),
            LibraryError::Meta(m) => write!(f, "meta: {m}"),
            LibraryError::History(m) => write!(f, "history: {m}"),
            LibraryError::NotFound(m) => write!(f, "not found: {m}"),
            LibraryError::Format(m) => write!(f, "{m}"),
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
    /// What the format sniff found, and whether the package can be opened.
    /// Present on EVERY summary: a package that cannot be opened is listed
    /// with its problem, never dropped.
    pub health: PackageHealth,
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
    /// Build a handle from per-project fs handles: replay history from
    /// `history_fs`, initializing it from the package's provenance sidecar
    /// when the log is empty (first open of a fresh package).
    ///
    /// This is the store-free half of [`LibraryStore::open`]; the
    /// `LibraryHost` implementations use it over per-project mounts.
    pub fn load(
        uid: PrefixedUid,
        slug: String,
        package_fs: Rc<RefCell<dyn LpFs>>,
        history_fs: Rc<RefCell<dyn LpFs>>,
    ) -> Result<Self, LibraryError> {
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
/// Randomness (`random`, uid bytes) and the local wall-clock slug stamp
/// (`stamp`, `"YYYY-MM-DD-HHMM"`) are injected per the sans-IO discipline;
/// timestamps arrive as arguments.
#[derive(Clone)]
pub struct LibraryStore {
    fs: Rc<RefCell<dyn LpFs>>,
    random: Rc<dyn Fn() -> [u8; 16]>,
    stamp: Rc<dyn Fn() -> String>,
}

impl LibraryStore {
    pub fn new(
        fs: Rc<RefCell<dyn LpFs>>,
        random: Rc<dyn Fn() -> [u8; 16]>,
        stamp: Rc<dyn Fn() -> String>,
    ) -> Self {
        Self { fs, random, stamp }
    }

    /// A store for read paths (gallery snapshots, exports): rng and slug
    /// stamping are only reached by package-creating ops, so they panic —
    /// mutating a snapshot is a bug, not a fallback.
    pub fn read_only(fs: Rc<RefCell<dyn LpFs>>) -> Self {
        Self::new(
            fs,
            Rc::new(|| unreachable!("read-only store never mints uids")),
            Rc::new(|| unreachable!("read-only store never stamps slugs")),
        )
    }

    /// The store root — packages, history, and the device registry all live
    /// under it. Sibling layers (device registry, home gallery) build here.
    pub fn fs_handle(&self) -> Rc<RefCell<dyn LpFs>> {
        Rc::clone(&self.fs)
    }

    /// Every package directory, one summary each.
    ///
    /// A directory that exists ALWAYS produces a summary. It used to be
    /// skipped with a `log::warn!` when its manifest would not parse, which
    /// meant a too-old or damaged project simply disappeared from the
    /// gallery with no user-visible trace — and, because the strict parser
    /// runs before the format gate, "too old" and "corrupt" looked
    /// identical. Now the problem rides the summary
    /// ([`PackageHealth::Blocked`]) and the card says so.
    pub fn list(&self) -> Result<Vec<PackageSummary>, LibraryError> {
        let mut summaries: Vec<PackageSummary> = self
            .package_slugs()?
            .into_iter()
            .map(|slug| self.summarize(&slug))
            .collect();
        // slug order = date order for stamped slugs (newest naming sorts last)
        summaries.sort_by(|a, b| a.slug.cmp(&b.slug));
        Ok(summaries)
    }

    /// Create a package from files (the primitive behind create/seed/import).
    ///
    /// The package gets a date-based slug (`<stamp>-<label>`, uniqued —
    /// the user-facing identifier). Ensures a manifest exists (minimal one
    /// if `files` lacks it), applies `label` as the manifest name when it
    /// has none, mints the uid, writes the provenance sidecar, and
    /// initializes history (origin event + the initial save snapshot).
    pub fn install_package(
        &self,
        label: &str,
        files: &[(String, Vec<u8>)],
        provenance: PackageProvenance,
        now: f64,
    ) -> Result<PackageSummary, LibraryError> {
        let slug = dated_slug(&(self.stamp)(), label, &self.package_slugs()?);
        let package_fs = self.chroot_package(&slug)?;
        {
            let view = package_fs.borrow();
            for (relative, bytes) in files {
                let path = format!("/{}", relative.trim_start_matches('/'));
                view.write_file(path.as_str().as_path(), bytes)?;
            }
            if !view.file_exists(package_manifest::MANIFEST_PATH.as_path())? {
                // `format` is required by the loader's container format gate
                // (a missing manifest is a hard refuse); without it a
                // Created package would be unloadable.
                let manifest = lpc_model::ProjectManifest::new_current(label);
                view.write_file(
                    package_manifest::MANIFEST_PATH.as_path(),
                    manifest.write_json().as_bytes(),
                )?;
            }
            if files.is_empty() && !view.file_exists("/module.json".as_path())? {
                // The root module node is the other half of the mitosis
                // split; a blank Created package needs one to be loadable.
                // Only for blank creates: installs (device pulls, imports)
                // must stay byte-faithful to their source or parity hashes
                // would diverge on adoption.
                view.write_file("/module.json".as_path(), b"{\n  \"kind\": \"Module\"\n}\n")?;
            }
            let fields = package_manifest::read_manifest(&*view)?;
            if fields.name.is_none() {
                package_manifest::set_name(&*view, label)?;
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
    /// The copy's slug re-stamps the source's label (`2026-07-09-1500-basic`
    /// from `2026-07-08-1851-basic`); the new date is the differentiator.
    pub fn duplicate(&self, uid: PrefixedUid, now: f64) -> Result<PackageSummary, LibraryError> {
        let source = self.open(uid)?;
        let label = strip_date_prefix(&source.slug).to_string();
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
        let package = self.install_files_with_fresh_uid(&label, &files, provenance, now)?;
        Ok(package)
    }

    /// Rename = change the slug, MOVE the package directory, and patch the
    /// manifest `name` to the raw typed name. The uid (and therefore history
    /// and device associations) is untouched. Post-mitosis the manifest is
    /// library-owned workspace metadata (never an authored def slot), so the
    /// gallery rename is THE manifest patch path. Returns the final slug
    /// (slugified, collision-suffixed). Slug no-op still patches the name.
    pub fn rename(&self, uid: PrefixedUid, new_slug: &str) -> Result<String, LibraryError> {
        let old_slug = self
            .slug_for_uid(uid)?
            .ok_or_else(|| LibraryError::NotFound(uid.to_string()))?;
        let requested = slugify(new_slug);
        if requested == old_slug {
            let package_fs = self.chroot_package(&old_slug)?;
            package_manifest::set_name(&*package_fs.borrow(), new_slug)?;
            return Ok(old_slug);
        }
        let taken: Vec<String> = self
            .package_slugs()?
            .into_iter()
            .filter(|slug| slug != &old_slug)
            .collect();
        let final_slug = unique_slug(&requested, &taken);

        // move: copy every file into the new dir, then drop the old one
        let old_fs = self.chroot_package(&old_slug)?;
        let new_fs = self.chroot_package(&final_slug)?;
        {
            let old_view = old_fs.borrow();
            let new_view = new_fs.borrow();
            let entries = old_view.list_dir("/".as_path(), true)?;
            for entry in entries {
                if old_view.is_dir(entry.as_path()).unwrap_or(false) {
                    continue;
                }
                let bytes = old_view.read_file(entry.as_path())?;
                new_view.write_file(entry.as_path(), &bytes)?;
            }
        }
        self.fs
            .borrow()
            .delete_dir(format!("{PACKAGES_DIR}/{old_slug}").as_str().as_path())?;
        package_manifest::set_name(&*new_fs.borrow(), new_slug)?;
        Ok(final_slug)
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

    /// Resolve a card/URL key — a `prj…` uid or a slug — to the uid.
    pub fn resolve_key(&self, key: &str) -> Result<PrefixedUid, LibraryError> {
        // A key that parses strictly IS a uid (a slug can't collide: the
        // whole key would have to be `prj` + exactly 16 base-32 chars);
        // anything else is treated as a slug.
        if let Ok(uid) = key.parse::<PrefixedUid>() {
            return Ok(uid);
        }
        if !self.package_slugs()?.iter().any(|slug| slug == key) {
            return Err(LibraryError::NotFound(key.to_string()));
        }
        // Lenient, like the gallery: a package the strict parser rejects is
        // still addressable, so its card's remedies work.
        Ok(self.sniff(key).uid)
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
        PackageHandle::load(uid, slug, package_fs, history_fs)
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

    /// The strict summary: used by the paths that have just written a
    /// healthy package (install, seed lookup) and must hear about it loudly
    /// if it is not. [`Self::list`] uses [`Self::summarize`] instead.
    fn read_summary(&self, slug: &str) -> Result<PackageSummary, LibraryError> {
        let package_fs = self.chroot_package(slug)?;
        let view = package_fs.borrow();
        let ManifestFields { uid, name, .. } = package_manifest::read_manifest(&*view)?;
        let uid = uid
            .ok_or_else(|| LibraryError::Manifest(format!("package {slug} has no uid")))?
            .parse()
            .map_err(|e| LibraryError::Manifest(format!("package {slug} uid: {e}")))?;
        let health = package_format::health_for(&package_format::classify_package(&*view), None);
        Ok(PackageSummary {
            uid,
            name: name.unwrap_or_else(|| slug.to_string()),
            kind: String::from("Module"),
            slug: slug.to_string(),
            health,
        })
    }

    /// A summary for `slug` that cannot fail. Whatever is wrong with the
    /// package rides its [`PackageHealth`]; the gallery shows the card
    /// either way.
    fn summarize(&self, slug: &str) -> PackageSummary {
        let sniff = self.sniff(slug);
        PackageSummary {
            uid: sniff.uid,
            name: sniff.name.unwrap_or_else(|| slug.to_string()),
            kind: String::from("Module"),
            slug: slug.to_string(),
            health: package_format::health_for(&sniff.class, sniff.defect.as_deref()),
        }
    }

    /// Read `slug`'s manifest twice over: leniently for the facts a card
    /// needs, strictly for the verdict on whether it would load today.
    fn sniff(&self, slug: &str) -> PackageSniff {
        let package_fs = match self.chroot_package(slug) {
            Ok(package_fs) => package_fs,
            Err(error) => {
                return PackageSniff {
                    class: FormatClass::Unreadable {
                        detail: error.to_string(),
                    },
                    uid: derived_uid(slug),
                    name: None,
                    defect: Some(error.to_string()),
                };
            }
        };
        let view = package_fs.borrow();
        let class = package_format::classify_package(&*view);

        // The strict read is the "would this load today" question, and its
        // answer is advisory here — NEVER the reason a package disappears.
        let (strict_uid, strict_name, defect) = match package_manifest::read_manifest(&*view) {
            Ok(fields) => match fields.uid.as_deref().map(str::parse) {
                Some(Ok(uid)) => (Some(uid), fields.name, None),
                Some(Err(error)) => (
                    None,
                    fields.name,
                    Some(format!("project.json uid: {error}")),
                ),
                None => (
                    None,
                    fields.name,
                    Some(String::from("project.json states no uid")),
                ),
            },
            Err(error) => (None, None, Some(error.to_string())),
        };

        let lenient = strict_name.is_none().then(|| lenient_manifest(&*view));
        PackageSniff {
            class,
            // A package with no readable uid still needs a handle its card's
            // delete and export can address — that is the whole point of
            // keeping it on screen. Derived from the slug, so every lookup
            // agrees; never written back.
            uid: strict_uid
                .or_else(|| lenient.as_ref().and_then(|fields| fields.uid))
                .unwrap_or_else(|| derived_uid(slug)),
            name: strict_name.or_else(|| lenient.and_then(|fields| fields.name)),
            defect,
        }
    }

    /// The uid a package answers to, read the same lenient way the gallery
    /// read it. Strict-parse-rejected packages used to resolve to "not
    /// found" here, which made them unreachable by exactly the remedies
    /// (delete, export) they needed.
    fn slug_for_uid(&self, uid: PrefixedUid) -> Result<Option<String>, LibraryError> {
        Ok(self
            .package_slugs()?
            .into_iter()
            .find(|slug| self.sniff(slug).uid == uid))
    }

    fn chroot_package(&self, slug: &str) -> Result<Rc<RefCell<dyn LpFs>>, LibraryError> {
        let fs = self.fs.borrow();
        Ok(fs.chroot(format!("{PACKAGES_DIR}/{slug}").as_str().as_path())?)
    }
}

/// What one lenient look at a package directory found.
struct PackageSniff {
    /// The authored format, sniffed without any typed parse.
    class: FormatClass,
    /// The uid the package answers to (its own, or one derived from the
    /// slug when the manifest states none we can read).
    uid: PrefixedUid,
    name: Option<String>,
    /// What the strict manifest reader complained about, if it did.
    defect: Option<String>,
}

/// The two manifest fields a card needs, dug out of raw JSON.
struct LenientFields {
    uid: Option<PrefixedUid>,
    name: Option<String>,
}

/// Best-effort `uid`/`name` from a manifest the strict reader refused.
/// Deliberately `serde_json::Value`, not [`lpc_model::ProjectManifest`]:
/// the strict reader is what refused, so asking it again would only fail
/// the same way.
fn lenient_manifest(fs: &dyn LpFs) -> LenientFields {
    let Ok(bytes) = fs.read_file(package_manifest::MANIFEST_PATH.as_path()) else {
        return LenientFields {
            uid: None,
            name: None,
        };
    };
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return LenientFields {
            uid: None,
            name: None,
        };
    };
    LenientFields {
        uid: value
            .get("uid")
            .and_then(serde_json::Value::as_str)
            .and_then(|uid| uid.parse().ok()),
        name: value
            .get("name")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
    }
}

/// A stand-in identity for a package whose manifest states no uid we can
/// read.
///
/// Derived from the slug, so the gallery card, `resolve_key`, `open` and
/// `delete` all name the same package — and never written to disk: looking
/// at a broken project must not author anything into it. The FNV-1a spread
/// is arbitrary but stable; collisions with a minted uid are not a practical
/// concern (a minted uid comes from 128 bits of randomness).
fn derived_uid(slug: &str) -> PrefixedUid {
    let mut bytes = [0u8; 16];
    bytes[..8].copy_from_slice(&fnv1a64(slug.as_bytes()).to_le_bytes());
    bytes[8..]
        .copy_from_slice(&fnv1a64(&[slug.as_bytes(), b"\x00lp-package"].concat()).to_le_bytes());
    PrefixedUid::mint(UidPrefix::Project, &bytes)
}

fn fnv1a64(input: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in input {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
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
        PackageProvenance::ImportedJson { .. } => EventKind::ImportedJson,
        PackageProvenance::PulledFromDevice { device_uid, .. } => match device_uid.parse() {
            Ok(device) => EventKind::PulledFromDevice { device },
            Err(_) => {
                log::warn!("unparseable device provenance; falling back to Created origin");
                EventKind::Created
            }
        },
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
            Rc::new(|| "2026-07-09-1421".to_string()),
        )
    }

    fn demo_files() -> Vec<(String, Vec<u8>)> {
        vec![
            (
                "project.json".to_string(),
                br#"{"format":8,"name":"demo"}"#.to_vec(),
            ),
            (
                "module.json".to_string(),
                br#"{"kind":"Module","nodes":{"clock":{"ref":"./clock.json"}}}"#.to_vec(),
            ),
            ("clock.json".to_string(), br#"{"kind":"Clock"}"#.to_vec()),
            ("shader.glsl".to_string(), b"void main() {}".to_vec()),
        ]
    }

    #[test]
    fn create_mints_uid_sidecar_slug_and_history() {
        let store = store();
        let summary = store.create("My Project!", 1.0).unwrap();
        assert_eq!(summary.slug, "2026-07-09-1421-my-project");
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

        let copy = store.duplicate(original.uid, 2.0).unwrap();
        assert_ne!(copy.uid, original.uid);
        // re-stamped label, uniqued against the same-stamp original
        assert_eq!(copy.slug, "2026-07-09-1421-demo-2");

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
        assert_eq!(source_files.len(), 5); // 4 demo files + sidecar
    }

    /// P1: `install_files_with_fresh_uid` (the primitive behind `duplicate`
    /// and import) rewrites the manifest via a generic `serde_json::Value`
    /// map, not the canonical writer — it should pass `kind`/`exports`
    /// through untouched, the same as any other key it does not itself
    /// know about.
    #[test]
    fn install_files_with_fresh_uid_preserves_kind_and_exports() {
        let store = store();
        let files = vec![
            (
                "project.json".to_string(),
                br#"{"format":5,"name":"demo","kind":"pattern","exports":["chase"]}"#.to_vec(),
            ),
            (
                "module.json".to_string(),
                br#"{"kind":"Module","nodes":{}}"#.to_vec(),
            ),
        ];
        let summary = store
            .install_files_with_fresh_uid("fresh", &files, PackageProvenance::Created, 1.0)
            .unwrap();
        let handle = store.open(summary.uid).unwrap();
        let fields = package_manifest::read_manifest(&*handle.package_fs.borrow()).unwrap();
        assert_eq!(
            fields.kind,
            lpc_model::ProjectKind::Pattern {
                exports: vec!["chase".to_string()]
            }
        );
        assert_eq!(fields.exports, vec!["chase".to_string()]);
    }

    #[test]
    fn rename_moves_the_directory_and_keeps_identity() {
        let store = store();
        let summary = store
            .install_package("demo", &demo_files(), PackageProvenance::Created, 1.0)
            .unwrap();
        let old_slug = summary.slug.clone();
        let head_before = store.open(summary.uid).unwrap().history.head();

        let final_slug = store.rename(summary.uid, "Porch Sign!").unwrap();
        assert_eq!(final_slug, "porch-sign"); // verbatim slugified, no auto date

        let listed = store.list().unwrap();
        assert_eq!(listed.len(), 1, "the old directory is gone");
        assert_eq!(listed[0].slug, "porch-sign");
        assert_eq!(listed[0].uid, summary.uid, "identity survives the move");

        let handle = store.open(summary.uid).unwrap();
        assert_eq!(handle.slug, "porch-sign");
        assert_eq!(handle.history.head(), head_before, "history untouched");
        assert!(
            handle
                .read_all_files()
                .unwrap()
                .iter()
                .any(|(path, _)| path == "shader.glsl"),
            "files moved"
        );
        assert_ne!(old_slug, final_slug);

        // no-op rename returns the same slug without churn
        assert_eq!(
            store.rename(summary.uid, "porch sign").unwrap(),
            "porch-sign"
        );

        // resolve by either key
        assert_eq!(store.resolve_key("porch-sign").unwrap(), summary.uid);
        assert_eq!(
            store.resolve_key(&summary.uid.to_string()).unwrap(),
            summary.uid
        );
        assert!(store.resolve_key(&old_slug).is_err());
    }

    #[test]
    fn created_package_loads_through_project_registry() {
        // Would have failed before the minimal manifest carried `format`:
        // the loader's root format gate rejects `found: None`.
        let store = store();
        let summary = store.create("Fresh", 1.0).unwrap();
        let handle = store.open(summary.uid).unwrap();

        let shapes = lpc_model::SlotShapeRegistry::default();
        let ctx = lpc_registry::ParseCtx { shapes: &shapes };
        let mut registry = lpc_registry::ProjectRegistry::new();
        let package_fs = handle.package_fs.borrow();
        registry
            .load_root(
                &*package_fs,
                LpPath::new("/project.json"),
                lpc_model::Revision::new(1),
                &ctx,
            )
            .expect("a freshly created package must load through the registry");
    }

    /// Write a package directory straight into the store's fs, bypassing
    /// `install_package` — which is the only way to get the shapes this
    /// suite is about (a hand-copied v4 export, a pre-mitosis leftover, a
    /// torn manifest) into the library at all.
    fn plant_package(fs: &Rc<RefCell<dyn LpFs>>, slug: &str, manifest: &[u8]) {
        let view = fs.borrow();
        view.write_file(
            format!("{PACKAGES_DIR}/{slug}/project.json")
                .as_str()
                .as_path(),
            manifest,
        )
        .unwrap();
        view.write_file(
            format!("{PACKAGES_DIR}/{slug}/module.json")
                .as_str()
                .as_path(),
            b"{\n  \"kind\": \"Module\"\n}\n",
        )
        .unwrap();
    }

    fn store_over(fs: Rc<RefCell<dyn LpFs>>) -> LibraryStore {
        LibraryStore::new(
            fs,
            Rc::new(|| [42u8; 16]),
            Rc::new(|| "2026-07-09-1421".to_string()),
        )
    }

    #[test]
    fn no_package_directory_ever_vanishes_from_the_gallery() {
        // The bug this is here to prevent: a package whose manifest the
        // strict reader refuses used to be logged and dropped, so a project
        // the user could see yesterday was simply gone.
        let fs: Rc<RefCell<dyn LpFs>> = Rc::new(RefCell::new(LpFsMemory::new()));
        let store = store_over(fs.clone());
        let healthy = store.create("Healthy", 1.0).unwrap();

        let uid = PrefixedUid::mint(UidPrefix::Project, &[9u8; 16]);
        plant_package(
            &fs,
            "a-format-4",
            format!(r#"{{"format":4,"uid":"{uid}","name":"Four"}}"#).as_bytes(),
        );
        plant_package(&fs, "b-format-3", br#"{"format":3,"name":"Three"}"#);
        plant_package(&fs, "c-format-99", br#"{"format":99,"name":"Future"}"#);
        plant_package(&fs, "d-garbage", b"{ not json at all");
        plant_package(
            &fs,
            "e-pre-mitosis",
            br#"{"kind":"Project","format":1,"name":"Ancient","nodes":{}}"#,
        );
        fs.borrow()
            .write_file(
                format!("{PACKAGES_DIR}/f-no-manifest/notes.txt")
                    .as_str()
                    .as_path(),
                b"nothing to see",
            )
            .unwrap();

        let listed = store.list().unwrap();
        let slugs: Vec<&str> = listed.iter().map(|s| s.slug.as_str()).collect();
        assert_eq!(
            slugs,
            vec![
                "2026-07-09-1421-healthy",
                "a-format-4",
                "b-format-3",
                "c-format-99",
                "d-garbage",
                "e-pre-mitosis",
                "f-no-manifest",
            ],
            "every directory that exists is listed"
        );

        let health = |slug: &str| {
            listed
                .iter()
                .find(|summary| summary.slug == slug)
                .map(|summary| summary.health.clone())
                .unwrap()
        };
        assert_eq!(health("2026-07-09-1421-healthy"), PackageHealth::Ready);
        assert_eq!(
            health("a-format-4"),
            PackageHealth::UpgradesOnOpen { found: 4 },
            "the floor migrates on open, so its card stays a normal card"
        );
        for (slug, expected) in [
            ("b-format-3", "Format 3 — too old for this Studio"),
            ("c-format-99", "Format 99 — made by a newer LightPlayer"),
            ("d-garbage", "project.json could not be read"),
            ("e-pre-mitosis", "Format 1 — too old for this Studio"),
            ("f-no-manifest", "No project.json — not a project"),
        ] {
            let health = health(slug);
            let (headline, remedy) = health
                .blocked()
                .unwrap_or_else(|| panic!("{slug}: {health:?}"));
            assert_eq!(headline, expected);
            assert!(remedy.ends_with('.'), "{slug}: {remedy}");
        }

        // the healthy package is untouched by all of this
        assert!(listed.iter().any(|summary| summary.uid == healthy.uid));
    }

    #[test]
    fn a_package_the_strict_parser_rejects_stays_reachable_by_its_remedies() {
        // Swallow point 3: `slug_for_uid` used to skip anything the strict
        // manifest reader refused, so the packages that most needed
        // deleting or exporting answered "not found".
        let fs: Rc<RefCell<dyn LpFs>> = Rc::new(RefCell::new(LpFsMemory::new()));
        let store = store_over(fs.clone());
        plant_package(
            &fs,
            "e-pre-mitosis",
            br#"{"kind":"Project","format":1,"name":"Ancient","nodes":{}}"#,
        );

        let summary = store.list().unwrap().into_iter().next().unwrap();
        // resolvable by slug AND by the uid the card carries
        assert_eq!(store.resolve_key("e-pre-mitosis").unwrap(), summary.uid);
        assert_eq!(
            store.resolve_key(&summary.uid.to_string()).unwrap(),
            summary.uid
        );

        // export reads raw files through an open handle — it must work
        let handle = store.open(summary.uid).unwrap();
        assert!(
            handle
                .read_all_files()
                .unwrap()
                .iter()
                .any(|(path, _)| path == "project.json")
        );

        store.delete(summary.uid).unwrap();
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn a_derived_identity_is_stable_and_slug_scoped() {
        // The stand-in uid must be the same every time it is computed, or
        // the card, `resolve_key` and `delete` would name different things.
        assert_eq!(derived_uid("porch-sign"), derived_uid("porch-sign"));
        assert_ne!(derived_uid("porch-sign"), derived_uid("porch-sign-2"));
        assert!(derived_uid("porch-sign").to_string().starts_with("prj"));
    }

    #[test]
    fn a_manifest_uid_still_wins_over_the_derived_one() {
        let fs: Rc<RefCell<dyn LpFs>> = Rc::new(RefCell::new(LpFsMemory::new()));
        let store = store_over(fs.clone());
        let uid = PrefixedUid::mint(UidPrefix::Project, &[9u8; 16]);
        // pre-mitosis root: the strict reader refuses it, the lenient read
        // still finds the identity the package states
        plant_package(
            &fs,
            "old-but-identified",
            format!(r#"{{"kind":"Project","format":2,"uid":"{uid}","nodes":{{}}}}"#).as_bytes(),
        );
        let summary = store.list().unwrap().into_iter().next().unwrap();
        assert_eq!(summary.uid, uid);
        assert_ne!(summary.uid, derived_uid("old-but-identified"));
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
        let store = LibraryStore::new(
            fs.clone(),
            Rc::new(|| [3u8; 16]),
            Rc::new(|| "2026-07-09-1421".to_string()),
        );
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

        let store2 = LibraryStore::new(
            fs,
            Rc::new(|| [4u8; 16]),
            Rc::new(|| "2026-07-09-1421".to_string()),
        );
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
