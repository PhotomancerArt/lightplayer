//! Zip import/export for packages (library-level codec; UI lands in M4).
//!
//! Export: every package file including `/.lp/meta.json` (provenance
//! travels), never `/history/**`, under a single top-level directory named
//! by the slug (the friendly unzip experience), deflated, deterministic
//! entry order.
//!
//! Import: mints a **new uid** (zips get shared; colliding uids would break
//! identity) with `ImportedZip` provenance recording the archive's own uid
//! when it had one. Tolerates Finder noise (`__MACOSX/`, `.DS_Store`) and a
//! nested top-level directory.
//!
//! Import also **gates on format before it installs anything**. An archive
//! or envelope arrives from wherever it was made — an old build, a
//! colleague, a chat window — so it is classified ([`lpa_upgrade::classify`])
//! and, where this build can, migrated in memory first. Installing an
//! unclassified stale archive is how a project used to land looking healthy
//! and then fail node by node when it was opened, which reads as a bug in
//! the nodes rather than a project that is simply too old.
//!
//! The migration happens to the file vec, never inside the installer:
//! `install_package`/`install_files_with_fresh_uid` stay byte-faithful by
//! design (device adoption compares hashes against those bytes).

use std::io::{Cursor, Read, Write};

use lpa_upgrade::{FormatClass, ProjectFiles, classify, upgrade_to_current};
use zip::write::SimpleFileOptions;

use super::library_store::{LibraryError, LibraryStore, PackageHandle, PackageSummary};
use super::package_meta::PackageProvenance;

/// Serialize a package to zip bytes.
pub fn export_package(handle: &PackageHandle) -> Result<Vec<u8>, LibraryError> {
    let files = handle.read_all_files()?; // sorted relative paths
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut writer = zip::ZipWriter::new(&mut cursor);
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (relative, bytes) in &files {
            writer
                .start_file(format!("{}/{relative}", handle.slug), options)
                .map_err(|e| LibraryError::Meta(format!("zip: {e}")))?;
            writer
                .write_all(bytes)
                .map_err(|e| LibraryError::Meta(format!("zip: {e}")))?;
        }
        writer
            .finish()
            .map_err(|e| LibraryError::Meta(format!("zip: {e}")))?;
    }
    Ok(cursor.into_inner())
}

/// What an import produced.
///
/// Separate from [`PackageSummary`] because the upgrade is a fact about the
/// *import*, not about the installed package — once the migrated bytes are
/// on disk the package is an ordinary current-format one, and only the
/// import's own notice has anything to say about where it came from.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportOutcome {
    /// The installed package.
    pub summary: PackageSummary,
    /// The format the incoming files were at, when they had to be migrated
    /// before install. `None` when they were already current.
    pub upgraded_from: Option<u32>,
}

/// Classify incoming package files, migrating them forward when this build
/// can and refusing when it cannot.
///
/// Returns the files to install (migrated in place where a step ran) and
/// the format they came from, if any. Everything is decided **before** a
/// byte is written: a refusal leaves the library exactly as it was.
fn gate_and_migrate(
    files: Vec<(String, Vec<u8>)>,
) -> Result<(Vec<(String, Vec<u8>)>, Option<u32>), LibraryError> {
    let mut project: ProjectFiles = files.into_iter().collect();
    match classify(&project) {
        FormatClass::Current => Ok((project.into_pairs(), None)),
        FormatClass::Upgradable { found } => {
            // All-or-nothing by contract: a refusal here leaves `project`
            // untouched, and nothing has been installed either way.
            let report = upgrade_to_current(&mut project).map_err(|error| {
                LibraryError::Format(format!(
                    "project format {found} could not be upgraded automatically: {error}"
                ))
            })?;
            Ok((project.into_pairs(), Some(report.from)))
        }
        other => Err(LibraryError::Format(other.describe())),
    }
}

