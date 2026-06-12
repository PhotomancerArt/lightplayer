//! Byte-level edits for one artifact body.

use alloc::vec::Vec;

/// Replace or delete an artifact body.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetOverlay {
    Delete,
    ReplaceBody(Vec<u8>),
}
