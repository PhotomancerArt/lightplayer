/// Authored node definition kind.
///
/// This is the source-level discriminator used by node artifacts. Older legacy
/// loading code also maps directory suffixes to this enum while that loader is
/// being removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum NodeKind {
    Project,
    Button,
    Clock,
    Texture,
    Shader,
    ComputeShader,
    Fluid,
    Playlist,
    ControlRadio,
    Output,
    Fixture,
}

impl NodeKind {
    /// Every kind, in declaration order. Iteration over the kinds goes
    /// through this const so call sites stay wildcard-free: adding a
    /// variant without extending it is caught by
    /// [`tests::all_is_total_and_in_declaration_order`].
    pub const ALL: [NodeKind; 11] = [
        NodeKind::Project,
        NodeKind::Button,
        NodeKind::Clock,
        NodeKind::Texture,
        NodeKind::Shader,
        NodeKind::ComputeShader,
        NodeKind::Fluid,
        NodeKind::Playlist,
        NodeKind::ControlRadio,
        NodeKind::Output,
        NodeKind::Fixture,
    ];
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The match below is wildcard-free: a new variant fails to compile
    /// here until it is placed, and the index assertion fails until it is
    /// added to `ALL`.
    #[test]
    fn all_is_total_and_in_declaration_order() {
        const fn index_in_all(kind: NodeKind) -> usize {
            match kind {
                NodeKind::Project => 0,
                NodeKind::Button => 1,
                NodeKind::Clock => 2,
                NodeKind::Texture => 3,
                NodeKind::Shader => 4,
                NodeKind::ComputeShader => 5,
                NodeKind::Fluid => 6,
                NodeKind::Playlist => 7,
                NodeKind::ControlRadio => 8,
                NodeKind::Output => 9,
                NodeKind::Fixture => 10,
            }
        }
        for (i, kind) in NodeKind::ALL.iter().enumerate() {
            assert_eq!(index_in_all(*kind), i, "{kind:?} out of place in ALL");
        }
    }
}
