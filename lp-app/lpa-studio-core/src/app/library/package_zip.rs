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

use std::io::{Cursor, Read, Write};

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

/// Install a package from zip bytes. See module docs for uid semantics.
pub fn import_zip(
    store: &LibraryStore,
    bytes: &[u8],
    now: f64,
) -> Result<PackageSummary, LibraryError> {
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

    store.install_files_with_fresh_uid(
        &name,
        &files,
        PackageProvenance::ImportedZip { original_uid },
        now,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpfs::{LpFs, LpFsMemory};
    use std::cell::RefCell;
    use std::rc::Rc;

    fn store_with_seed(seed: u8) -> LibraryStore {
        LibraryStore::new(
            Rc::new(RefCell::new(LpFsMemory::new())),
            Rc::new(move || [seed; 16]),
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
                        br#"{"kind":"Project","name":"demo","nodes":{}}"#.to_vec(),
                    ),
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
            .open(imported.uid)
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
