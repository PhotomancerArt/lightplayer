//! The fixture card's permanent face.

use crate::{UiPanelControl, UiProducedProduct};

/// Permanent face for a fixture node card.
///
/// Renders the lit preview (LED sample points, not the shader texture) with
/// the dominant horizontal brightness fader below; the mapping editor is a
/// later custom drawer.
#[derive(Clone, Debug, PartialEq)]
pub struct UiFixtureFace {
    /// The fixture's produced control output, rendered as the lit preview.
    pub preview: UiProducedProduct,
    /// The dominant brightness fader, bound to `FixtureDef.brightness.some`
    /// (0–255) through the standard slot write path.
    pub brightness: UiPanelControl,
}
