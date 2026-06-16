//! Effective asset inventory changes.
//!
//! These changes are derived by comparing two effective asset inventories. They
//! tell consumers which asset identities entered, left, or changed state.

use crate::{AssetLocation, ChangeSummary};

/// Effective asset changes visible to runtime/project consumers.
pub type AssetChangeSummary = ChangeSummary<AssetLocation, AssetChange>;

/// One changed asset and its coarse runtime-facing classification.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AssetChange {
    /// Changed asset identity.
    pub location: AssetLocation,
    /// Coarse classification of the change.
    pub kind: AssetChangeKind,
}

impl AssetChange {
    pub fn new(location: AssetLocation, kind: AssetChangeKind) -> Self {
        Self { location, kind }
    }
}

/// Runtime-facing asset change classification.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetChangeKind {
    /// Asset body changed while remaining available.
    Body,
    /// Asset moved from available state into an error state.
    EnteredError,
    /// Asset moved from an error state into available state.
    LeftError,
}
