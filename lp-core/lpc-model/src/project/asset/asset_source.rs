//! Project asset identity.

use alloc::string::String;

use crate::{ArtifactLocation, NodeDefLocation, SlotPath};

/// Identity for a project asset referenced by the effective project graph.
#[derive(
    Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AssetSource {
    Artifact {
        location: ArtifactLocation,
    },
    Inline {
        owner: NodeDefLocation,
        path: SlotPath,
    },
    Url {
        url: String,
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
