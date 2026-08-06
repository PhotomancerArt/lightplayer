//! Content-addressed blob bytes — a plane the domain never reads.

use alloc::vec::Vec;
use lpc_history::ContentHash;

/// Storage for content-addressed bytes: tree manifests, file blobs, preview
/// PNGs.
///
/// Defined here because it is a *service* port, but deliberately not held by
/// [`CloudService`](crate::cloud_service::CloudService): blob transfer is
/// edge-level (`GET`/`PUT /b/<hash>`), and the domain only ever reasons
/// about hashes — whether the index has one, and which ones a push is
/// missing. Keeping bytes out of the domain is what makes "the server is
/// content-opaque" (D3) structural rather than a promise.
///
/// [`put`](BlobStore::put) takes bytes and *returns* their address rather
/// than accepting a caller-supplied hash: a content-addressed store that
/// lets the caller name the address can be made to lie. The edge compares
/// the returned hash against the one in the request URL and rejects a
/// mismatch.
pub trait BlobStore {
    /// Whether this blob is already stored.
    fn has(&self, hash: ContentHash) -> bool;

    /// Fetch a blob's bytes, or `None` if it is not stored.
    fn get(&self, hash: ContentHash) -> Option<Vec<u8>>;

    /// Store bytes under their own hash and return it. Idempotent.
    fn put(&mut self, bytes: &[u8]) -> ContentHash;
}
