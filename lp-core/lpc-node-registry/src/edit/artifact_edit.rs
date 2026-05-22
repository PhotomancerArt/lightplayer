//! One artifact block in an [`super::EditBatch`].

use alloc::vec::Vec;

use super::{AssetEdit, EditTarget, SlotEdit};

/// Edits targeting a single artifact path or id.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ArtifactEdit {
    /// Structured slot edits on a `.toml` artifact.
    Slot {
        target: EditTarget,
        ops: Vec<SlotEdit>,
    },
    /// Opaque file-body edits (assets, delete, TOML import escape hatch).
    Asset {
        target: EditTarget,
        ops: Vec<AssetEdit>,
    },
}

impl ArtifactEdit {
    pub fn target(&self) -> &EditTarget {
        match self {
            Self::Slot { target, .. } | Self::Asset { target, .. } => target,
        }
    }

    pub fn slot(target: EditTarget, ops: Vec<SlotEdit>) -> Self {
        Self::Slot { target, ops }
    }

    pub fn asset(target: EditTarget, ops: Vec<AssetEdit>) -> Self {
        Self::Asset { target, ops }
    }
}
