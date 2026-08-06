//! Blob bytes as files under a root directory.
//!
//! This is the default blob backend: a directory of content-addressed
//! files, sharded by the first two hex characters of the hash (see
//! [`crate::blob_layout`]). It is what the tests use, what local dev uses,
//! and a perfectly good production backend for a single node with a disk.
//!
//! # Write-tmp-then-rename
//!
//! A blob is written to a temporary file and then `rename`d into place.
//! `rename` within a filesystem is atomic, so a reader can only ever see a
//! blob that is complete: no torn file, no half-written PNG, and no need
//! for a "is this one finished?" flag. A crash mid-write leaves a stray
//! temporary file and nothing else — the blob simply is not there, and the
//! client re-uploads it.
//!
//! The temporary file lives under the same root as the destination, so the
//! rename is always within one filesystem. A `/tmp` staging directory would
//! be a cross-device rename, which is not atomic and not even permitted.
//!
//! # What failure does
//!
//! Nothing here returns a `Result`: see [`crate::store_fatal`]. A blob
//! store that cannot read its own disk is a dead node.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use lp_cloud_domain::BlobStore;
use lpc_history::ContentHash;

use crate::blob_layout::blob_object_path;
use crate::store_fatal::fatal;

/// Directory holding the staging files for in-flight writes.
const TMP_DIR: &str = "tmp";

/// Content-addressed blob storage in a directory tree.
#[derive(Debug)]
pub struct FsBlobStore {
    root: PathBuf,
    /// Distinguishes the staging files of concurrent writes of the *same*
    /// blob, which would otherwise collide on one temporary name.
    next_tmp: u64,
}

impl FsBlobStore {
    /// Open (creating if needed) a blob store rooted at `root`.
    pub fn open(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        let tmp = root.join(TMP_DIR);
        fatal(
            &format!("creating the blob directory {}", tmp.display()),
            fs::create_dir_all(&tmp),
        );
        Self { root, next_tmp: 0 }
    }

    /// The directory this store keeps blobs under.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Where this blob's bytes live.
    pub fn path_for(&self, hash: ContentHash) -> PathBuf {
        self.root.join(blob_object_path(hash))
    }

    /// A staging path no other in-flight write is using.
    fn next_tmp_path(&mut self, hash: ContentHash) -> PathBuf {
        self.next_tmp += 1;
        self.root.join(TMP_DIR).join(format!(
            "{hash}.{}.{}.tmp",
            std::process::id(),
            self.next_tmp
        ))
    }
}

impl BlobStore for FsBlobStore {
    fn has(&self, hash: ContentHash) -> bool {
        self.path_for(hash).is_file()
    }

    fn get(&self, hash: ContentHash) -> Option<Vec<u8>> {
        let path = self.path_for(hash);
        match fs::read(&path) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => panic!(
                "lp-cloud-store-sqlite: reading blob {} failed: {error}",
                path.display()
            ),
        }
    }

    fn put(&mut self, bytes: &[u8]) -> ContentHash {
        let hash = ContentHash::of(bytes);
        let destination = self.path_for(hash);
        if destination.is_file() {
            // Content-addressed: the bytes already there are these bytes.
            return hash;
        }

        let shard = destination
            .parent()
            .expect("a blob path always has a shard directory");
        fatal(
            &format!("creating the blob shard {}", shard.display()),
            fs::create_dir_all(shard),
        );

        let staging = self.next_tmp_path(hash);
        fatal(
            &format!("writing the blob staging file {}", staging.display()),
            fs::write(&staging, bytes),
        );
        fatal(
            &format!("publishing the blob {}", destination.display()),
            fs::rename(&staging, &destination),
        );
        hash
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    /// The layout is a promise to operators (and to the S3 adapter): a blob
    /// is at `<root>/ab/cdef…`, and nothing else is left behind.
    #[test]
    fn a_stored_blob_lands_at_its_sharded_path() {
        let dir = tempdir().unwrap();
        let mut store = FsBlobStore::open(dir.path());

        let hash = store.put(b"hello");
        let hex = hash.to_string();
        let expected = dir.path().join(&hex[..2]).join(&hex[2..]);

        assert!(expected.is_file());
        assert_eq!(fs::read(&expected).unwrap(), b"hello");
        assert_eq!(store.path_for(hash), expected);
    }

    /// A crash between the write and the rename must not publish a partial
    /// blob, so nothing is ever written directly at the destination.
    #[test]
    fn staging_files_do_not_survive_a_completed_write() {
        let dir = tempdir().unwrap();
        let mut store = FsBlobStore::open(dir.path());
        store.put(b"hello");
        store.put(b"hello again");

        let leftovers: Vec<_> = fs::read_dir(dir.path().join(TMP_DIR))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        assert!(
            leftovers.is_empty(),
            "staging files left behind: {leftovers:?}"
        );
    }

    /// Reopening a store finds what the last one wrote — the difference
    /// between a blob store and a cache.
    #[test]
    fn blobs_survive_reopening_the_directory() {
        let dir = tempdir().unwrap();
        let hash = FsBlobStore::open(dir.path()).put(b"persisted");

        let store = FsBlobStore::open(dir.path());
        assert!(store.has(hash));
        assert_eq!(store.get(hash).as_deref(), Some(&b"persisted"[..]));
    }
}
