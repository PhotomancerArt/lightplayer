use crate::{ArtifactLocation, AssetBodyOverlay, SlotEdit, SlotPath};

/// One ordered mutation to the canonical project overlay.
///
/// A mutation operation is the command payload without client correlation id or
/// result status. It is intentionally close to [`crate::ProjectOverlay`]'s CRUD
/// operations.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case", tag = "op")]
pub enum MutationOp {
    /// Add or replace one pending slot edit for an artifact.
    PutSlotEdit {
        /// Artifact whose slot overlay should be changed.
        artifact: ArtifactLocation,
        /// Slot edit to insert into the artifact overlay.
        edit: SlotEdit,
    },
    /// Remove one pending slot edit from an artifact.
    RemoveSlotEdit {
        /// Artifact whose slot overlay should be changed.
        artifact: ArtifactLocation,
        /// Slot path whose pending edit should be removed.
        path: SlotPath,
    },
    /// Set whole-body pending intent for one artifact.
    SetArtifactBody {
        /// Artifact whose body should be replaced or deleted.
        artifact: ArtifactLocation,
        /// Whole-body asset edit.
        edit: AssetBodyOverlay,
    },
    /// Remove all pending intent for one artifact.
    ClearArtifact {
        /// Artifact to clear from the overlay.
        artifact: ArtifactLocation,
    },
    /// Remove all pending intent from the overlay.
    Clear,
}
