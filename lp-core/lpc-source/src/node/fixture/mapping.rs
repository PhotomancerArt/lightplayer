use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use lpc_model::{
    FieldSlot, FrameId, FromModelValue, MapSlot, ModelType, ModelValue, PositiveF32Slot,
    SlotDataAccess, SlotEditorHint, SlotEnumAccess, SlotEnumOption, SlotEnumShape, SlotLeaf,
    SlotLeafError, SlotLeafId, SlotMapKeyShape, SlotMapValueAccess, SlotMeta, SlotRecordAccess,
    SlotShape, SlotValueShape, ToModelValue, ValueSlot, XySlot, current_state_version,
};
use serde::{Deserialize, Serialize};

/// Fixture-to-texture mapping authored on a fixture definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MappingConfig {
    /// A mapping defined by fixture paths sampled from the target texture.
    PathPoints {
        #[serde(skip, default = "current_state_version")]
        variant_changed_frame: FrameId,
        paths: MapSlot<u32, PathSpec>,
        sample_diameter: PositiveF32Slot,
    },
}

/// Specifies one path for a fixture.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PathSpec {
    /// A display made of concentric rings of lamps, usually LEDs on a PCB.
    RingArray {
        #[serde(skip, default = "current_state_version")]
        variant_changed_frame: FrameId,
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
    pub fn path_points(paths: MapSlot<u32, PathSpec>, sample_diameter: f32) -> Self {
        Self::PathPoints {
            variant_changed_frame: current_state_version(),
            paths,
            sample_diameter: PositiveF32Slot::new(sample_diameter),
        }
    }

    pub fn path_points_vec(paths: Vec<PathSpec>, sample_diameter: f32) -> Self {
        let mut entries = BTreeMap::new();
        for (index, path) in paths.into_iter().enumerate() {
            entries.insert(index as u32, path);
        }
        Self::path_points(MapSlot::new(entries), sample_diameter)
    }
}

impl SlotEnumShape for MappingConfig {
    fn slot_enum_shape() -> SlotShape {
        mapping_shape()
    }
}

impl SlotEnumAccess for MappingConfig {
    fn variant_changed_frame(&self) -> FrameId {
        match self {
            Self::PathPoints {
                variant_changed_frame,
                ..
            } => *variant_changed_frame,
        }
    }

    fn variant(&self) -> &str {
        match self {
            Self::PathPoints { .. } => "path_points",
        }
    }

    fn data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Record(self)
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

impl SlotMapValueAccess for MappingConfig {
    fn slot_data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Enum(self)
    }
}

impl FieldSlot for MappingConfig {
    fn slot_field_shape() -> SlotShape {
        mapping_shape()
    }

    fn slot_field_data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Enum(self)
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
            variant_changed_frame: current_state_version(),
            center: XySlot::new(center),
            diameter: PositiveF32Slot::new(diameter),
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
}

impl SlotEnumShape for PathSpec {
    fn slot_enum_shape() -> SlotShape {
        path_spec_shape()
    }
}

impl SlotEnumAccess for PathSpec {
    fn variant_changed_frame(&self) -> FrameId {
        match self {
            Self::RingArray {
                variant_changed_frame,
                ..
            } => *variant_changed_frame,
        }
    }

    fn variant(&self) -> &str {
        match self {
            Self::RingArray { .. } => "ring_array",
        }
    }

    fn data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Record(self)
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

impl SlotMapValueAccess for PathSpec {
    fn slot_data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Enum(self)
    }
}

impl FieldSlot for PathSpec {
    fn slot_field_shape() -> SlotShape {
        path_spec_shape()
    }

    fn slot_field_data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Enum(self)
    }
}

impl RingOrder {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InnerFirst => "inner_first",
            Self::OuterFirst => "outer_first",
        }
    }

    pub fn parse(value: &str) -> Result<Self, SlotLeafError> {
        match value {
            "inner_first" => Ok(Self::InnerFirst),
            "outer_first" => Ok(Self::OuterFirst),
            other => Err(SlotLeafError::new(alloc::format!(
                "unknown ring order {other:?}"
            ))),
        }
    }
}

impl ToModelValue for RingOrder {
    fn to_model_value(&self) -> ModelValue {
        ModelValue::String(self.as_str().into())
    }
}

impl FromModelValue for RingOrder {
    fn from_model_value(value: ModelValue) -> Result<Self, SlotLeafError> {
        match value {
            ModelValue::String(value) => Self::parse(&value),
            other => Err(SlotLeafError::new(alloc::format!(
                "expected String, got {other:?}"
            ))),
        }
    }
}

impl SlotLeaf for RingOrder {
    const LEAF_ID: SlotLeafId = SlotLeafId::from_static_name("slot.leaf.ring_order");

    fn value_shape() -> SlotValueShape {
        SlotValueShape {
            leaf: Self::LEAF_ID,
            ty: ModelType::String,
            meta: SlotMeta::empty(),
            editor: SlotEditorHint::Dropdown {
                options: alloc::vec![
                    SlotEnumOption::new("inner_first", "Inner first"),
                    SlotEnumOption::new("outer_first", "Outer first"),
                ],
            },
        }
    }
}

fn mapping_shape() -> SlotShape {
    use lpc_model::slot::shape::{field, leaf, map, record, variant};

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
                field("sample_diameter", leaf(lpc_model::positive_f32_shape())),
            ]),
        )],
    }
}

fn path_spec_shape() -> SlotShape {
    use lpc_model::slot::shape::{field, leaf, map, record, value, variant};

    SlotShape::Enum {
        meta: SlotMeta::empty(),
        variants: alloc::vec![variant(
            "ring_array",
            record(alloc::vec![
                field("center", leaf(lpc_model::xy_shape())),
                field("diameter", leaf(lpc_model::positive_f32_shape())),
                field("start_ring_inclusive", value(ModelType::U32)),
                field("end_ring_exclusive", value(ModelType::U32)),
                field(
                    "ring_lamp_counts",
                    map(SlotMapKeyShape::U32, value(ModelType::U32))
                ),
                field("offset_angle", value(ModelType::F32)),
                field("order", leaf(RingOrder::value_shape())),
            ]),
        )],
    }
}
