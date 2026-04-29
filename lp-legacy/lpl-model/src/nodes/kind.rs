/// Node kind - matches directory suffixes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum NodeKind {
    Texture,
    Shader,
    Output,
    Fixture,
}
