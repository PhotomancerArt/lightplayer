//! Blob bytes in a map.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use lp_cloud_domain::BlobStore;
use lpc_history::ContentHash;

/// Content-addressed blob storage in memory.
///
/// Fine for tests and local dev, where a project's whole content is a few
/// hundred kilobytes. It has no eviction and no size cap, so it is not a
/// backend for a real deployment — that is `lp-cloud-store-sqlite`'s fs and
/// S3 adapters.
#[derive(Debug, Clone, Default)]
pub struct MemBlobStore {
    blobs: BTreeMap<ContentHash, Vec<u8>>,
}

impl MemBlobStore {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// How many blobs are held.
    pub fn len(&self) -> usize {
        self.blobs.len()
    }

    /// Whether the store holds nothing.
    pub fn is_empty(&self) -> bool {
        self.blobs.is_empty()
    }

    /// Every stored hash, in hash order.
    pub fn hashes(&self) -> Vec<ContentHash> {
        self.blobs.keys().copied().collect()
    }
}

impl BlobStore for MemBlobStore {
    fn has(&self, hash: ContentHash) -> bool {
        self.blobs.contains_key(&hash)
    }

    fn get(&self, hash: ContentHash) -> Option<Vec<u8>> {
        self.blobs.get(&hash).cloned()
    }

    fn put(&mut self, bytes: &[u8]) -> ContentHash {
        let hash = ContentHash::of(bytes);
        self.blobs.entry(hash).or_insert_with(|| bytes.to_vec());
        hash
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_is_idempotent() {
        let mut store = MemBlobStore::new();
        assert!(store.is_empty());

        let hash = store.put(b"hello");
        assert!(store.has(hash));
        assert_eq!(store.get(hash).as_deref(), Some(&b"hello"[..]));

        assert_eq!(store.put(b"hello"), hash);
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn missing_blob_is_none() {
        let store = MemBlobStore::new();
        let absent = ContentHash::of(b"never stored");
        assert!(!store.has(absent));
        assert_eq!(store.get(absent), None);
    }
}
