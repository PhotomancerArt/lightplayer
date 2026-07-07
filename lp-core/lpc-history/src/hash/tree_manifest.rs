//! Tree manifest: the sorted (path, file-hash) listing of a package version.
//!
//! # Canonical package-hash preimage
//!
//! The package hash is SHA-256 over exactly these bytes:
//!
//! ```text
//! "lph1\n"
//! for each entry, in ascending path-byte order:
//!     <path utf-8 bytes> 0x00 <64 lowercase hex chars of file hash> "\n"
//! ```
//!
//! The leading `lph1` format tag exists so any future canonicalization
//! change bumps the tag (`lph2`, …) instead of silently colliding with
//! hashes computed under the old rules. Changing this preimage in any way
//! must be a loud, deliberate act — the known-answer test pins it.

use alloc::string::ToString;
use alloc::vec::Vec;

use lpfs::LpPathBuf;
use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};

use crate::hash::content_hash::ContentHash;
use crate::history_error::HistoryError;

/// Format tag baked into the package-hash preimage.
pub const TREE_FORMAT_TAG: &str = "lph1";

/// One file in a package version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreeEntry {
    /// Absolute package path (e.g. `/shader.glsl`).
    pub path: LpPathBuf,
    /// Hash of the file's bytes.
    pub hash: ContentHash,
}

/// Sorted, duplicate-free listing of a package version's hashed files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TreeManifest {
    entries: Vec<TreeEntry>,
}

impl TreeManifest {
    /// Build a manifest; entries are sorted by path bytes, duplicates rejected.
    pub fn from_entries(mut entries: Vec<TreeEntry>) -> Result<Self, HistoryError> {
        entries.sort_by(|a, b| a.path.as_str().as_bytes().cmp(b.path.as_str().as_bytes()));
        for pair in entries.windows(2) {
            if pair[0].path == pair[1].path {
                return Err(HistoryError::DuplicateTreePath(
                    pair[0].path.as_str().to_string(),
                ));
            }
        }
        Ok(Self { entries })
    }

    pub fn entries(&self) -> &[TreeEntry] {
        &self.entries
    }

    /// The canonical package hash (see module docs for the exact preimage).
    pub fn package_hash(&self) -> ContentHash {
        let mut hasher = Sha256::new();
        hasher.update(TREE_FORMAT_TAG.as_bytes());
        hasher.update(b"\n");
        for entry in &self.entries {
            hasher.update(entry.path.as_str().as_bytes());
            hasher.update([0u8]);
            hasher.update(entry.hash.to_string().as_bytes());
            hasher.update(b"\n");
        }
        ContentHash::from_bytes(hasher.finalize().into())
    }
}

impl<'de> Deserialize<'de> for TreeManifest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Raw {
            entries: Vec<TreeEntry>,
        }
        let raw = Raw::deserialize(deserializer)?;
        TreeManifest::from_entries(raw.entries).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    fn entry(path: &str, data: &[u8]) -> TreeEntry {
        TreeEntry {
            path: LpPathBuf::from(path),
            hash: ContentHash::of(data),
        }
    }

    #[test]
    fn insertion_order_does_not_matter() {
        let a = TreeManifest::from_entries(vec![entry("/a", b"1"), entry("/b", b"2")]).unwrap();
        let b = TreeManifest::from_entries(vec![entry("/b", b"2"), entry("/a", b"1")]).unwrap();
        assert_eq!(a.package_hash(), b.package_hash());
        assert_eq!(a, b);
    }

    #[test]
    fn content_changes_change_the_hash() {
        let a = TreeManifest::from_entries(vec![entry("/a", b"1")]).unwrap();
        let b = TreeManifest::from_entries(vec![entry("/a", b"2")]).unwrap();
        let c = TreeManifest::from_entries(vec![entry("/b", b"1")]).unwrap();
        assert_ne!(a.package_hash(), b.package_hash());
        assert_ne!(a.package_hash(), c.package_hash());
    }

    #[test]
    fn duplicate_paths_rejected() {
        let err = TreeManifest::from_entries(vec![entry("/a", b"1"), entry("/a", b"2")]);
        assert!(matches!(err, Err(HistoryError::DuplicateTreePath(_))));
    }

    #[test]
    fn empty_manifest_hashes_the_tag_only() {
        let empty = TreeManifest::from_entries(vec![]).unwrap();
        assert_eq!(empty.package_hash(), ContentHash::of(b"lph1\n"));
    }

    #[test]
    fn serde_round_trip_revalidates() {
        let manifest =
            TreeManifest::from_entries(vec![entry("/a", b"1"), entry("/b", b"2")]).unwrap();
        let json = serde_json::to_string(&manifest).unwrap();
        let back: TreeManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(back, manifest);

        // duplicate entries in raw JSON are rejected on deserialize
        let dup = json.replace("/b", "/a");
        let bad: Result<TreeManifest, _> = serde_json::from_str(&dup);
        assert!(bad.is_err());
    }
}
