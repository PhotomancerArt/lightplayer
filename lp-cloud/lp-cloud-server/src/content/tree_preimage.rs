//! Storing a tree manifest at its own package hash.
//!
//! # The problem
//!
//! [`BlobStore`](lp_cloud_domain::BlobStore) is content-addressed by
//! construction: `put` takes bytes and *returns* the address, so a caller
//! can never name one. A tree, though, is addressed by its **package hash**
//! — the canonical `lph1` preimage over its entries — which is not the hash
//! of the JSON that carries it on the wire. Storing the JSON would put the
//! manifest at an address nobody asks for.
//!
//! # The resolution
//!
//! Store the **preimage itself**. `lpc-history` defines it as
//!
//! ```text
//! "lph1\n"  then, per entry in ascending path order:
//! <path utf-8> 0x00 <64 lowercase hex of the file hash> "\n"
//! ```
//!
//! and it is losslessly decodable, so the bytes whose SHA-256 *is* the
//! package hash are also the bytes a manifest can be rebuilt from. The
//! generic blob store then holds trees at exactly the right address, with no
//! side index to keep in step, no second storage backend, and no way for the
//! two to disagree — for S3 and the filesystem alike.
//!
//! The wire format stays the manifest's JSON (that is what the client's
//! `get_tree`/`put_tree` speak); this encoding is how the service *stores*
//! it. [`encode`] and [`decode`] are inverses, and the test at the bottom
//! pins the property the whole scheme rests on: `ContentHash::of(encode(m))
//! == m.package_hash()`.

use std::fmt;

use lpc_history::hash::tree_manifest::TREE_FORMAT_TAG;
use lpc_history::{ContentHash, TreeEntry, TreeManifest};
use lpfs::LpPathBuf;

/// The canonical preimage bytes for a manifest — the exact bytes
/// [`TreeManifest::package_hash`] hashes.
pub fn encode(manifest: &TreeManifest) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(TREE_FORMAT_TAG.as_bytes());
    bytes.push(b'\n');
    for entry in manifest.entries() {
        bytes.extend_from_slice(entry.path.as_str().as_bytes());
        bytes.push(0u8);
        bytes.extend_from_slice(entry.hash.to_string().as_bytes());
        bytes.push(b'\n');
    }
    bytes
}

/// Rebuild a manifest from its canonical preimage.
pub fn decode(bytes: &[u8]) -> Result<TreeManifest, TreePreimageError> {
    let tag = format!("{TREE_FORMAT_TAG}\n");
    let body = bytes
        .strip_prefix(tag.as_bytes())
        .ok_or(TreePreimageError::UnknownFormat)?;

    let mut entries = Vec::new();
    for line in body.split_inclusive(|byte| *byte == b'\n') {
        let line = line
            .strip_suffix(b"\n")
            .ok_or(TreePreimageError::Truncated)?;
        let separator = line
            .iter()
            .position(|byte| *byte == 0u8)
            .ok_or(TreePreimageError::Malformed)?;
        let (path, hash) = line.split_at(separator);
        let path = std::str::from_utf8(path).map_err(|_| TreePreimageError::Malformed)?;
        let hash: ContentHash = std::str::from_utf8(&hash[1..])
            .map_err(|_| TreePreimageError::Malformed)?
            .parse()
            .map_err(|_| TreePreimageError::Malformed)?;
        entries.push(TreeEntry {
            path: LpPathBuf::from(path),
            hash,
        });
    }

    TreeManifest::from_entries(entries).map_err(|_| TreePreimageError::Malformed)
}

/// Why stored bytes could not be read back as a tree.
///
/// Every variant means the same operational thing — the blob at that address
/// is not a tree this build understands — and they are separate only so a
/// log line says which way it was wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreePreimageError {
    /// The bytes do not start with the `lph1` format tag.
    UnknownFormat,
    /// The last line has no terminating newline.
    Truncated,
    /// A line is not `<path> 0x00 <64 hex>`.
    Malformed,
}

impl fmt::Display for TreePreimageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            TreePreimageError::UnknownFormat => "not an lph1 tree preimage",
            TreePreimageError::Truncated => "truncated tree preimage",
            TreePreimageError::Malformed => "malformed tree preimage entry",
        };
        f.write_str(message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The property the whole storage scheme rests on: the bytes we store
    /// hash to the address we store them at. If this ever fails, trees are
    /// being filed where no client will look for them.
    #[test]
    fn the_encoding_hashes_to_the_package_hash() {
        for manifest in [empty(), one_file(), several_files()] {
            assert_eq!(
                ContentHash::of(&encode(&manifest)),
                manifest.package_hash(),
                "for {manifest:?}"
            );
        }
    }

    #[test]
    fn decode_inverts_encode() {
        for manifest in [empty(), one_file(), several_files()] {
            assert_eq!(decode(&encode(&manifest)).unwrap(), manifest);
        }
    }

    /// The empty package is a real case (a project with no files yet), and
    /// `lpc-history` pins its hash to the bare tag.
    #[test]
    fn the_empty_tree_is_the_tag_alone() {
        assert_eq!(encode(&empty()), b"lph1\n");
    }

    #[test]
    fn junk_is_refused_rather_than_guessed_at() {
        assert_eq!(decode(b""), Err(TreePreimageError::UnknownFormat));
        assert_eq!(decode(b"lph2\n"), Err(TreePreimageError::UnknownFormat));
        assert_eq!(
            decode(b"lph1\n/a\0beef\n"),
            Err(TreePreimageError::Malformed)
        );
        assert_eq!(decode(b"lph1\n/a"), Err(TreePreimageError::Truncated));
    }

    fn empty() -> TreeManifest {
        TreeManifest::from_entries(vec![]).unwrap()
    }

    fn one_file() -> TreeManifest {
        TreeManifest::from_entries(vec![entry("/project.json", b"{}")]).unwrap()
    }

    fn several_files() -> TreeManifest {
        TreeManifest::from_entries(vec![
            entry("/shader.glsl", b"void main() {}"),
            entry("/project.json", b"{}"),
            entry("/assets/deep/nested name.png", b"\x89PNG"),
        ])
        .unwrap()
    }

    fn entry(path: &str, data: &[u8]) -> TreeEntry {
        TreeEntry {
            path: LpPathBuf::from(path),
            hash: ContentHash::of(data),
        }
    }
}
