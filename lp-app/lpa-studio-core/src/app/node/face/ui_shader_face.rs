//! The shader card's permanent face.

use crate::{UiAgentView, UiAssetEditor, UiPanelControl, UiProducedProduct, UiSpaceSection};

/// Permanent face for a shader node card.
///
/// Renders top-down as: preview → (perf line, separate run of work) → knob
/// row → agent chat; the code drawer (CodeMirror asset editor) and the
/// advanced drawer (full slot view) expand beneath.
#[derive(Clone, Debug, PartialEq)]
pub struct UiShaderFace {
    /// The shader's produced visual output, rendered as the face hero.
    pub preview: UiProducedProduct,
    /// Panel controls projected from uniform slots bound to a bus channel
    /// (Q13: binding is publicity).
    pub controls: Vec<UiPanelControl>,
    /// The shader's agent chat handle, as the shader editor decoration
    /// carries today ([`UiAssetEditor::agent`]). `None` when no provider is
    /// configured or in project-controller unit contexts.
    pub agent: Option<UiAgentView>,
    /// The code drawer's inline GLSL editor. `None` until the def's source
    /// asset resolves to an editable artifact.
    pub code_drawer: Option<UiAssetEditor>,
    /// The producer half of the two-sided space model (D13): what this
    /// shader declares it renders in, and how it answers the other
    /// dimension. Mirrors [`crate::UiFixtureFace::space`] by construction —
    /// same DTO, other side. `None` when the `space` slot row is absent
    /// (hand-built fixtures, pre-slot projections), in which case the card
    /// renders no section and the rows stay in the advanced drawer.
    pub space: Option<UiSpaceSection>,
}
