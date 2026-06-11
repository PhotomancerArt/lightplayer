//! Shared authored project edit vocabulary.

pub mod artifact_body_edit;
pub mod artifact_overlay;
pub mod overlay_mutation;
pub mod project_commit_summary;
pub mod project_overlay;
pub mod slot_edit;
pub mod slot_overlay;

pub use artifact_body_edit::ArtifactBodyEdit;
pub use artifact_overlay::ArtifactOverlay;
pub use overlay_mutation::{
    OverlayMutation, OverlayMutationBatch, OverlayMutationBatchResult, OverlayMutationCommand,
    OverlayMutationCommandId, OverlayMutationCommandResult, OverlayMutationCommandStatus,
    OverlayMutationEffect, OverlayMutationRejection, OverlayMutationRejectionReason,
};
pub use project_commit_summary::ProjectCommitSummary;
pub use project_overlay::ProjectOverlay;
pub use slot_edit::{SlotEdit, SlotEditOp};
pub use slot_overlay::SlotOverlay;
