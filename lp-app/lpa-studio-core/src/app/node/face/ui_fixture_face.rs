//! The fixture card's permanent face.

use crate::{UiAssetEditor, UiFixturePower, UiPanelControl, UiProducedProduct};

/// Permanent face for a fixture node card.
///
/// Renders the lit preview (LED sample points, not the shader texture) with
/// the dominant horizontal brightness fader below. When the fixture's
/// mapping is a `Map2d` document, the output display doubles as the entry
/// to the in-place mapping editor ("one home"): `mapping_editor` carries
/// the asset-pipeline plumbing the web editor syncs through.
#[derive(Clone, Debug, PartialEq)]
pub struct UiFixtureFace {
    /// The fixture's produced control output, rendered as the lit preview.
    pub preview: UiProducedProduct,
    /// The dominant brightness fader, bound to `FixtureDef.brightness.some`
    /// (0–255) through the standard slot write path.
    pub brightness: UiPanelControl,
    /// The mapping document's inline-editor plumbing (fetch/apply/revert
    /// targets), present when the mapping slot resolves to a `Map2d` asset.
    pub mapping_editor: Option<UiAssetEditor>,
    /// Estimated draw against the declared supply budget. `None` when the
    /// fixture declares no budget, in which case nothing is ever limited and
    /// there is nothing worth saying on the face.
    pub power: Option<UiFixturePower>,
}
