//! Hash a whole package filesystem into a tree manifest + package hash.

use alloc::vec::Vec;

use lpfs::{FsError, LpFs, LpPath};

use crate::hash::content_hash::ContentHash;
use crate::hash::hash_rules::is_hashed_path;
use crate::hash::tree_manifest::{TreeEntry, TreeManifest};
use crate::history_error::HistoryError;

/// Hash every file of a package (per the hash rules) into a manifest.
///
/// Deterministic regardless of `list_dir` enumeration order — the manifest
/// sorts entries by path bytes.
pub fn hash_package(fs: &dyn LpFs) -> Result<(ContentHash, TreeManifest), HistoryError> {
    let paths = match fs.list_dir(LpPath::new("/"), true) {
        Ok(paths) => paths,
        // an empty package directory may not exist yet on some backends
        Err(FsError::NotFound(_)) => Vec::new(),
        Err(e) => return Err(e.into()),
    };

    let mut entries = Vec::new();
    for path in paths {
        if !is_hashed_path(&path) {
            continue;
        }
        if fs.is_dir(&path)? {
            continue;
        }
        let bytes = fs.read_file(&path)?;
        entries.push(TreeEntry {
            path,
            hash: ContentHash::of(&bytes),
        });
    }
    let manifest = TreeManifest::from_entries(entries)?;
    Ok((manifest.package_hash(), manifest))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpfs::LpFsMemory;

    fn write(fs: &LpFsMemory, path: &str, data: &[u8]) {
        fs.write_file(LpPath::new(path), data).unwrap();
    }

    #[test]
    fn empty_package_hashes_cleanly() {
        let fs = LpFsMemory::new();
        let (hash, manifest) = hash_package(&fs).unwrap();
        assert!(manifest.entries().is_empty());
        assert_eq!(hash, manifest.package_hash());
    }

    #[test]
    fn single_file_package() {
        let fs = LpFsMemory::new();
        write(&fs, "/project.json", b"{}");
        let (_, manifest) = hash_package(&fs).unwrap();
        assert_eq!(manifest.entries().len(), 1);
        assert_eq!(manifest.entries()[0].path.as_str(), "/project.json");
        assert_eq!(manifest.entries()[0].hash, ContentHash::of(b"{}"));
    }

    #[test]
    fn reserved_namespace_does_not_affect_the_hash() {
        let fs = LpFsMemory::new();
        write(&fs, "/project.json", b"{}");
        write(&fs, "/shader.glsl", b"void main() {}");
        let (before, _) = hash_package(&fs).unwrap();

        write(&fs, "/.lp/meta.json", b"{\"origin\":\"somewhere\"}");
        let (after, _) = hash_package(&fs).unwrap();
        assert_eq!(before, after);

        write(&fs, "/shader.glsl", b"void main() { }");
        let (changed, _) = hash_package(&fs).unwrap();
        assert_ne!(before, changed);
    }

    #[test]
    fn nested_files_are_included() {
        let fs = LpFsMemory::new();
        write(&fs, "/modules/plasma/module.json", b"{}");
        let (_, manifest) = hash_package(&fs).unwrap();
        assert_eq!(manifest.entries().len(), 1);
        assert_eq!(
            manifest.entries()[0].path.as_str(),
            "/modules/plasma/module.json"
        );
    }

    #[test]
    fn known_answer_vector_pins_the_preimage() {
        // Fixed tiny package with a committed expected hash. If this test
        // fails, the canonical preimage changed — that must be a loud,
        // deliberate act (bump the lph format tag and update this vector).
        let fs = LpFsMemory::new();
        write(
            &fs,
            "/project.json",
            b"{\"kind\":\"Project\",\"name\":\"kat\"}",
        );
        write(&fs, "/shader.glsl", b"void main() {}\n");
        let (hash, _) = hash_package(&fs).unwrap();
        assert_eq!(
            alloc::format!("{hash}"),
            "4871c9a4ff89d732ed427d084e3ccbffb72d4ee5821a7ff81d36530f9d98bfc7"
        );
    }
}
