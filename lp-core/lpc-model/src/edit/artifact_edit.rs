//! Artifact-addressed authored edit commands.

use crate::LpPathBuf;
use crate::edit::{ArtifactBodyEdit, SlotEdit};

/// One edit addressed to an artifact path.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ArtifactEdit {
    pub artifact_path: LpPathBuf,
    pub op: ArtifactEditOp,
}

/// Structured or byte-level edit for one artifact.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum ArtifactEditOp {
    Slot { edit: SlotEdit },
    Body { edit: ArtifactBodyEdit },
}

impl ArtifactEdit {
    pub fn slot(artifact_path: LpPathBuf, edit: SlotEdit) -> Self {
        Self {
            artifact_path,
            op: ArtifactEditOp::Slot { edit },
        }
    }

    pub fn body(artifact_path: LpPathBuf, edit: ArtifactBodyEdit) -> Self {
        Self {
            artifact_path,
            op: ArtifactEditOp::Body { edit },
        }
    }
}
