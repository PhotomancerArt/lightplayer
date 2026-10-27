//! The fixture's frame plan: what its mapping says about its lamps, derived
//! once per mapping instead of three times a frame.

use alloc::vec::Vec;

use lpc_model::{ColorOrder, ControlExtent, Revision};

use crate::frame_plan::PlannedRead;
use crate::products::control::ControlSpan;

/// What a fixture's produce and render used to rebuild from its mapping every
/// frame — the path spans walked for the lamp count (in produce, for the
/// control extent), again for the control layout (in render), and a third
/// time for the power estimate when a budget is set. Built the first frame
/// after its key moves; the per-lamp work (coordinates, sampling, the lamp
/// writes) stays per frame.
///
/// Held in a [`crate::frame_plan::PlanSlot`] keyed on [`FixturePlanKey`].
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FixtureFramePlan {
    /// One past the highest lamp any path reaches.
    pub(crate) lamp_channel_count: u32,
    /// The control product's extent: one row of `lamp_channel_count` RGB
    /// lamps.
    pub(crate) control_extent: ControlExtent,
    /// The control layout of a render that wrote every lamp
    /// (`lamp_channel_count × 3` samples), in [`FixturePlanKey`]'s colour
    /// order. A render that wrote fewer builds its own.
    pub(crate) full_spans: Vec<ControlSpan>,
    /// The colour order [`Self::full_spans`] carry.
    pub(crate) color_order: ColorOrder,
}

/// What can move a fixture's plan: a structural edit, the mapping (every
/// mapping change stamps `mapping_version`), and the colour order its spans
/// are reported in (a per-frame settings read, structural in practice).
pub(crate) type FixturePlanKey = (u64, Revision, ColorOrder);

/// The fixture's per-frame settings reads, each planned to its interned
/// query id ([`PlannedRead`]).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct FixtureReads {
    pub(crate) render_size: PlannedRead,
    pub(crate) color_order: PlannedRead,
    pub(crate) brightness: PlannedRead,
    pub(crate) gamma_correction: PlannedRead,
    pub(crate) power: PlannedRead,
    pub(crate) diagnostic_mode: PlannedRead,
    pub(crate) selection: PlannedRead,
    pub(crate) strip_order_meaningful: PlannedRead,
    pub(crate) wire_reversed: PlannedRead,
    pub(crate) consume: PlannedRead,
    pub(crate) input: PlannedRead,
}
