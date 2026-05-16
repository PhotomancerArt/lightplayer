use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

use crate::{
    EnumSlot, FromLpValue, LpType, LpValue, MapSlot, PositiveF32, PositiveF32Slot, SlotDataAccess,
    SlotDataMutAccess, SlotEnumOption, SlotEnumShape, SlotMapKeyShape, SlotMeta, SlotMutationError,
    SlotRecordAccess, SlotRecordMutAccess, SlotShape, SlotShapeId, SlotValue, SlotValueShape,
    SlottedEnum, SlottedEnumMut, ToLpValue, ValueEditorHint, ValueRootError, ValueSlot, Xy, XySlot,
};

/// Fixture-to-texture mapping authored on a fixture definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MappingConfig {
    /// A mapping defined by fixture paths sampled from the target texture.
    PathPoints {
        paths: MapSlot<u32, EnumSlot<PathSpec>>,
        sample_diameter: PositiveF32Slot,
    },
}

/// Specifies one path for a fixture.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PathSpec {
    /// A display made of concentric rings of lamps, usually LEDs on a PCB.
    RingArray {
        center: XySlot,
        diameter: PositiveF32Slot,
        start_ring_inclusive: ValueSlot<u32>,
        end_ring_exclusive: ValueSlot<u32>,
        ring_lamp_counts: MapSlot<u32, ValueSlot<u32>>,
        offset_angle: ValueSlot<f32>,
        order: ValueSlot<RingOrder>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RingOrder {
    #[default]
    InnerFirst,
    OuterFirst,
}

impl MappingConfig {
    pub fn path_points(paths: MapSlot<u32, EnumSlot<PathSpec>>, sample_diameter: f32) -> Self {
        Self::PathPoints {
            paths,
            sample_diameter: PositiveF32Slot::new(PositiveF32(sample_diameter)),
        }
    }

    pub fn path_points_vec(paths: Vec<PathSpec>, sample_diameter: f32) -> Self {
        let mut entries = BTreeMap::new();
        for (index, path) in paths.into_iter().enumerate() {
            entries.insert(index as u32, EnumSlot::new(path));
        }
        Self::path_points(MapSlot::new(entries), sample_diameter)
    }

    pub fn default_variant(variant: &str) -> Result<Self, SlotMutationError> {
        match variant {
            "path_points" => Ok(Self::PathPoints {
                paths: MapSlot::default(),
                sample_diameter: PositiveF32Slot::default(),
            }),
            other => Err(SlotMutationError::unknown_variant(alloc::format!(
                "unknown MappingConfig variant {other:?}; expected one of: path_points"
            ))),
        }
    }
}

impl Default for MappingConfig {
    fn default() -> Self {
        Self::default_variant("path_points").expect("default MappingConfig variant is valid")
    }
}

impl SlotEnumShape for MappingConfig {
    fn slot_enum_shape() -> SlotShape {
        mapping_shape()
    }
}

impl SlottedEnum for MappingConfig {
    fn variant(&self) -> &str {
        match self {
            Self::PathPoints { .. } => "path_points",
        }
    }

    fn data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Record(self)
    }
}

impl SlottedEnumMut for MappingConfig {
    fn data_mut(&mut self) -> SlotDataMutAccess<'_> {
        SlotDataMutAccess::Record(self)
    }

    fn set_variant_default(&mut self, variant: &str) -> Result<(), SlotMutationError> {
        *self = Self::default_variant(variant)?;
        Ok(())
    }
}

impl SlotRecordAccess for MappingConfig {
    fn field(&self, index: usize) -> Option<SlotDataAccess<'_>> {
        match self {
            Self::PathPoints {
                paths,
                sample_diameter,
                ..
            } => match index {
                0 => Some(SlotDataAccess::Map(paths)),
                1 => Some(SlotDataAccess::Value(sample_diameter)),
                _ => None,
            },
        }
    }
}