/// Install a package from zip bytes. See module docs for uid semantics and
/// the format gate.
pub fn import_zip(
    store: &LibraryStore,
    bytes: &[u8],
    now: f64,
) -> Result<ImportOutcome, LibraryError> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| LibraryError::Meta(format!("not a zip archive: {e}")))?;

    // collect entries, tolerating archiver noise
    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|e| LibraryError::Meta(format!("zip entry: {e}")))?;
        if file.is_dir() {
            continue;
        }
        let name = file.name().to_string();
        if name.starts_with("__MACOSX/") || name.ends_with(".DS_Store") {
            continue;
        }
        let mut content = Vec::new();
        file.read_to_end(&mut content)
            .map_err(|e| LibraryError::Meta(format!("zip read {name}: {e}")))?;
        entries.push((name, content));
    }

    // locate the directory holding project.json (top level or one deep)
    let manifest_entry = entries
        .iter()
        .map(|(name, _)| name.as_str())
        .filter(|name| *name == "project.json" || name.ends_with("/project.json"))
        .min_by_key(|name| name.matches('/').count())
        .ok_or_else(|| LibraryError::Manifest("no project.json in this zip".to_string()))?;
    let prefix = manifest_entry.trim_end_matches("project.json").to_string();

    let files: Vec<(String, Vec<u8>)> = entries
        .iter()
        .filter_map(|(name, bytes)| {
            name.strip_prefix(&prefix)
                .map(|relative| (relative.to_string(), bytes.clone()))
        })
        .filter(|(relative, _)| !relative.is_empty())
        .collect();

    // Gate + migrate BEFORE anything is written, so a too-old archive is
    // refused rather than installed and discovered later.
    let (files, upgraded_from) = gate_and_migrate(files)?;

    // the archive's own identity, if it had one, rides the provenance
    let manifest_bytes = files
        .iter()
        .find(|(relative, _)| relative == "project.json")
        .map(|(_, bytes)| bytes.clone())
        .expect("manifest located above");
    let manifest: serde_json::Value = serde_json::from_slice(&manifest_bytes)
        .map_err(|e| LibraryError::Manifest(format!("zip project.json: {e}")))?;
    let original_uid = manifest
        .get("uid")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let name = manifest
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("Imported project")
        .to_string();

    let summary = store.install_files_with_fresh_uid(
        &name,
        &files,
        PackageProvenance::ImportedZip { original_uid },
        now,
    )?;
    Ok(ImportOutcome {
        summary,
        upgraded_from,
    })
}

