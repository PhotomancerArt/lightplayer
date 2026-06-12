use crate::{ArtifactLocation, NodeDefLocation, SlotPath};

/// Identity for a project asset referenced by the effective project graph.
///
/// `AssetSource` identifies an asset independently of how the registry later
/// materializes it. Artifact-backed assets point at artifact locations, and
/// inline assets point back into the owning node definition. Remote or otherwise
/// external asset bodies should be modeled as artifact locations, not as a
/// separate asset-source kind.
#[derive(
    Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AssetSource {
    // TODO-Assets: I'm not convinced this is the right name. This might be AssetUse or similar?
    //              it should probably be part of AssetSlot once added.


    /// Asset body lives in a project artifact.
    Artifact {
        /// Artifact location containing the asset body.
        location: ArtifactLocation,
    },
    /// Asset body is embedded inside an effective node definition.
    Inline {
        /// Node definition that owns the inline asset body.
        owner: NodeDefLocation,
        /// Slot path of the inline asset field within `owner`.
        path: SlotPath,
    },
}

impl AssetSource {
    pub fn artifact(location: ArtifactLocation) -> Self {
        Self::Artifact { location }
    }

    pub fn inline(owner: NodeDefLocation, path: SlotPath) -> Self {
        Self::Inline { owner, path }
    }
}
