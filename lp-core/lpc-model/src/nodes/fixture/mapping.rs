use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

use crate::{
    EnumSlot, FromLpValue, LpType, LpValue, MapSlot, PositiveF32, PositiveF32Slot, SlotEnumOption,
    SlotMeta, SlotShapeId, SlotValue, SlotValueShape, Slotted, ToLpValue, ValueEditorHint,
    ValueRootError, ValueSlot, Xy, XySlot,
};

/// Fixture-to-texture mapping authored on a fixture definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Slotted)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MappingConfig {
    /// No authored fixture mapping has been selected yet.
    #[default]
    Unset,

    /// A mapping defined by fixture paths sampled from the target texture.
    PathPoints {
        paths: MapSlot<u32, EnumSlot<PathSpec>>,
        sample_diameter: PositiveF32Slot,
    },
}

/// Specifies one path for a fixture.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Slotted)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PathSpec {
    /// A display made of concentric rings of lamps, usually LEDs on a PCB.
    #[default]
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
