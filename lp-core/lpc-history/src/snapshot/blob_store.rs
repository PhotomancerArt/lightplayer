//! Content-addressed file blobs under `blobs/` in a history root.

use alloc::vec::Vec;

use lpfs::{FsError, LpFs, LpPathBuf};

use crate::hash::content_hash::ContentHash;
use crate::history_error::HistoryError;

/// Content-addressed blob storage over a caller-supplied history-root fs.
///
/// Blobs live flat at `/blobs/<hex64>`; sharding into subdirectories is
/// future work if a project's history ever grows enough to need it.
pub struct BlobStore<'a> {
    fs: &'a dyn LpFs,
}

impl<'a> BlobStore<'a> {
    pub fn new(fs: &'a dyn LpFs) -> Self {
        Self { fs }
    }

    fn blob_path(hash: &ContentHash) -> LpPathBuf {
        LpPathBuf::new(alloc::format!("/blobs/{hash}"))
    }

    pub fn has(&self, hash: &ContentHash) -> Result<bool, HistoryError> {
        Ok(self.fs.file_exists(&Self::blob_path(hash))?)
    }

    /// Store bytes, returning their hash. No-op if the blob already exists.
    pub fn put(&self, bytes: &[u8]) -> Result<ContentHash, HistoryError> {
        let hash = ContentHash::of(bytes);
        let path = Self::blob_path(&hash);
        if !self.fs.file_exists(&path)? {
            self.fs.write_file(&path, bytes)?;
        }
        Ok(hash)
    }

    /// Fetch a blob, verifying its bytes still hash to the key.
    pub fn get(&self, hash: &ContentHash) -> Result<Vec<u8>, HistoryError> {
        let bytes = match self.fs.read_file(&Self::blob_path(hash)) {
            Ok(bytes) => bytes,
            Err(FsError::NotFound(_)) => return Err(HistoryError::MissingBlob(*hash)),
            Err(e) => return Err(e.into()),
        };
        if ContentHash::of(&bytes) != *hash {
            return Err(HistoryError::CorruptBlob(*hash));
        }
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpfs::{LpFsMemory, LpPath};

    #[test]
    fn put_get_round_trip() {
        let fs = LpFsMemory::new();
        let store = BlobStore::new(&fs);
        let hash = store.put(b"hello").unwrap();
        assert!(store.has(&hash).unwrap());
        assert_eq!(store.get(&hash).unwrap(), b"hello");
    }

    #[test]
    fn put_is_idempotent() {
        let fs = LpFsMemory::new();
        let store = BlobStore::new(&fs);
        let a = store.put(b"same").unwrap();
        let b = store.put(b"same").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn missing_blob_errors() {
        let fs = LpFsMemory::new();
        let store = BlobStore::new(&fs);
        let absent = ContentHash::of(b"never stored");
        assert!(matches!(store.get(&absent), Err(HistoryError::MissingBlob(h)) if h == absent));
    }

    #[test]
    fn tampered_blob_is_detected() {
        let fs = LpFsMemory::new();
        let store = BlobStore::new(&fs);
        let hash = store.put(b"original").unwrap();
        let path = alloc::format!("/blobs/{hash}");
        fs.write_file(LpPath::new(&path), b"tampered").unwrap();
        assert!(matches!(store.get(&hash), Err(HistoryError::CorruptBlob(h)) if h == hash));
    }
}
