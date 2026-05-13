use lpc_model::{
    Affine2d, Affine2dSlot, BindingDefs, ColorOrderSlot, ColorOrderValue, Dim2u, Dim2uSlot,
    OptionSlot, ValueSlot,
};

use super::{MappingConfig, shader_def::ScalarHint};

#[derive(lpc_model::SlotRecord, serde::Serialize, serde::Deserialize)]
#[slot(root)]
pub struct FixtureDef {
    #[slot(skip)]
    pub kind: String,
    #[serde(default = "default_render_size")]
    render_size: Dim2uSlot,
    #[serde(default, skip_serializing_if = "BindingDefs::is_empty")]
    bindings: BindingDefs,
    #[slot(skip)]
    #[serde(default)]
    sampling: FixtureSamplingConfig,
    #[slot(enum)]
    mapping: MappingConfig,
    color_order: ColorOrderSlot,
    transform: Affine2dSlot,
    brightness: OptionSlot<ScalarHint>,
    #[serde(default = "default_gamma_correction")]
    gamma_correction: OptionSlot<ValueSlot<bool>>,
}

impl FixtureDef {
    pub const KIND: &'static str = "fixture";

    pub fn new() -> Self {
        Self {
            kind: Self::KIND.to_string(),
            render_size: default_render_size(),
            bindings: BindingDefs::default(),
            sampling: FixtureSamplingConfig::TextureArea,
            mapping: MappingConfig::path_points_default(),
            color_order: ColorOrderSlot::new(ColorOrderValue::Grb),
            transform: Affine2dSlot::new(Affine2d::identity()),
            brightness: OptionSlot::some(ScalarHint::mock(0.8)),
            gamma_correction: default_gamma_correction(),
        }
    }

    pub fn switch_mapping_to_square(&mut self) {
        self.mapping = MappingConfig::square();
    }

    pub fn disable_mapping(&mut self) {
        self.mapping = MappingConfig::disabled();
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FixtureSamplingConfig {
    #[default]
    TextureArea,
    Point,
}

fn default_render_size() -> Dim2uSlot {
    Dim2uSlot::new(Dim2u {
        width: 16,
        height: 16,
    })
}

fn default_gamma_correction() -> OptionSlot<ValueSlot<bool>> {
    OptionSlot::some(ValueSlot::new(true))
}
