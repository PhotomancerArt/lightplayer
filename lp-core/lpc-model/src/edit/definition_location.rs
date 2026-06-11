//! Portable location of a node definition within an artifact.

use crate::{LpPathBuf, SlotPath};

/// File-backed definition location suitable for edit results and wire summaries.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DefinitionLocation {
    pub artifact_path: LpPathBuf,
    pub path: SlotPath,
}

impl DefinitionLocation {
    pub fn new(artifact_path: LpPathBuf, path: SlotPath) -> Self {
        Self {
            artifact_path,
            path,
        }
    }
}
