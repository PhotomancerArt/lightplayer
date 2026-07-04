use crate::ArtifactLocation;

/// Identity for a project asset referenced by the effective project graph.
///
/// `AssetLocation` identifies an asset independently of how the registry later
/// materializes it. Asset bodies always live in separate artifact files —
/// remote or otherwise external asset bodies should be modeled as artifact
/// locations too, not as a separate asset-location kind.
#[derive(
    Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum AssetLocation {
    /// Asset body lives in a project artifact.
    Artifact {
        /// Artifact location containing the asset body.
        location: ArtifactLocation,
    },
}

impl AssetLocation {
    pub fn artifact(location: ArtifactLocation) -> Self {
        Self::Artifact { location }
    }
}
