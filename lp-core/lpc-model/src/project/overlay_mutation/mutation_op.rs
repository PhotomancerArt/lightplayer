use crate::{ArtifactLocation, AssetBodyOverlay, SlotEdit, SlotPath};

/// One ordered mutation to the canonical project overlay.
///
/// A mutation operation is the command payload without client correlation id or
/// result status. It is intentionally close to [`crate::ProjectOverlay`]'s CRUD
/// operations.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
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
    /// Move one map entry to a new key within the same map.
    ///
    /// `from` and `to` are sibling map-entry paths (same parent map path,
    /// terminal key segments). Keys are path segments, so a key change is a
    /// distinct mutation, not a value edit: the registry materializes it into
    /// per-path overlay edits that reconstruct the *effective* value at
    /// `from` under `to` (`EnsurePresent` at `to` plus leaf assignments and
    /// structural selections where the moved value diverges from a fresh
    /// entry's factory defaults) and remove `from`. The per-command ack
    /// reports the stored edits via [`crate::MutationEffect::Materialized`];
    /// this op is never applied to a [`crate::ProjectOverlay`] directly.
    MoveSlotEntry {
        /// Artifact whose slot overlay should be changed.
        artifact: ArtifactLocation,
        /// Map-entry path to move from (must be present in the effective
        /// definition).
        from: SlotPath,
        /// Map-entry path to move to (must be absent in the effective
        /// definition).
        to: SlotPath,
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
