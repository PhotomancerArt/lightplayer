//! Wire-facing pending edit model.

mod artifact_overlay;
mod slot_edit;

pub use artifact_overlay::{ArtifactEdits, ArtifactOverlay, AssetEdit};
pub use slot_edit::SlotEdit;

#[deprecated(note = "renamed to ArtifactOverlay")]
pub type ChangeOverlay = ArtifactOverlay;
