use lpc_model::{
    Affine2d, Affine2dSlot, BindingDefs, ColorOrderSlot, ColorOrderValue, Dim2u, Dim2uSlot,
    EnumSlot, FromLpValue, LpType, LpValue, OptionSlot, SlotMeta, SlotShapeId, SlotValue,
    SlotValueShape, Slotted, ToLpValue, ValueEditorHint, ValueRootError, ValueSlot,
};

use super::{MappingConfig, shader_def::ScalarHint};

#[derive(Default, Slotted)]
pub struct FixtureDef {
    pub render_size: Dim2uSlot,
    pub bindings: BindingDefs,
    pub sampling: ValueSlot<FixtureSamplingConfig>,
    pub mapping: EnumSlot<MappingConfig>,
    pub color_order: ColorOrderSlot,
    pub transform: Affine2dSlot,
    pub brightness: OptionSlot<ScalarHint>,
    pub gamma_correction: OptionSlot<ValueSlot<bool>>,
}

impl FixtureDef {
    pub const KIND: &'static str = "fixture";

    pub fn new() -> Self {
        Self {
            render_size: default_render_size(),
            bindings: BindingDefs::default(),
            sampling: ValueSlot::new(FixtureSamplingConfig::TextureArea),
            mapping: EnumSlot::new(MappingConfig::path_points_default()),
            color_order: ColorOrderSlot::new(ColorOrderValue::Grb),
            transform: Affine2dSlot::new(Affine2d::identity()),
            brightness: OptionSlot::some(ScalarHint::mock(0.8)),
            gamma_correction: default_gamma_correction(),
        }
    }

    pub fn switch_mapping_to_square(&mut self) {
        self.mapping = EnumSlot::new(MappingConfig::square());
    }

    pub fn disable_mapping(&mut self) {
        self.mapping = EnumSlot::new(MappingConfig::disabled());
    }

    pub fn clear_brightness(&mut self) {
        self.brightness.set_none();
    }

    pub fn sampling(&self) -> FixtureSamplingConfig {
        *self.sampling.value()
    }

    pub fn render_size(&self) -> Dim2u {
        *self.render_size.value()
    }

    pub fn mapping(&self) -> &MappingConfig {
        self.mapping.value()
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
        self.mapping.value_mut().set_ring_lamp_counts(counts)
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

    pub fn as_str(self) -> &'static str {
        match self {
            Self::TextureArea => "texture_area",
            Self::Point => "point",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "texture_area" => Some(Self::TextureArea),
            "point" => Some(Self::Point),
            _ => None,
        }
    }
}

impl ToLpValue for FixtureSamplingConfig {
    fn to_lp_value(&self) -> LpValue {
        LpValue::String(self.as_str().to_string())
    }
}

impl FromLpValue for FixtureSamplingConfig {
    fn from_lp_value(value: &LpValue) -> Result<Self, ValueRootError> {
        match value {
            LpValue::String(value) => {
                Self::parse(value).ok_or_else(|| ValueRootError::new("expected fixture sampling"))
            }
            other => Err(ValueRootError::new(format!(
                "expected String, got {other:?}"
            ))),
        }
    }
}

impl SlotValue for FixtureSamplingConfig {
    const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name("FixtureSamplingConfig");

    fn value_shape() -> SlotValueShape {
        SlotValueShape {
            id: Self::SHAPE_ID,
            ty: LpType::String,
            meta: SlotMeta::empty(),
            editor: ValueEditorHint::Plain,
        }
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
