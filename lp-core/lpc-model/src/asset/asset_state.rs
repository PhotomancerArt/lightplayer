//! Effective state for a referenced project asset.

use alloc::string::String;

/// Whether an available asset body comes from committed artifacts or overlay.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetBodySource {
    Committed,
    Inline,
    OverlayReplace,
}

/// Effective state for a referenced project asset.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum AssetState {
    Available { source: AssetBodySource },
    NotFound,
    Deleted,
    ReadError { message: String },
}

impl AssetState {
    pub fn is_available(&self) -> bool {
        matches!(self, Self::Available { .. })
    }
}
