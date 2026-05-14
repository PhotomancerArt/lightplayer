use lpc_model::{
    Affine2d, Affine2dSlot, BindingDefs, ColorOrderSlot, ColorOrderValue, Dim2u, Dim2uSlot,
    OptionSlot, ValueSlot,
};

use super::{MappingConfig, shader_def::ScalarHint};

#[derive(lpc_model::SlotRecord)]
#[slot(root)]
pub struct FixtureDef {
    #[slot(skip)]
    pub kind: String,
    render_size: Dim2uSlot,
    bindings: BindingDefs,
    #[slot(skip)]
    sampling: FixtureSamplingConfig,
    #[slot(enum)]
    mapping: MappingConfig,
    color_order: ColorOrderSlot,
    transform: Affine2dSlot,
    brightness: OptionSlot<ScalarHint>,
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

    pub fn from_codec(
        render_size: Dim2u,
        mapping: MappingConfig,
        color_order: ColorOrderValue,
        transform: Affine2d,
        brightness: Option<ScalarHint>,
        gamma_correction: Option<bool>,
    ) -> Self {
        Self {
            kind: Self::KIND.to_string(),
            render_size: Dim2uSlot::new(render_size),
            bindings: BindingDefs::default(),
            sampling: FixtureSamplingConfig::TextureArea,
            mapping,
            color_order: ColorOrderSlot::new(color_order),
            transform: Affine2dSlot::new(transform),
            brightness: match brightness {
                Some(brightness) => OptionSlot::some(brightness),
                None => OptionSlot::none(),
            },
            gamma_correction: match gamma_correction {
                Some(value) => OptionSlot::some(ValueSlot::new(value)),
                None => OptionSlot::none(),
            },
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

    pub fn sampling(&self) -> FixtureSamplingConfig {
        self.sampling
    }

    pub fn render_size(&self) -> Dim2u {
        *self.render_size.value()
    }

    pub fn mapping(&self) -> &MappingConfig {
        &self.mapping
    }

    pub fn color_order(&self) -> ColorOrderValue {
        *self.color_order.value()
    }

    pub fn transform(&self) -> Affine2d {
        *self.transform.value()
    }

    pub fn brightness(&self) -> Option<&ScalarHint> {
        self.brightness.data.as_ref()
    }

    pub fn gamma_correction(&self) -> Option<bool> {
        self.gamma_correction
            .data
            .as_ref()
            .map(|value| *value.value())
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FixtureSamplingConfig {
    #[default]
    TextureArea,
    Point,
}

impl FixtureSamplingConfig {
    pub fn point() -> Self {
        Self::Point
    }
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
