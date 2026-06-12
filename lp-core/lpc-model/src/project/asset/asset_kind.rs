//! Project asset specialization.

/// Coarse kind for a referenced project asset.
#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum AssetKind {
    ShaderSource,
    ComputeShaderSource,
    FixtureSvg,
    Image,
    Text,
    Binary,
}