impl SlotRecordMutAccess for MappingConfig {
    fn field_mut(&mut self, index: usize) -> Option<SlotDataMutAccess<'_>> {
        match self {
            Self::PathPoints {
                paths,
                sample_diameter,
                ..
            } => match index {
                0 => Some(SlotDataMutAccess::Map(paths)),
                1 => Some(SlotDataMutAccess::Value(sample_diameter)),
                _ => None,
            },
        }
    }
}

impl PathSpec {
    pub fn ring_array(
        center: [f32; 2],
        diameter: f32,
        start_ring_inclusive: u32,
        end_ring_exclusive: u32,
        ring_lamp_counts: MapSlot<u32, ValueSlot<u32>>,
        offset_angle: f32,
        order: RingOrder,
    ) -> Self {
        Self::RingArray {
            center: XySlot::new(Xy(center)),
            diameter: PositiveF32Slot::new(PositiveF32(diameter)),
            start_ring_inclusive: ValueSlot::new(start_ring_inclusive),
            end_ring_exclusive: ValueSlot::new(end_ring_exclusive),
            ring_lamp_counts,
            offset_angle: ValueSlot::new(offset_angle),
            order: ValueSlot::new(order),
        }
    }

    pub fn ring_array_counts(
        center: [f32; 2],
        diameter: f32,
        start_ring_inclusive: u32,
        end_ring_exclusive: u32,
        ring_lamp_counts: &[u32],
        offset_angle: f32,
        order: RingOrder,
    ) -> Self {
        let mut counts = BTreeMap::new();
        for (index, count) in ring_lamp_counts.iter().copied().enumerate() {
            counts.insert(index as u32, ValueSlot::new(count));
        }
        Self::ring_array(
            center,
            diameter,
            start_ring_inclusive,
            end_ring_exclusive,
            MapSlot::new(counts),
            offset_angle,
            order,
        )
    }

    pub fn default_variant(variant: &str) -> Result<Self, SlotMutationError> {
        match variant {
            "ring_array" => Ok(Self::RingArray {
                center: XySlot::default(),
                diameter: PositiveF32Slot::default(),
                start_ring_inclusive: ValueSlot::default(),
                end_ring_exclusive: ValueSlot::default(),
                ring_lamp_counts: MapSlot::default(),
                offset_angle: ValueSlot::default(),
                order: ValueSlot::default(),
            }),
            other => Err(SlotMutationError::unknown_variant(alloc::format!(
                "unknown PathSpec variant {other:?}; expected one of: ring_array"
            ))),
        }
    }
}

impl Default for PathSpec {
    fn default() -> Self {
        Self::default_variant("ring_array").expect("default PathSpec variant is valid")
    }
}

impl SlotEnumShape for PathSpec {
    fn slot_enum_shape() -> SlotShape {
        path_spec_shape()
    }
}

impl SlottedEnum for PathSpec {
    fn variant(&self) -> &str {
        match self {
            Self::RingArray { .. } => "ring_array",
        }
    }

    fn data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Record(self)
    }
}

impl SlottedEnumMut for PathSpec {
    fn data_mut(&mut self) -> SlotDataMutAccess<'_> {
        SlotDataMutAccess::Record(self)
    }

    fn set_variant_default(&mut self, variant: &str) -> Result<(), SlotMutationError> {
        *self = Self::default_variant(variant)?;
        Ok(())
    }
}

impl SlotRecordAccess for PathSpec {
    fn field(&self, index: usize) -> Option<SlotDataAccess<'_>> {
        match self {
            Self::RingArray {
                center,
                diameter,
                start_ring_inclusive,
                end_ring_exclusive,
                ring_lamp_counts,
                offset_angle,
                order,
                ..
            } => match index {
                0 => Some(SlotDataAccess::Value(center)),
                1 => Some(SlotDataAccess::Value(diameter)),
                2 => Some(SlotDataAccess::Value(start_ring_inclusive)),
                3 => Some(SlotDataAccess::Value(end_ring_exclusive)),
                4 => Some(SlotDataAccess::Map(ring_lamp_counts)),
                5 => Some(SlotDataAccess::Value(offset_angle)),
                6 => Some(SlotDataAccess::Value(order)),
                _ => None,
            },
        }
    }
}

