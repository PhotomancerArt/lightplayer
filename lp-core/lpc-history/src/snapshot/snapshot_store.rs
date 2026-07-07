//! Package-version snapshots: blobs + tree manifests.

use alloc::string::ToString;

use lpfs::{FsError, LpFs, LpPathBuf};

use crate::hash::content_hash::ContentHash;
use crate::hash::package_hasher::hash_package;
use crate::hash::tree_manifest::TreeManifest;
use crate::history_error::HistoryError;
use crate::snapshot::blob_store::BlobStore;

/// Snapshot storage over a caller-supplied history-root fs.
///
/// Layout inside the history root:
///
/// ```text
/// blobs/<hex64>          file blobs (see BlobStore)
/// trees/<hex64>.json     tree manifests, keyed by package hash
/// ```
pub struct SnapshotStore<'a> {
    fs: &'a dyn LpFs,
}

impl<'a> SnapshotStore<'a> {
    pub fn new(fs: &'a dyn LpFs) -> Self {
        Self { fs }
    }

    fn blobs(&self) -> BlobStore<'a> {
        BlobStore::new(self.fs)
    }

    fn tree_path(hash: &ContentHash) -> LpPathBuf {
        LpPathBuf::new(alloc::format!("/trees/{hash}.json"))
    }

    pub fn has(&self, package_hash: &ContentHash) -> Result<bool, HistoryError> {
        Ok(self.fs.file_exists(&Self::tree_path(package_hash))?)
    }

    /// Snapshot a package: hash it, store any missing blobs, store the tree.
    ///
    /// Idempotent for a package hash that is already stored.
    pub fn put_package(
        &self,
        package_fs: &dyn LpFs,
    ) -> Result<(ContentHash, TreeManifest), HistoryError> {
        let (package_hash, manifest) = hash_package(package_fs)?;
        let blobs = self.blobs();
        for entry in manifest.entries() {
            if blobs.has(&entry.hash)? {
                continue;
            }
            let bytes = package_fs.read_file(&entry.path)?;
            let stored = blobs.put(&bytes)?;
            if stored != entry.hash {
                // the file changed between hashing and storing
                return Err(HistoryError::CorruptBlob(entry.hash));
            }
        }
        let tree_path = Self::tree_path(&package_hash);
        if !self.fs.file_exists(&tree_path)? {
            let json =
                serde_json::to_vec(&manifest).map_err(|e| HistoryError::Encode(e.to_string()))?;
            self.fs.write_file(&tree_path, &json)?;
        }
        Ok((package_hash, manifest))
    }

    /// Load the tree manifest for a stored package version.
    pub fn get_tree(&self, package_hash: &ContentHash) -> Result<TreeManifest, HistoryError> {
        let bytes = match self.fs.read_file(&Self::tree_path(package_hash)) {
            Ok(bytes) => bytes,
            Err(FsError::NotFound(_)) => return Err(HistoryError::MissingTree(*package_hash)),
            Err(e) => return Err(e.into()),
        };
        let manifest: TreeManifest = serde_json::from_slice(&bytes)
            .map_err(|_| HistoryError::MalformedTree(*package_hash))?;
        if manifest.package_hash() != *package_hash {
            return Err(HistoryError::MalformedTree(*package_hash));
        }
        Ok(manifest)
    }

    /// Write a stored package version's files into `dest`.
    ///
    /// Does **not** delete extra files already present at the destination and
    /// does **not** touch `/.lp/**` — callers own destination hygiene
    /// (restore/fork/push flows decide what a clean destination means).
    pub fn materialize(
        &self,
        package_hash: &ContentHash,
        dest: &dyn LpFs,
    ) -> Result<(), HistoryError> {
        let manifest = self.get_tree(package_hash)?;
        let blobs = self.blobs();
        for entry in manifest.entries() {
            let bytes = blobs.get(&entry.hash)?;
            dest.write_file(&entry.path, &bytes)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpfs::{LpFsMemory, LpPath};

    fn write(fs: &LpFsMemory, path: &str, data: &[u8]) {
        fs.write_file(LpPath::new(path), data).unwrap();
    }

    fn blob_count(fs: &LpFsMemory) -> usize {
        match fs.list_dir(LpPath::new("/blobs"), true) {
            Ok(paths) => paths
                .iter()
                .filter(|p| !fs.is_dir(p).unwrap_or(false))
                .count(),
            Err(_) => 0,
        }
    }

    #[test]
    fn round_trip_materialize() {
        let package = LpFsMemory::new();
        write(&package, "/project.json", b"{}");
        write(&package, "/shader.glsl", b"void main() {}");
        write(&package, "/.lp/meta.json", b"{\"origin\":\"x\"}");

        let history = LpFsMemory::new();
        let store = SnapshotStore::new(&history);
        let (hash, manifest) = store.put_package(&package).unwrap();
        assert!(store.has(&hash).unwrap());
        assert_eq!(manifest.entries().len(), 2);

        let dest = LpFsMemory::new();
        store.materialize(&hash, &dest).unwrap();
        assert_eq!(dest.read_file(LpPath::new("/project.json")).unwrap(), b"{}");
        assert_eq!(
            dest.read_file(LpPath::new("/shader.glsl")).unwrap(),
            b"void main() {}"
        );
        // the reserved namespace is never materialized
        assert!(!dest.file_exists(LpPath::new("/.lp/meta.json")).unwrap());
    }

    #[test]
    fn shared_files_are_stored_once() {
        let history = LpFsMemory::new();
        let store = SnapshotStore::new(&history);

        let v1 = LpFsMemory::new();
        write(&v1, "/project.json", b"{}");
        write(&v1, "/shader.glsl", b"v1");
        store.put_package(&v1).unwrap();
        assert_eq!(blob_count(&history), 2);

        // second version shares project.json — only the new shader blob is added
        let v2 = LpFsMemory::new();
        write(&v2, "/project.json", b"{}");
        write(&v2, "/shader.glsl", b"v2");
        store.put_package(&v2).unwrap();
        assert_eq!(blob_count(&history), 3);
    }

    #[test]
    fn re_put_is_idempotent() {
        let package = LpFsMemory::new();
        write(&package, "/project.json", b"{}");
        let history = LpFsMemory::new();
        let store = SnapshotStore::new(&history);
        let (a, _) = store.put_package(&package).unwrap();
        let (b, _) = store.put_package(&package).unwrap();
        assert_eq!(a, b);
        assert_eq!(blob_count(&history), 1);
    }

    #[test]
    fn missing_blob_on_materialize() {
        let package = LpFsMemory::new();
        write(&package, "/project.json", b"{}");
        let history = LpFsMemory::new();
        let store = SnapshotStore::new(&history);
        let (hash, manifest) = store.put_package(&package).unwrap();

        let blob = alloc::format!("/blobs/{}", manifest.entries()[0].hash);
        history.delete_file(LpPath::new(&blob)).unwrap();
        let dest = LpFsMemory::new();
        assert!(matches!(
            store.materialize(&hash, &dest),
            Err(HistoryError::MissingBlob(_))
        ));
    }

    #[test]
    fn tampered_tree_is_detected() {
        let package = LpFsMemory::new();
        write(&package, "/project.json", b"{}");
        let history = LpFsMemory::new();
        let store = SnapshotStore::new(&history);
        let (hash, _) = store.put_package(&package).unwrap();

        let tree_path = alloc::format!("/trees/{hash}.json");
        history
            .write_file(LpPath::new(&tree_path), b"not json")
            .unwrap();
        assert!(matches!(
            store.get_tree(&hash),
            Err(HistoryError::MalformedTree(_))
        ));

        let missing = ContentHash::of(b"nope");
        assert!(matches!(
            store.get_tree(&missing),
            Err(HistoryError::MissingTree(_))
        ));
    }
}
