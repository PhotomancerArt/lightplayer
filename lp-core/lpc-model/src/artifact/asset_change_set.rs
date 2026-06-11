//! Effective asset inventory changes.

use alloc::vec::Vec;

use crate::ArtifactLocation;

/// Effective asset changes visible to runtime/project consumers.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AssetChangeSet {
    pub added: Vec<ArtifactLocation>,
    pub changed: Vec<AssetChange>,
    pub removed: Vec<ArtifactLocation>,
}

impl AssetChangeSet {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.changed.is_empty() && self.removed.is_empty()
    }
}

/// One changed asset and its coarse runtime-facing classification.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AssetChange {
    pub location: ArtifactLocation,
    pub kind: AssetChangeKind,
}

impl AssetChange {
    pub fn new(location: ArtifactLocation, kind: AssetChangeKind) -> Self {
        Self { location, kind }
    }
}

/// Runtime-facing asset change classification.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetChangeKind {
    Body,
    EnteredError,
    LeftError,
}
