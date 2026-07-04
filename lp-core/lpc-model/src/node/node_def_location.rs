use crate::ArtifactLocation;

/// Location of authored node definition data within project artifacts.
///
/// `NodeDefLocation` identifies the definition payload itself, not a node use
/// in the project tree and not a runtime node. Multiple [`crate::ProjectNode`]
/// uses can point at the same definition when an artifact is referenced more
/// than once. Definitions live strictly one per artifact file, so a location
/// is just the containing artifact.
#[derive(
    Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
pub struct NodeDefLocation {
    /// Artifact containing the node definition.
    pub artifact: ArtifactLocation,
}

impl NodeDefLocation {
    pub fn artifact_root(artifact: ArtifactLocation) -> Self {
        Self { artifact }
    }
}
