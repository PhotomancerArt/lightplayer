//! Top-level client edit batch.

use alloc::vec::Vec;

use super::ArtifactChange;

/// Stable identifier for a client edit batch (wire / replay).
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct ChangeSetId(pub u64);

/// Ordered client edits grouped by artifact.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ChangeSet {
    pub id: ChangeSetId,
    pub changes: Vec<ArtifactChange>,
}

impl ChangeSet {
    pub fn new(id: ChangeSetId, changes: Vec<ArtifactChange>) -> Self {
        Self { id, changes }
    }
}
