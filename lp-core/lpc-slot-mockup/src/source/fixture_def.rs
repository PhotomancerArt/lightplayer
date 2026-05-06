use lpc_model::{
    Affine2d, Affine2dSlot, ColorOrderSlot, ColorOrderValue, FrameId, ModelType, PositiveF32Slot,
    RelativeNodeRef, RelativeNodeRefSlot, SlotAccess, SlotDataAccess, SlotEnumAccess,
    SlotMapValueAccess, SlotOption, SlotRecordAccess, SlotShapeId, SlotShapeRegistry,
    SlotShapeRegistryError, StaticSlotAccess, XySlot, affine2d_shape, color_order_shape,
    current_state_version, relative_node_ref_shape,
};

use crate::model::{field, id, leaf, mapping_shape, option, record, reference, value};

use super::shader_def::ScalarHint;

pub struct FixtureDef {
    output_loc: RelativeNodeRefSlot,
    texture_loc: RelativeNodeRefSlot,
    mapping: FixtureMapping,
    color_order: ColorOrderSlot,
    transform: Affine2dSlot,
    brightness: SlotOption<ScalarHint>,
    gamma_correction: lpc_model::SlotValue<bool>,
}

pub enum FixtureMapping {
    Disabled {
        variant_changed_frame: FrameId,
    },
    Circle {
        variant_changed_frame: FrameId,
        center: XySlot,
        radius: PositiveF32Slot,
    },
    Square {
        variant_changed_frame: FrameId,
        origin: XySlot,
        size: XySlot,
    },
}

impl FixtureDef {
    pub fn new() -> Self {
        Self {
            output_loc: RelativeNodeRefSlot::new(RelativeNodeRef::parse("..output").unwrap()),
            texture_loc: RelativeNodeRefSlot::new(RelativeNodeRef::parse("..texture").unwrap()),
            mapping: FixtureMapping::circle(),
            color_order: ColorOrderSlot::new(ColorOrderValue::Grb),
            transform: Affine2dSlot::new(Affine2d::identity()),
            brightness: SlotOption::some(ScalarHint::mock(0.8)),
            gamma_correction: lpc_model::SlotValue::new(true),
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
            Self::SHAPE_ID,
            record(vec![
                field("output_loc", leaf(relative_node_ref_shape())),
                field("texture_loc", leaf(relative_node_ref_shape())),
                field("mapping", mapping_shape()),
                field("color_order", leaf(color_order_shape())),
                field("transform", leaf(affine2d_shape())),
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
            4 => Some(SlotDataAccess::Value(&self.transform)),
            5 => Some(SlotDataAccess::Option(&self.brightness)),
            6 => Some(SlotDataAccess::Value(&self.gamma_correction)),
            _ => None,
        }
    }
}

impl FixtureMapping {
    pub fn disabled() -> Self {
        Self::Disabled {
            variant_changed_frame: current_state_version(),
        }
    }

    pub fn circle() -> Self {
        Self::Circle {
            variant_changed_frame: current_state_version(),
            center: XySlot::new([0.5, 0.5]),
            radius: PositiveF32Slot::new(0.4),
        }
    }

    pub fn square() -> Self {
        Self::Square {
            variant_changed_frame: current_state_version(),
            origin: XySlot::new([0.1, 0.2]),
            size: XySlot::new([0.8, 0.7]),
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
            }
            | Self::Disabled {
                variant_changed_frame,
            } => *variant_changed_frame,
        }
    }

    fn variant(&self) -> &str {
        match self {
            Self::Disabled { .. } => "disabled",
            Self::Circle { .. } => "circle",
            Self::Square { .. } => "square",
        }
    }

    fn data(&self) -> SlotDataAccess<'_> {
        match self {
            Self::Disabled {
                variant_changed_frame,
            } => SlotDataAccess::Unit(*variant_changed_frame),
            Self::Circle { .. } | Self::Square { .. } => SlotDataAccess::Record(self),
        }
    }
}

impl SlotRecordAccess for FixtureMapping {
    fn field(&self, index: usize) -> Option<SlotDataAccess<'_>> {
        match self {
            Self::Disabled { .. } => None,
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
