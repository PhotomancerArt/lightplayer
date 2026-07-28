//! The effect card's permanent face (effects-are-projects ADR).

use crate::{UiPanelControl, UiProducedProduct};

/// Permanent face for an embedded project ("effect") node card.
///
/// Renders top-down as: output-mirror preview → promoted-control knob row →
/// provenance line; the advanced drawer keeps the full slot view. Promotion
/// is DTO-level aliasing: each control's `address` is the **inner child's**
/// slot address, so edits, dirty state, and bound-violet ride the standard
/// slot machinery with no forwarding layer.
#[derive(Clone, Debug, PartialEq)]
pub struct UiEffectFace {
    /// The project node's produced `output` mirror, rendered as the face
    /// hero (the scope's `visual.out`, forwarded).
    pub preview: UiProducedProduct,
    /// Promoted controls aliased onto inner-child slots. A control whose
    /// target does not resolve renders disabled with an Invalid affordance
    /// (never dropped silently).
    pub controls: Vec<UiPanelControl>,
    /// Compact provenance line ("author · v1 · CC0-1.0"); `None` when the
    /// def carries no provenance fields.
    pub provenance: Option<String>,
}
