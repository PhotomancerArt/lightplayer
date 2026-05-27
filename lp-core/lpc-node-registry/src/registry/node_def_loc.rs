//! Address of a parsed node definition within the artifact store.

use lpc_model::SlotPath;

use crate::ArtifactLoc;

/// Definition location for a registry entry.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NodeDefLoc {
    /// Artifact where the node is defined.
    pub artifact: ArtifactLoc,
    /// Path in the artifact.
    pub path: SlotPath,
}

impl NodeDefLoc {
    pub fn artifact_root(artifact: ArtifactLoc) -> Self {
        Self {
            artifact,
            path: SlotPath::root(),
        }
    }
}
