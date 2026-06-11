//! Pending edit model compatibility exports.

mod artifact_overlay;
mod slot_edit;

pub use artifact_overlay::{ArtifactEdits, ArtifactOverlay, AssetEdit};
pub use lpc_model::edit::ArtifactBodyEdit;
pub use slot_edit::SlotEdit;

#[deprecated(note = "renamed to ArtifactOverlay")]
pub type ChangeOverlay = ArtifactOverlay;
