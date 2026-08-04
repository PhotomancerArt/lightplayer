//! The shader card's permanent face.

use crate::{UiAgentView, UiAssetEditor, UiPanelControl, UiProducedProduct};

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
}
