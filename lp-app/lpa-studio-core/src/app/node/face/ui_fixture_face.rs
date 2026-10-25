//! The fixture card's permanent face.

use crate::{
    ProjectSlotAddress, UiAssetEditor, UiFixturePower, UiPanelControl, UiProducedProduct,
    UiSpaceSection,
};

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
    /// The patch document's plumbing (`{stem}.patch.json`), when the
    /// fixture authors one — the patch surface's write target (P6).
    pub patch_editor: Option<UiAssetEditor>,
    /// Estimated draw against the declared supply budget. `None` when the
    /// fixture declares no budget, in which case nothing is ever limited and
    /// there is nothing worth saying on the face.
    pub power: Option<UiFixturePower>,
    /// The consumer half of the two-sided space model (D13/D14): this
    /// fixture's `consume` policy and its `strip_order_meaningful` bit.
    /// The SAME DTO the shader face carries, so the mirror is a data-level
    /// fact rather than two components that happen to look alike. `None`
    /// when the backing rows are absent.
    pub space: Option<UiSpaceSection>,
    /// The Shape declaration moment's slot targets (D13, plan-B P5) —
    /// what the guided preset tiles WRITE. `None` when the backing rows
    /// are absent (a hand-built face, or a def predating the slots); the
    /// guided state then renders nothing rather than inventing targets.
    pub shape_presets: Option<UiShapePresets>,
    /// The patch bay's fixture side (D34a): this fixture's own runs, laid
    /// along ITS channel space and labelled with where each landed on the
    /// wire. The SAME cells the driven output's face shows — one model,
    /// two orderings.
    ///
    /// Filled by the project controller's `apply_patch_bays` pass, from the
    /// output-frame probe. `None` when nothing this fixture produces has
    /// reached an output's wire yet.
    pub patch: Option<crate::UiFixturePatch>,
}

/// The slot addresses the fixture Shape presets write — **the
/// preset→slot-writes seam** (deliberately narrow, per the P5 amendment):
/// a preset is nothing but a batch of ordinary `EnsurePresent`/`SetValue`
/// ops at these addresses (the same ops the advanced-drawer rows and the
/// dimensionality section dispatch — no parallel write path), built by
/// the web's `shape_preset_actions`.
///
/// **D15 retrofit note (mapping-patching vision):** when the explicit
/// fixture declared space lands, presets start writing it by adding its
/// row's address HERE and its write to `shape_preset_actions` — nothing
/// else about the moment changes. Keep every preset-authored slot target
/// on this struct so that stays a two-place change.
#[derive(Clone, Debug, PartialEq)]
pub struct UiShapePresets {
    /// The `mapping` enum row (`MappingConfig`): Strip/Matrix ensure
    /// `Unset` (their shape is the render area, not a map).
    pub mapping: Option<ProjectSlotAddress>,
    /// The `render_size` row (`Dim2u`): Strip writes a 1-row area (the
    /// declared-strip idiom `fixture_carries_2d_coords` reads), Matrix a
    /// square one.
    pub render_size: Option<ProjectSlotAddress>,
    /// The `strip_order_meaningful` row: Strip writes `true` (along the
    /// wire), Matrix `false` (the area is the real layout).
    pub strip_order: Option<ProjectSlotAddress>,
    /// Whether the mapping currently resolves to a `Map2d` document —
    /// what makes the "Mapped shape" tile a live entry into the mapping
    /// editor rather than a disabled hint.
    pub has_map2d: bool,
}
