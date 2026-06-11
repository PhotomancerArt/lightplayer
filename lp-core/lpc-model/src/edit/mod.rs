//! Shared authored project edit vocabulary.

pub mod artifact_body_edit;
pub mod artifact_edit;
pub mod definition_location;
pub mod project_edit;
pub mod slot_edit;

pub use artifact_body_edit::ArtifactBodyEdit;
pub use artifact_edit::{ArtifactEdit, ArtifactEditOp};
pub use definition_location::DefinitionLocation;
pub use project_edit::{
    ProjectCommitSummary, ProjectDefChangeDetail, ProjectDefUpdates, ProjectEditBatch,
    ProjectEditBatchResult, ProjectEditCommand, ProjectEditCommandId, ProjectEditCommandResult,
    ProjectEditCommandStatus, ProjectEditEffect, ProjectEditOp, ProjectEditRejection,
    ProjectEditRejectionReason,
};
pub use slot_edit::SlotEdit;
