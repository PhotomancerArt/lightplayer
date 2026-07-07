//! Canonical content hashing: file hashes, tree manifests, package hashes.

pub mod content_hash;
pub mod hash_rules;
pub mod package_hasher;
pub mod tree_manifest;

pub use content_hash::ContentHash;
pub use hash_rules::is_hashed_path;
pub use package_hasher::hash_package;
pub use tree_manifest::{TreeEntry, TreeManifest};
