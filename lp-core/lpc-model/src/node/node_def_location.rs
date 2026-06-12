use crate::{ArtifactLocation, SlotPath};

/// Location of authored node definition data within project artifacts.
///
/// `NodeDefLocation` identifies the definition payload itself, not a node use
/// in the project tree and not a runtime node. Multiple [`crate::ProjectNode`]
/// uses can point at the same definition when an artifact is referenced more
/// than once.
#[derive(
    Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
pub struct NodeDefLocation {
    /// Artifact containing the node definition.
    pub artifact: ArtifactLocation,
    /// Slot path of the definition inside the artifact.
    ///
    /// Artifact-root definitions use [`SlotPath::root`]. Inline child
    /// definitions use the parent-owned invocation slot path.
    pub path: SlotPath,
}

impl NodeDefLocation {
    pub fn artifact_root(artifact: ArtifactLocation) -> Self {
        Self {
            artifact,
            path: SlotPath::root(),
        }
    }
}
