use lpc_model::{
    FrameId, ModelType, RelativeNodeRef, SlotAccess, SlotDataAccess, SlotEnumAccess,
    SlotMapValueAccess, SlotOption, SlotRecordAccess, SlotShapeId, SlotShapeRegistry,
    SlotShapeRegistryError, SlotValue, StaticSlotAccess, current_state_version,
};

use crate::model::{field, id, mapping_shape, option, record, reference, value, version};

use super::shader_def::ScalarHint;

pub struct FixtureDef {
    output_loc: SlotValue<RelativeNodeRef>,
    texture_loc: SlotValue<RelativeNodeRef>,
    mapping: FixtureMapping,
    color_order: SlotValue<String>,
    brightness: SlotOption<ScalarHint>,
    gamma_correction: SlotValue<bool>,
}

pub enum FixtureMapping {
    Circle {
        variant_changed_frame: FrameId,
        center: SlotValue<[f32; 2]>,
        radius: SlotValue<f32>,
    },
    Square {
        variant_changed_frame: FrameId,
        origin: SlotValue<[f32; 2]>,
        size: SlotValue<[f32; 2]>,
    },
}

impl FixtureDef {
    pub fn new() -> Self {
        Self {
            output_loc: SlotValue::new(RelativeNodeRef::parse("..output").unwrap()),
            texture_loc: SlotValue::new(RelativeNodeRef::parse("..texture").unwrap()),
            mapping: FixtureMapping::circle(),
            color_order: SlotValue::new(String::from("grb")),
            brightness: SlotOption::some(ScalarHint::mock(0.8)),
            gamma_correction: SlotValue::new(true),
        }
    }

    pub fn switch_mapping_to_square(&mut self) {
        self.mapping = FixtureMapping::square();
    }

    pub fn clear_brightness(&mut self) {
        self.brightness.set_none();
    }
}

impl Default for FixtureDef {
    fn default() -> Self {
        Self::new()
    }
}

impl SlotAccess for FixtureDef {
    fn shape_id(&self) -> SlotShapeId {
        <Self as StaticSlotAccess>::SHAPE_ID
    }

    fn data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Record(self)
    }
}

impl StaticSlotAccess for FixtureDef {
    const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name("source.fixture");

    fn register_shape(registry: &mut SlotShapeRegistry) -> Result<(), SlotShapeRegistryError> {
        registry.register_tree(
            version(),
            Self::SHAPE_ID,
            record(vec![
                field("output_loc", value(ModelType::String)),
                field("texture_loc", value(ModelType::String)),
                field("mapping", mapping_shape()),
                field("color_order", value(ModelType::String)),
                field("brightness", option(reference(id("source.scalar_hint")))),
                field("gamma_correction", value(ModelType::Bool)),
            ]),
        )
    }
}

impl SlotRecordAccess for FixtureDef {
    fn field(&self, index: usize) -> Option<SlotDataAccess<'_>> {
        match index {
            0 => Some(SlotDataAccess::Value(&self.output_loc)),
            1 => Some(SlotDataAccess::Value(&self.texture_loc)),
            2 => Some(SlotDataAccess::Enum(&self.mapping)),
            3 => Some(SlotDataAccess::Value(&self.color_order)),
            4 => Some(SlotDataAccess::Option(&self.brightness)),
            5 => Some(SlotDataAccess::Value(&self.gamma_correction)),
            _ => None,
        }
    }
}

impl FixtureMapping {
    pub fn circle() -> Self {
        Self::Circle {
            variant_changed_frame: current_state_version(),
            center: SlotValue::new([0.5, 0.5]),
            radius: SlotValue::new(0.4),
        }
    }

    pub fn square() -> Self {
        Self::Square {
            variant_changed_frame: current_state_version(),
            origin: SlotValue::new([0.1, 0.2]),
            size: SlotValue::new([0.8, 0.7]),
        }
    }
}

impl SlotEnumAccess for FixtureMapping {
    fn variant_changed_frame(&self) -> FrameId {
        match self {
            Self::Circle {
                variant_changed_frame,
                ..
            }
            | Self::Square {
                variant_changed_frame,
                ..
            } => *variant_changed_frame,
        }
    }

    fn variant(&self) -> &str {
        match self {
            Self::Circle { .. } => "circle",
            Self::Square { .. } => "square",
        }
    }

    fn data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Record(self)
    }
}

impl SlotRecordAccess for FixtureMapping {
    fn field(&self, index: usize) -> Option<SlotDataAccess<'_>> {
        match self {
            Self::Circle { center, radius, .. } => match index {
                0 => Some(SlotDataAccess::Value(center)),
                1 => Some(SlotDataAccess::Value(radius)),
                _ => None,
            },
            Self::Square { origin, size, .. } => match index {
                0 => Some(SlotDataAccess::Value(origin)),
                1 => Some(SlotDataAccess::Value(size)),
                _ => None,
            },
        }
    }
}

impl SlotMapValueAccess for FixtureMapping {
    fn slot_data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Enum(self)
    }
}
