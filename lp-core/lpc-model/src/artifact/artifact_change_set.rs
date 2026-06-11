//! Persistence-level artifact changes.

use alloc::vec::Vec;

use crate::ArtifactLocation;

/// Artifact writes/deletes performed against durable storage.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ArtifactChangeSet {
    pub added: Vec<ArtifactLocation>,
    pub changed: Vec<ArtifactLocation>,
    pub removed: Vec<ArtifactLocation>,
}

impl ArtifactChangeSet {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.changed.is_empty() && self.removed.is_empty()
    }
}
