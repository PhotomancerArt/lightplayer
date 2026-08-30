/// Authored node definition kind.
///
/// This is the source-level discriminator used by node artifacts. Older legacy
/// loading code also maps directory suffixes to this enum while that loader is
/// being removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum NodeKind {
    Module,
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
        NodeKind::Module,
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

    /// Whether this kind's runtime publishes a renderable
    /// [`crate::VisualProduct`] — the contract a playlist entry child must
    /// meet, since a playlist selects and blends its entries' visual
    /// outputs into its own (`PlaylistState.output`).
    ///
    /// Shader, fluid, and playlist state each carry a produced visual
    /// output slot; a module mirrors its scope's `visual.out`. Texture is a
    /// resource shaders sample, not a renderable product, and the compute
    /// shader publishes artifact-shaped data slots — neither can be watched
    /// on its own. Wildcard-free so a new kind must be placed.
    pub const fn produces_visual(self) -> bool {
        match self {
            NodeKind::Module | NodeKind::Shader | NodeKind::Fluid | NodeKind::Playlist => true,
            NodeKind::Button
            | NodeKind::Clock
            | NodeKind::Texture
            | NodeKind::ComputeShader
            | NodeKind::ControlRadio
            | NodeKind::Output
            | NodeKind::Fixture => false,
        }
    }
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
                NodeKind::Module => 0,
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

    /// The visual set is exactly the kinds whose runtime publishes a
    /// `VisualProduct` (shader/fluid/playlist state, the module mirror).
    #[test]
    fn visual_kinds_are_the_watchable_four() {
        let visual: alloc::vec::Vec<NodeKind> = NodeKind::ALL
            .iter()
            .copied()
            .filter(|kind| kind.produces_visual())
            .collect();
        assert_eq!(
            visual,
            [
                NodeKind::Module,
                NodeKind::Shader,
                NodeKind::Fluid,
                NodeKind::Playlist,
            ]
        );
    }
}
