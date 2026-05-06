use lpc_model::{
    Affine2d, Affine2dSlot, ColorOrderSlot, ColorOrderValue, FieldSlot, FrameId, OptionSlot,
    PositiveF32Slot, RelativeNodeRef, RelativeNodeRefSlot, SlotDataAccess, SlotEnumAccess,
    SlotEnumShape, SlotMapValueAccess, SlotRecordAccess, SlotShape, XySlot, current_state_version,
};

use crate::model::mapping_shape;

use super::shader_def::ScalarHint;

#[derive(lpc_model::SlotRecord)]
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
}

impl Default for FixtureDef {
    fn default() -> Self {
        Self::new()
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

impl SlotEnumShape for FixtureMapping {
    fn slot_enum_shape() -> lpc_model::SlotShape {
        mapping_shape()
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

impl FieldSlot for FixtureMapping {
    fn slot_field_shape() -> SlotShape {
        mapping_shape()
    }

    fn slot_field_data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Enum(self)
    }
}
