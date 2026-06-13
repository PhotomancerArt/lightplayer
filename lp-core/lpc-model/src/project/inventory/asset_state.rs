//! Effective state for a referenced project asset.
//!
//! The registry derives this state by combining referenced assets, artifact
//! availability, and pending overlay edits. It is inventory state, not the asset
//! body itself.

use alloc::string::String;

/// Whether an available asset body comes from committed artifacts or overlay.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetBodyOrigin {
    /// Body comes from committed artifact storage.
    Committed,
    /// Body is embedded inside the owning node definition.
    Inline,
    /// Body is supplied by a pending overlay replacement.
    OverlayReplace,
}

/// Effective state for a referenced project asset.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetState {
    /// The asset body can be materialized from the indicated source.
    Available { origin: AssetBodyOrigin },
    /// The referenced artifact does not exist.
    NotFound,
    /// The referenced artifact has been deleted or is pending deletion.
    Deleted,
    /// The registry attempted to read or interpret the asset and failed.
    ReadError { message: String },
}

impl AssetState {
    pub fn is_available(&self) -> bool {
        matches!(self, Self::Available { .. })
    }
}
