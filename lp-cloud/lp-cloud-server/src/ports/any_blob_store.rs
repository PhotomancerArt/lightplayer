//! Whichever [`BlobStore`] the configuration named.

use lp_cloud_domain::BlobStore;
use lpc_history::ContentHash;

/// A [`BlobStore`] chosen at runtime — the blob-plane twin of
/// [`AnyMetaStore`](crate::ports::any_meta_store::AnyMetaStore), and for the
/// same reason: one concrete state type for the router, three possible
/// backends behind it (memory, filesystem, S3).
pub struct AnyBlobStore(Box<dyn BlobStore + Send>);

impl AnyBlobStore {
    /// Wrap a concrete adapter.
    pub fn new(store: impl BlobStore + Send + 'static) -> Self {
        Self(Box::new(store))
    }
}

impl BlobStore for AnyBlobStore {
    fn has(&self, hash: ContentHash) -> bool {
        self.0.has(hash)
    }

    fn get(&self, hash: ContentHash) -> Option<Vec<u8>> {
        self.0.get(hash)
    }

    fn put(&mut self, bytes: &[u8]) -> ContentHash {
        self.0.put(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_cloud_store_mem::MemBlobStore;

    #[test]
    fn forwards_to_the_wrapped_store() {
        let mut store = AnyBlobStore::new(MemBlobStore::new());
        let hash = store.put(b"hello");

        assert!(store.has(hash));
        assert_eq!(store.get(hash).as_deref(), Some(&b"hello"[..]));
        assert_eq!(store.get(ContentHash::of(b"absent")), None);
    }
}
