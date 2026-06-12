use crate::{ProjectChangeSet, Revision};

use super::MutationCmdBatchResult;

/// Result from applying an ordered batch of overlay mutations.
///
/// This carries the per-command acceptance/rejection results plus the aggregate
/// effective project change set produced by the accepted commands.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MutationBatchResults {
    /// Per-command acceptance/rejection results.
    pub commands: MutationCmdBatchResult,
    /// Revision at which the overlay was last changed.
    pub overlay_revision: Revision,
    /// Effective project changes produced by the batch.
    pub changes: ProjectChangeSet,
}

impl MutationBatchResults {
    pub fn new(
        commands: MutationCmdBatchResult,
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
    /// Revision at which the overlay was last changed.
    pub overlay_revision: Revision,
    /// Whether the operation changed canonical overlay state.
    pub overlay_changed: bool,
    /// Effective project changes produced by the operation.
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