/// Install a package from a pasted `lp.package` share envelope.
///
/// Same identity semantics as [`import_zip`] — fresh uid, source uid kept
/// on the provenance — and the same format gate, for the same reason: the
/// envelope's own `format` field versions the ENVELOPE, and says nothing
/// about the project format of the `project.json` inside it.
/// See `docs/adr/2026-07-28-share-envelopes.md`.
pub fn import_json(
    store: &LibraryStore,
    text: &str,
    now: f64,
) -> Result<ImportOutcome, LibraryError> {
    let envelope = crate::app::share::PackageEnvelope::decode(text)
        .map_err(|error| LibraryError::Manifest(error.to_string()))?;
    let original_uid = envelope.original_uid();
    let name = envelope.name.clone();
    let files = envelope
        .into_files()
        .map_err(|error| LibraryError::Manifest(error.to_string()))?;
    let (files, upgraded_from) = gate_and_migrate(files)?;

    let summary = store.install_files_with_fresh_uid(
        &name,
        &files,
        PackageProvenance::ImportedJson { original_uid },
        now,
    )?;
    Ok(ImportOutcome {
        summary,
        upgraded_from,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpfs::LpFsMemory;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn store_with_seed(seed: u8) -> LibraryStore {
        LibraryStore::new(
            Rc::new(RefCell::new(LpFsMemory::new())),
            Rc::new(move || [seed; 16]),
            Rc::new(|| "2026-07-09-1421".to_string()),
        )
    }

    fn store() -> LibraryStore {
        store_with_seed(9)
    }

    fn seeded(store: &LibraryStore) -> PackageSummary {
        store
            .install_package(
                "demo",
                &[
                    (
                        "project.json".to_string(),
                        br#"{"format":10,"name":"demo"}"#.to_vec(),
                    ),
                    ("module.json".to_string(), br#"{"kind":"Module"}"#.to_vec()),
                    ("shader.glsl".to_string(), b"void main() {}".to_vec()),
                    ("assets/map.bin".to_string(), vec![0u8, 159, 146, 150]),
                ],
                PackageProvenance::Created,
                1.0,
            )
            .unwrap()
    }

    #[test]
    fn export_import_round_trips_with_fresh_uid() {
        let source_store = store();
        let source = seeded(&source_store);
        let handle = source_store.open(source.uid).unwrap();
        let bytes = export_package(&handle).unwrap();

        let dest_store = store_with_seed(42);
        let imported = import_zip(&dest_store, &bytes, 2.0).unwrap();
        assert_eq!(
            imported.upgraded_from, None,
            "a current archive is installed as-is"
        );
        let imported = imported.summary;
        assert_ne!(imported.uid, source.uid, "import must mint a fresh uid");
        assert_eq!(imported.name, "demo");

        let imported_handle = dest_store.open(imported.uid).unwrap();
        let files = imported_handle.read_all_files().unwrap();
        let shader = files.iter().find(|(p, _)| p == "shader.glsl").unwrap();
        assert_eq!(shader.1, b"void main() {}");
        let binary = files.iter().find(|(p, _)| p == "assets/map.bin").unwrap();
        assert_eq!(binary.1, vec![0u8, 159, 146, 150]);

        // provenance records the original uid
        let meta = super::super::package_meta::read_meta(&*imported_handle.package_fs.borrow())
            .unwrap()
            .unwrap();
        assert_eq!(
            meta.provenance,
            PackageProvenance::ImportedZip {
                original_uid: Some(source.uid.to_string())
            }
        );
    }

    #[test]
    fn tolerates_finder_noise_and_nesting() {
        let source_store = store();
        let source = seeded(&source_store);
        let handle = source_store.open(source.uid).unwrap();
        let clean = export_package(&handle).unwrap();

        // rebuild the archive with Finder junk alongside the nested dir
        let mut archive = zip::ZipArchive::new(Cursor::new(clean.as_slice())).unwrap();
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut cursor);
            let options = SimpleFileOptions::default();
            writer.start_file("__MACOSX/._junk", options).unwrap();
            writer.write_all(b"junk").unwrap();
            writer.start_file("demo/.DS_Store", options).unwrap();
            writer.write_all(b"junk").unwrap();
            for index in 0..archive.len() {
                let mut file = archive.by_index(index).unwrap();
                let name = file.name().to_string();
                let mut content = Vec::new();
                file.read_to_end(&mut content).unwrap();
                writer.start_file(name, options).unwrap();
                writer.write_all(&content).unwrap();
            }
            writer.finish().unwrap();
        }

        let dest_store = store();
        let imported = import_zip(&dest_store, &cursor.into_inner(), 2.0).unwrap();
        let files = dest_store
            .open(imported.summary.uid)
            .unwrap()
            .read_all_files()
            .unwrap();
        assert!(files.iter().any(|(p, _)| p == "shader.glsl"));
        assert!(!files.iter().any(|(p, _)| p.contains("DS_Store")));
    }

    #[test]
    fn missing_manifest_errors_cleanly() {
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut cursor);
            writer
                .start_file("readme.txt", SimpleFileOptions::default())
                .unwrap();
            writer.write_all(b"hi").unwrap();
            writer.finish().unwrap();
        }
        let err = import_zip(&store(), &cursor.into_inner(), 1.0).unwrap_err();
        assert!(err.to_string().contains("no project.json"));

        let err = import_zip(&store(), b"not a zip", 1.0).unwrap_err();
        assert!(err.to_string().contains("not a zip"));
    }

    /// A real format-4 project, read from the corpus `lpa-upgrade` already
    /// keeps honest. Borrowed rather than copied so the two crates cannot
    /// drift apart about what a format-4 project looks like — and never
    /// mutated: callers clone what they need.
    fn corpus_v4(project: &str) -> Vec<(String, Vec<u8>)> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../lpa-upgrade/tests/corpus/v4")
            .join(project);
        let mut files = Vec::new();
        read_tree(&root, &root, &mut files);
        assert!(!files.is_empty(), "corpus project {project} is missing");
        files.sort();
        files
    }

    fn read_tree(root: &std::path::Path, dir: &std::path::Path, out: &mut Vec<(String, Vec<u8>)>) {
        for entry in std::fs::read_dir(dir).expect("read the corpus directory") {
            let path = entry.expect("a corpus entry").path();
            if path.is_dir() {
                read_tree(root, &path, out);
                continue;
            }
            let relative = path
                .strip_prefix(root)
                .expect("under the project root")
                .to_string_lossy()
                .into_owned();
            out.push((relative, std::fs::read(&path).expect("a corpus file")));
        }
    }

    /// Replace `project.json`'s `format` with `found`, leaving everything
    /// else alone — the shape of an archive from a build we cannot read.
    fn restamped(files: &[(String, Vec<u8>)], found: u32) -> Vec<(String, Vec<u8>)> {
        files
            .iter()
            .map(|(path, bytes)| {
                if path != "project.json" {
                    return (path.clone(), bytes.clone());
                }
                let mut manifest: serde_json::Value = serde_json::from_slice(bytes).unwrap();
                manifest["format"] = serde_json::json!(found);
                (path.clone(), serde_json::to_vec_pretty(&manifest).unwrap())
            })
            .collect()
    }

    fn zip_of(files: &[(String, Vec<u8>)]) -> Vec<u8> {
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut cursor);
            for (path, bytes) in files {
                writer
                    .start_file(format!("plasma/{path}"), SimpleFileOptions::default())
                    .unwrap();
                writer.write_all(bytes).unwrap();
            }
            writer.finish().unwrap();
        }
        cursor.into_inner()
    }

    fn installed(store: &LibraryStore, summary: &PackageSummary) -> Vec<(String, Vec<u8>)> {
        store
            .open(summary.uid)
            .unwrap()
            .read_all_files()
            .unwrap()
            .into_iter()
            .filter(|(path, _)| !path.starts_with(".lp/"))
            .collect()
    }

    #[test]
    fn a_stale_archive_is_upgraded_before_it_is_installed() {
        // The 2026-07-24 shape: a format-4 archive used to install cleanly
        // and then fail per-node on open. It must arrive migrated instead,
        // and the import must say so.
        let source = corpus_v4("plasma");
        let store = store();
        let outcome = import_zip(&store, &zip_of(&source), 2.0).unwrap();
        assert_eq!(outcome.upgraded_from, Some(4));

        let files = installed(&store, &outcome.summary);
        let manifest: serde_json::Value = serde_json::from_slice(
            &files
                .iter()
                .find(|(path, _)| path == "project.json")
                .unwrap()
                .1,
        )
        .unwrap();
        assert_eq!(manifest["format"], lpc_model::PROJECT_FORMAT_VERSION);

        // The manifest bump alone would prove nothing: the artifact the
        // v4→v5 step retypes must have moved too.
        let before = source.iter().find(|(p, _)| p == "shader.json").unwrap();
        let after = files.iter().find(|(p, _)| p == "shader.json").unwrap();
        assert_ne!(before.1, after.1, "the shader slot was not migrated");

        // ...and the v5→v6 step renames the GLSL entry, so the shader asset
        // moves too — to the explicit entry name, with the bare one gone.
        let before = source.iter().find(|(p, _)| p == "shader.glsl").unwrap();
        let after = files.iter().find(|(p, _)| p == "shader.glsl").unwrap();
        assert_ne!(before.1, after.1, "the GLSL entry was not migrated");
        let text = core::str::from_utf8(&after.1).expect("utf8 glsl");
        assert!(text.contains("vec4 render_2d("), "explicit entry expected");
        assert!(!text.contains("vec4 render("), "bare entry must be gone");
    }

    #[test]
    fn an_archive_below_the_floor_is_refused_loudly_and_installs_nothing() {
        let store = store();
        let stale = restamped(&corpus_v4("plasma"), 3);
        let error = import_zip(&store, &zip_of(&stale), 2.0).unwrap_err();
        assert!(
            matches!(error, LibraryError::Format(_)),
            "a format refusal must be classified, not a manifest complaint: {error}"
        );
        let message = error.to_string();
        assert!(message.contains('3'), "names what was found: {message}");
        assert!(
            message.contains(&lpc_model::PROJECT_FORMAT_VERSION.to_string()),
            "names what was expected: {message}"
        );
        assert!(message.contains("too old"), "names a remedy: {message}");
        assert!(
            store.list().unwrap().is_empty(),
            "a refused import must leave the library untouched"
        );
    }

    #[test]
    fn an_archive_from_a_newer_lightplayer_is_refused() {
        let store = store();
        let future = restamped(&corpus_v4("plasma"), 99);
        let error = import_zip(&store, &zip_of(&future), 2.0).unwrap_err();
        assert!(matches!(error, LibraryError::Format(_)), "{error}");
        let message = error.to_string();
        assert!(message.contains("99"), "{message}");
        assert!(message.contains("Update LightPlayer"), "{message}");
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn a_pasted_envelope_carrying_an_old_project_is_migrated_on_import() {
        // The envelope's own `format` is current — it is the project.json
        // INSIDE it that is stale, which is exactly the case the envelope
        // header cannot see.
        let source = corpus_v4("plasma");
        let json = crate::app::share::PackageEnvelope::encode("Plasma", &source)
            .to_json()
            .unwrap();

        let store = store();
        let outcome = import_json(&store, &json, 2.0).unwrap();
        assert_eq!(outcome.upgraded_from, Some(4));

        let files = installed(&store, &outcome.summary);
        let after = files.iter().find(|(p, _)| p == "shader.json").unwrap();
        let before = source.iter().find(|(p, _)| p == "shader.json").unwrap();
        assert_ne!(before.1, after.1);
    }

    #[test]
    fn a_pasted_envelope_below_the_floor_is_refused() {
        let stale = restamped(&corpus_v4("plasma"), 2);
        let json = crate::app::share::PackageEnvelope::encode("Plasma", &stale)
            .to_json()
            .unwrap();
        let store = store();
        let error = import_json(&store, &json, 2.0).unwrap_err();
        assert!(matches!(error, LibraryError::Format(_)), "{error}");
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn export_excludes_history_and_includes_sidecar() {
        let source_store = store();
        let source = seeded(&source_store);
        let handle = source_store.open(source.uid).unwrap();
        let bytes = export_package(&handle).unwrap();

        let mut archive = zip::ZipArchive::new(Cursor::new(bytes.as_slice())).unwrap();
        let names: Vec<String> = (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().to_string())
            .collect();
        assert!(names.iter().any(|n| n.ends_with(".lp/meta.json")));
        assert!(!names.iter().any(|n| n.contains("history")));
    }
}
