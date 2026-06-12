use crate::{ArtifactLocation, AssetOverlay, SlotEdit, SlotPath};

/// One ordered mutation to the canonical project overlay.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case", tag = "op")]
pub enum MutationOp {
    PutSlotEdit {
        artifact: ArtifactLocation,
        edit: SlotEdit,
    },
    RemoveSlotEdit {
        artifact: ArtifactLocation,
        path: SlotPath,
    },
    SetArtifactBody {
        artifact: ArtifactLocation,
        edit: AssetOverlay,
    },
    ClearArtifact {
        artifact: ArtifactLocation,
    },
    Clear,
}