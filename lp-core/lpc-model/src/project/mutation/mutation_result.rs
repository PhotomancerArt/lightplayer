//! Results from applying overlay mutations to an effective project inventory.

use crate::{OverlayMutationBatchResult, ProjectChangeSet, Revision};

/// Ordered command results plus the aggregate effective project change set.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MutationBatchResults {
    pub commands: OverlayMutationBatchResult,
    pub overlay_revision: Revision,
    pub changes: ProjectChangeSet,
}

impl MutationBatchResults {
    pub fn new(
        commands: OverlayMutationBatchResult,
        overlay_revision: Revision,
        changes: ProjectChangeSet,
    ) -> Self {
        Self {
            commands,
            overlay_revision,
            changes,
        }
    }
}


/// Result from applying one or more overlay mutations.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MutationResult {
    pub overlay_revision: Revision,
    pub overlay_changed: bool,
    pub changes: ProjectChangeSet,
}

impl MutationResult {
    pub fn new(
        overlay_revision: Revision,
        overlay_changed: bool,
        changes: ProjectChangeSet,
    ) -> Self {
        Self {
            overlay_revision,
            overlay_changed,
            changes,
        }
    }
}