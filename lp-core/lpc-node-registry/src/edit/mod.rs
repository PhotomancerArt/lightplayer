//! Registry edit apply helpers and public edit vocabulary.

pub use crate::edit_apply::{EditError, serialize_slot_draft};
pub use crate::registry::CommitError;
pub use crate::registry::path_validation::require_absolute_path;
pub use lpc_model::{
    ArtifactBodyEdit, ArtifactOverlay, ProjectOverlay, SlotEdit, SlotEditOp, SlotOverlay,
};
