//! Address of a parsed node definition within an artifact.

use crate::{ArtifactLocation, SlotPath};

/// Location of a node definition within an authored artifact.
#[derive(
    Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
pub struct NodeDefLocation {
    pub artifact: ArtifactLocation,
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
