use crate::{ArtifactLocation, SlotPath};

/// Location of a node definition within a project.
///
/// A node definition location identifies definition data, not a runtime node
/// instance. Multiple [`crate::ProjectNode`] occurrences can point at the same
/// `NodeDefLocation` when a definition artifact is referenced more than once.
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
