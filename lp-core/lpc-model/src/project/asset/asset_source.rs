use alloc::string::String;

use crate::{ArtifactLocation, NodeDefLocation, SlotPath};

/// Identity for a project asset referenced by the effective project graph.
///
/// `AssetSource` identifies an asset independently of how the registry later
/// materializes it. Artifact-backed assets point at files or other artifact
/// locations; inline assets point back into the owning node definition; URL
/// assets are represented but not yet loadable.
#[derive(
    Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AssetSource {
    /// Asset body lives in a project artifact.
    Artifact {
        /// Artifact location containing the asset body.
        location: ArtifactLocation,
    },
    /// Asset body is embedded inside an effective node definition.
    Inline {
        /// Node definition that owns the inline asset body.
        owner: NodeDefLocation,
        /// Slot path of the inline asset field within `owner`.
        path: SlotPath,
    },
    /// External URL asset. Represented for model completeness; loading is future work.
    Url {
        /// URL string as authored by the project.
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
