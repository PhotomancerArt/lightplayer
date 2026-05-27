//! Compatibility facade for pending edit model and apply helpers.

pub use crate::edit_apply::{EditError, serialize_slot_draft};
#[allow(deprecated, reason = "legacy overlay alias")]
pub use crate::edit_model::ChangeOverlay;
pub use crate::edit_model::{ArtifactEdits, ArtifactOverlay, AssetEdit, SlotEdit};
pub use crate::registry::CommitError;
pub use crate::registry::path_validation::require_absolute_path;
