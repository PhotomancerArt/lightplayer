use lpc_model::{
    Affine2d, Affine2dSlot, ColorOrderSlot, ColorOrderValue, OptionSlot, RelativeNodeRef,
    RelativeNodeRefSlot,
};

use super::{FixtureMapping, shader_def::ScalarHint};

#[derive(lpc_model::SlotRecord, serde::Serialize, serde::Deserialize)]
#[slot(root)]
pub struct FixtureDef {
    output_loc: RelativeNodeRefSlot,
    texture_loc: RelativeNodeRefSlot,
    mapping: FixtureMapping,
    color_order: ColorOrderSlot,
    transform: Affine2dSlot,
    brightness: OptionSlot<ScalarHint>,
    gamma_correction: lpc_model::ValueSlot<bool>,
}

impl FixtureDef {
    pub fn new() -> Self {
        Self {
            output_loc: RelativeNodeRefSlot::new(RelativeNodeRef::parse("..output").unwrap()),
            texture_loc: RelativeNodeRefSlot::new(RelativeNodeRef::parse("..texture").unwrap()),
            mapping: FixtureMapping::path_points(),
            color_order: ColorOrderSlot::new(ColorOrderValue::Grb),
            transform: Affine2dSlot::new(Affine2d::identity()),
            brightness: OptionSlot::some(ScalarHint::mock(0.8)),
            gamma_correction: lpc_model::ValueSlot::new(true),
        }
    }

    pub fn switch_mapping_to_square(&mut self) {
        self.mapping = FixtureMapping::square();
    }

    pub fn disable_mapping(&mut self) {
        self.mapping = FixtureMapping::disabled();
    }

    pub fn clear_brightness(&mut self) {
        self.brightness.set_none();
    }

    pub fn set_ring_lamp_counts(&mut self, counts: Vec<u32>) -> bool {
        self.mapping.set_ring_lamp_counts(counts)
    }
}

impl Default for FixtureDef {
    fn default() -> Self {
        Self::new()
    }
}
