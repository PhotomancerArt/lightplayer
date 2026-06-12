/// Coarse specialization for a referenced project asset.
///
/// Asset kind lets registry and engine code choose materialization and
/// validation paths without making the asset identity itself shader-, fixture-,
/// or image-specific.
#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum AssetKind {
    // TODO-Assets: it doesn't seem right that AssetKinds should be this specific.
    //              what purpose does that serve? It seems we may not want this at all
    //              but instead rely on the slot metadata once we have AssetSlot?

    /// GLSL source consumed by a visual shader node.
    ShaderSource,
    /// GLSL source consumed by a compute shader node.
    ComputeShaderSource,
    /// SVG path mapping consumed by a fixture node.
    FixtureSvg,
    /// Image data; decoding details are future work.
    Image,
    /// Generic UTF-8 text.
    Text,
    /// Generic binary data.
    Binary,
}
