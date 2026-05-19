/// Authored node definition kind.
///
/// This is the source-level discriminator used by node artifacts. Older legacy
/// loading code also maps directory suffixes to this enum while that loader is
/// being removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum NodeKind {
    Project,
    Clock,
    Texture,
    Shader,
    ComputeShader,
    Fluid,
    Output,
    Fixture,
}
