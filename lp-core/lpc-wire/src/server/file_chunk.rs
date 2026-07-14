//! File-sync chunk types for `FsRequest::ChangesSince` pages.
//!
//! A `ChangesSince` response is **stateless cursor pagination**, not a
//! multi-frame stream: each page is a normal single-frame response packed
//! under the frame budget (see `crate::budget`), and the client requests the
//! next page (`cursor = previous page's next`) only after fully receiving
//! the previous one — which is the pull model's auto-throttling rule by
//! construction, with zero server-side client state. Enumeration order is
//! deterministic (paths sorted bytewise), so a cursor identifies an exact
//! resume point.
//!
//! Files larger than one chunk arrive as ordered `offset`/`total` pieces;
//! deletes arrive as tombstones (`kind: Delete`, `offset: 0`, `total: 0`,
//! empty data). These are new canonical wire forms — no compatibility shims.

use crate::serde_base64;
use alloc::vec::Vec;
use lpc_model::LpPathBuf;
use serde::{Deserialize, Serialize};

/// How a file changed, as seen by a `ChangesSince` read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FileChangeKind {
    /// The file exists with the streamed content (created or modified —
    /// the receiver treats both identically).
    Upsert,
    /// The file was deleted; this entry is a tombstone.
    Delete,
}

/// Resume point within a paginated `ChangesSince` enumeration.
///
/// Identifies the first not-yet-sent byte: the entry for `path` (relative to
/// the requested prefix, bytewise path order) starting at `offset`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileCursor {
    pub path: LpPathBuf,
    pub offset: u32,
}

/// One chunk of one file in a `ChangesSince` page.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileChunk {
    /// Path relative to the requested prefix.
    pub path: LpPathBuf,
    pub kind: FileChangeKind,
    /// Byte offset of this chunk within the file.
    pub offset: u32,
    /// Total file size in bytes (repeated on every chunk of the file).
    pub total: u32,
    #[serde(
        serialize_with = "serde_base64::serialize_smart",
        deserialize_with = "serde_base64::deserialize_smart"
    )]
    pub data: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_model::AsLpPathBuf;

    #[test]
    fn upsert_chunk_round_trips_with_committed_sample() {
        let chunk = FileChunk {
            path: "/shader.glsl".as_path_buf(),
            kind: FileChangeKind::Upsert,
            offset: 0,
            total: 14,
            data: b"void main() {}".to_vec(),
        };
        let json = crate::json::to_string(&chunk).unwrap();
        // committed wire sample — changing this breaks peers; must be deliberate
        assert_eq!(
            json,
            "{\"path\":\"/shader.glsl\",\"kind\":\"upsert\",\"offset\":0,\"total\":14,\"data\":\"void main() {}\"}"
        );
        let back: FileChunk = crate::json::from_str(&json).unwrap();
        assert_eq!(back.data, chunk.data);
    }

    #[test]
    fn binary_chunk_goes_base64() {
        let chunk = FileChunk {
            path: "/blob.bin".as_path_buf(),
            kind: FileChangeKind::Upsert,
            offset: 4096,
            total: 8192,
            data: alloc::vec![0u8, 159, 146, 150, 255],
        };
        let json = crate::json::to_string(&chunk).unwrap();
        let back: FileChunk = crate::json::from_str(&json).unwrap();
        assert_eq!(back.data, chunk.data);
        assert_eq!(back.offset, 4096);
    }

    #[test]
    fn delete_tombstone_round_trips() {
        let chunk = FileChunk {
            path: "/gone.json".as_path_buf(),
            kind: FileChangeKind::Delete,
            offset: 0,
            total: 0,
            data: Vec::new(),
        };
        let json = crate::json::to_string(&chunk).unwrap();
        assert_eq!(
            json,
            "{\"path\":\"/gone.json\",\"kind\":\"delete\",\"offset\":0,\"total\":0,\"data\":\"\"}"
        );
        let back: FileChunk = crate::json::from_str(&json).unwrap();
        assert_eq!(back.kind, FileChangeKind::Delete);
        assert!(back.data.is_empty());
    }
}