impl SlotRecordMutAccess for PathSpec {
    fn field_mut(&mut self, index: usize) -> Option<SlotDataMutAccess<'_>> {
        match self {
            Self::RingArray {
                center,
                diameter,
                start_ring_inclusive,
                end_ring_exclusive,
                ring_lamp_counts,
                offset_angle,
                order,
                ..
            } => match index {
                0 => Some(SlotDataMutAccess::Value(center)),
                1 => Some(SlotDataMutAccess::Value(diameter)),
                2 => Some(SlotDataMutAccess::Value(start_ring_inclusive)),
                3 => Some(SlotDataMutAccess::Value(end_ring_exclusive)),
                4 => Some(SlotDataMutAccess::Map(ring_lamp_counts)),
                5 => Some(SlotDataMutAccess::Value(offset_angle)),
                6 => Some(SlotDataMutAccess::Value(order)),
                _ => None,
            },
        }
    }
}

impl RingOrder {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InnerFirst => "inner_first",
            Self::OuterFirst => "outer_first",
        }
    }

    pub fn parse(value: &str) -> Result<Self, ValueRootError> {
        match value {
            "inner_first" => Ok(Self::InnerFirst),
            "outer_first" => Ok(Self::OuterFirst),
            other => Err(ValueRootError::new(alloc::format!(
                "unknown ring order {other:?}"
            ))),
        }
    }
}

impl ToLpValue for RingOrder {
    fn to_lp_value(&self) -> LpValue {
        LpValue::String(self.as_str().into())
    }
}

impl FromLpValue for RingOrder {
    fn from_lp_value(value: &LpValue) -> Result<Self, ValueRootError> {
        match value {
            LpValue::String(value) => Self::parse(&value),
            other => Err(ValueRootError::new(alloc::format!(
                "expected String, got {other:?}"
            ))),
        }
    }
}

impl SlotValue for RingOrder {
    const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name("RingOrder");

    fn value_shape() -> SlotValueShape {
        SlotValueShape {
            id: Self::SHAPE_ID,
            ty: LpType::String,
            meta: SlotMeta::empty(),
            editor: ValueEditorHint::Dropdown {
                options: alloc::vec![
                    SlotEnumOption::new("inner_first", "Inner first"),
                    SlotEnumOption::new("outer_first", "Outer first"),
                ],
            },
        }
    }
}

fn mapping_shape() -> SlotShape {
    use crate::slot::shape::{field, leaf, map, record, variant};

    SlotShape::Enum {
        meta: SlotMeta::empty(),
        variants: alloc::vec![variant(
            "path_points",
            record(alloc::vec![
                field(
                    "paths",
                    map(
                        SlotMapKeyShape::U32,
                        <PathSpec as SlotEnumShape>::slot_enum_shape()
                    ),
                ),
                field("sample_diameter", leaf(PositiveF32::value_shape())),
            ]),
        )],
    }
}

fn path_spec_shape() -> SlotShape {
    use crate::slot::shape::{field, leaf, map, record, value, variant};

    SlotShape::Enum {
        meta: SlotMeta::empty(),
        variants: alloc::vec![variant(
            "ring_array",
            record(alloc::vec![
                field("center", leaf(Xy::value_shape())),
                field("diameter", leaf(PositiveF32::value_shape())),
                field("start_ring_inclusive", value(LpType::U32)),
                field("end_ring_exclusive", value(LpType::U32)),
                field(
                    "ring_lamp_counts",
                    map(SlotMapKeyShape::U32, value(LpType::U32))
                ),
                field("offset_angle", value(LpType::F32)),
                field("order", leaf(RingOrder::value_shape())),
            ]),
        )],
    }
}
