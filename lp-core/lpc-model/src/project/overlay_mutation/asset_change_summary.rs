//! Effective asset inventory changes.
//!
//! These changes are derived by comparing two effective asset inventories. They
//! tell consumers which asset identities entered, left, or changed state.

use alloc::vec::Vec;

use crate::AssetSource;

/// Effective asset changes visible to runtime/project consumers.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AssetChangeSummary {
    /// Newly referenced assets.
    pub added: Vec<AssetSource>,
    /// Previously referenced assets whose effective state changed.
    pub changed: Vec<AssetChange>,
    /// Assets that are no longer referenced.
    pub removed: Vec<AssetSource>,
}

impl AssetChangeSummary {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.changed.is_empty() && self.removed.is_empty()
    }
}

/// One changed asset and its coarse runtime-facing classification.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AssetChange {
    /// Changed asset identity.
    pub source: AssetSource,
    /// Coarse classification of the change.
    pub kind: AssetChangeKind,
}

impl AssetChange {
    pub fn new(source: AssetSource, kind: AssetChangeKind) -> Self {
        Self { source, kind }
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
