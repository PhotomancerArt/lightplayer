//! Top-level client edit batch.
//!
//! An [`EditBatch`] is an ordered, id'd list of [`super::ArtifactEdit`] blocks.
//! Apply via [`crate::NodeDefRegistry::apply_edit_batch`]; commit or discard the
//! resulting slot overlay separately.

use alloc::vec::Vec;

use super::ArtifactEdit;

/// Stable identifier for a client edit batch (wire / replay).
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct EditBatchId(pub u64);

/// Ordered client edits grouped by artifact.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EditBatch {
    pub id: EditBatchId,
    #[serde(alias = "changes")]
    pub edits: Vec<ArtifactEdit>,
}

impl EditBatch {
    pub fn new(id: EditBatchId, edits: Vec<ArtifactEdit>) -> Self {
        Self { id, edits }
    }
}
