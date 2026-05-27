//! Overlay domain model and apply helpers.

mod artifact_overlay;
mod artifact_projection;
mod commit_error;
mod edit_error;
mod path_validation;
mod slot_edit;
mod slot_edit_apply;

pub use artifact_overlay::{ArtifactEdits, ArtifactOverlay, AssetEdit};
pub(crate) use artifact_projection::{
    parse_toml_bytes, project_artifact_bytes, project_artifact_def, project_def_at_loc,
    read_error_state,
};
pub use commit_error::CommitError;
pub use edit_error::EditError;
pub use path_validation::require_absolute_path;
pub use slot_edit::SlotEdit;
#[cfg(feature = "diff")]
pub(crate) use slot_edit_apply::apply_ops_to_node_def;
pub use slot_edit_apply::serialize_slot_draft;
pub(crate) use slot_edit_apply::{apply_op_to_def, parse_def_bytes};

#[deprecated(note = "renamed to ArtifactOverlay")]
pub type ChangeOverlay = ArtifactOverlay;
