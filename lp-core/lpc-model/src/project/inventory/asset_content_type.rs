/// Coarse specialization for a referenced project asset.
///
/// Asset content type lets registry and engine code choose materialization and
/// validation paths without making the asset identity itself shader-, fixture-,
/// or image-specific.
#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum AssetContentType {
    /// GLSL source consumed by a visual shader node.
    ShaderSource,
    /// GLSL source consumed by a compute shader node.
    ComputeShaderSource,
    /// 2D mapping document (`*.map2d.json`) consumed by a fixture node.
    FixtureMap2d,
    /// Patch document (`*.patch.json`) consumed by a fixture node.
    ///
    /// Its own content type rather than a second `FixtureMap2d`: assets are
    /// looked up one-per-(node, content type), and a fixture references both
    /// of its documents at once.
    FixturePatch,
    /// Image data; decoding details are future work.
    Image,
    /// Generic UTF-8 text.
    Text,
    /// Generic binary data.
    Binary,
}
