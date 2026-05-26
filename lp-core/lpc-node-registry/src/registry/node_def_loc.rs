//! Address of a parsed node definition within the artifact store.

use lpc_model::SlotPath;

use crate::ArtifactId;

/// Source location for a registry entry.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NodeDefLoc {
    /// Artifact where the node is defined
    pub artifact_id: ArtifactId,

    /// Path in the artifact
    pub path: SlotPath,
}

impl NodeDefLoc {
    pub fn artifact_root(artifact_id: ArtifactId) -> Self {
        Self {
            artifact_id,
            path: SlotPath::root(),
        }
    }
}
