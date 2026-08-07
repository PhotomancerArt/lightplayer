//! The content plane: bytes addressed by content hash.
//!
//! Two route pairs, because there are two addressing rules (see
//! `lpa-cloud-client`'s `CloudPort`):
//!
//! - [`blob_route`] — `/b/{hash}`, where the address is the SHA-256 of the
//!   body, verified by recomputing it.
//! - [`tree_route`] — `/t/{hash}`, where the address is the manifest's
//!   *package* hash, verified by recomputing
//!   [`TreeManifest::package_hash`](lpc_history::TreeManifest::package_hash).
//!
//! Collapsing them into one route would mean a caller could upload a tree at
//! an address the server has no way to check, which is the one thing a
//! content-addressed store must never allow.

pub mod blob_route;
pub mod content_type;
pub mod tree_preimage;
pub mod tree_route;

/// `Cache-Control` for anything on this plane: content-addressed bytes can
/// never change under their address, so the answer is cacheable forever.
pub const IMMUTABLE_CACHE_CONTROL: &str = "public, max-age=31536000, immutable";
