//! Overlay domain model and apply helpers.

mod artifact_overlay;
mod commit_error;
mod edit_error;
mod path_validation;
mod slot_edit;

pub use artifact_overlay::{ArtifactEdits, ArtifactOverlay, AssetEdit};
pub use commit_error::CommitError;
pub use edit_error::EditError;
pub use path_validation::require_absolute_path;
pub use slot_edit::SlotEdit;

#[deprecated(note = "renamed to ArtifactOverlay")]
pub type ChangeOverlay = ArtifactOverlay;
