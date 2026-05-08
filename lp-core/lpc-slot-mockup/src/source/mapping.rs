use std::collections::BTreeMap;

use lpc_model::{
    FieldSlot, Revision, MapSlot, PositiveF32Slot, RatioSlot, SlotDataAccess, SlotEnumAccess,
    SlotEnumShape, SlotMapKeyShape, SlotMapValueAccess, SlotRecordAccess, SlotRecordShape,
    SlotShape, ValueSlot, XySlot, current_revision,
};

use super::{RingLampCounts, ring_lamp_counts_shape};

/// Fixture pixel/point mapping authored on a fixture definition.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FixtureMapping {
    Disabled {
        #[serde(skip, default = "current_state_version")]
        variant_revision: Revision,
    },
    Circle {
        #[serde(skip, default = "current_state_version")]
        variant_revision: Revision,
        center: XySlot,
        radius: PositiveF32Slot,
    },
    Square {
        #[serde(skip, default = "current_state_version")]
        variant_revision: Revision,
        origin: XySlot,
        size: XySlot,
    },
    PathPoints {
        #[serde(skip, default = "current_state_version")]
        variant_revision: Revision,
        points: MapSlot<u32, MappingPoint>,
        path: PathSpec,
    },
}

/// Stable-key point data used by source-like fixture mappings.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, lpc_model::SlotRecord)]
pub struct MappingPoint {
    position: XySlot,
    intensity: RatioSlot,
}

/// Higher-level path generator/config that owns no map keys itself.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PathSpec {
    RingArray {
        #[serde(skip, default = "current_state_version")]
        variant_revision: Revision,
        ring_lamp_counts: ValueSlot<Vec<u32>>,
        semantic_ring_lamp_counts: ValueSlot<RingLampCounts>,
        clockwise: ValueSlot<bool>,
    },
    Manual {
        #[serde(skip, default = "current_state_version")]
        variant_revision: Revision,
    },
}

impl FixtureMapping {
    pub fn disabled() -> Self {
        Self::Disabled {
            variant_revision: current_revision(),
        }
    }

    pub fn circle() -> Self {
        Self::Circle {
            variant_revision: current_revision(),
            center: XySlot::new([0.5, 0.5]),
            radius: PositiveF32Slot::new(0.4),
        }
    }

    pub fn square() -> Self {
        Self::Square {
            variant_revision: current_revision(),
            origin: XySlot::new([0.1, 0.2]),
            size: XySlot::new([0.8, 0.7]),
        }
    }

    pub fn path_points() -> Self {
        let mut points = BTreeMap::new();
        points.insert(1, MappingPoint::new([0.1, 0.2], 1.0));
        points.insert(2, MappingPoint::new([0.4, 0.8], 0.75));

        Self::PathPoints {
            variant_revision: current_revision(),
            points: MapSlot::new(points),
            path: PathSpec::ring_array(vec![1, 96], true),
        }
    }

    pub fn set_ring_lamp_counts(&mut self, counts: Vec<u32>) -> bool {
        let Self::PathPoints { path, .. } = self else {
            return false;
        };
        path.set_ring_lamp_counts(counts)
    }
}

impl SlotEnumShape for FixtureMapping {
    fn slot_enum_shape() -> SlotShape {
        mapping_shape()
    }
}

impl SlotEnumAccess for FixtureMapping {
    fn variant_revision(&self) -> Revision {
        match self {
            Self::Disabled {
                variant_revision,
            }
            | Self::Circle {
                variant_revision,
                ..
            }
            | Self::Square {
                variant_revision,
                ..
            }
            | Self::PathPoints {
                variant_revision,
                ..
            } => *variant_revision,
        }
    }

    fn variant(&self) -> &str {
        match self {
            Self::Disabled { .. } => "disabled",
            Self::Circle { .. } => "circle",
            Self::Square { .. } => "square",
            Self::PathPoints { .. } => "path_points",
        }
    }

    fn data(&self) -> SlotDataAccess<'_> {
        match self {
            Self::Disabled {
                variant_revision,
            } => SlotDataAccess::Unit(*variant_revision),
            Self::Circle { .. } | Self::Square { .. } | Self::PathPoints { .. } => {
                SlotDataAccess::Record(self)
            }
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
            Self::PathPoints { points, path, .. } => match index {
                0 => Some(SlotDataAccess::Map(points)),
                1 => Some(SlotDataAccess::Enum(path)),
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

impl MappingPoint {
    fn new(position: [f32; 2], intensity: f32) -> Self {
        Self {
            position: XySlot::new(position),
            intensity: RatioSlot::new(intensity),
        }
    }
}

impl PathSpec {
    fn ring_array(ring_lamp_counts: Vec<u32>, clockwise: bool) -> Self {
        Self::RingArray {
            variant_revision: current_revision(),
            semantic_ring_lamp_counts: ValueSlot::new(RingLampCounts::new(
                ring_lamp_counts.clone(),
            )),
            ring_lamp_counts: ValueSlot::new(ring_lamp_counts),
            clockwise: ValueSlot::new(clockwise),
        }
    }

    fn set_ring_lamp_counts(&mut self, counts: Vec<u32>) -> bool {
        let Self::RingArray {
            ring_lamp_counts, ..
        } = self
        else {
            return false;
        };
        ring_lamp_counts.set(counts);
        true
    }
}

impl SlotEnumShape for PathSpec {
    fn slot_enum_shape() -> SlotShape {
        path_spec_shape()
    }
}

impl SlotEnumAccess for PathSpec {
    fn variant_revision(&self) -> Revision {
        match self {
            Self::RingArray {
                variant_revision,
                ..
            }
            | Self::Manual {
                variant_revision,
            } => *variant_revision,
        }
    }

    fn variant(&self) -> &str {
        match self {
            Self::RingArray { .. } => "ring_array",
            Self::Manual { .. } => "manual",
        }
    }

    fn data(&self) -> SlotDataAccess<'_> {
        match self {
            Self::RingArray { .. } => SlotDataAccess::Record(self),
            Self::Manual {
                variant_revision,
            } => SlotDataAccess::Unit(*variant_revision),
        }
    }
}

impl SlotRecordAccess for PathSpec {
    fn field(&self, index: usize) -> Option<SlotDataAccess<'_>> {
        match self {
            Self::RingArray {
                ring_lamp_counts,
                semantic_ring_lamp_counts,
                clockwise,
                ..
            } => match index {
                0 => Some(SlotDataAccess::Value(ring_lamp_counts)),
                1 => Some(SlotDataAccess::Value(semantic_ring_lamp_counts)),
                2 => Some(SlotDataAccess::Value(clockwise)),
                _ => None,
            },
            Self::Manual { .. } => None,
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

fn mapping_shape() -> SlotShape {
    use lpc_model::slot::shape::{field, leaf, map, record, unit, variant};

    SlotShape::Enum {
        meta: lpc_model::SlotMeta::empty(),
        variants: vec![
            variant(
                "circle",
                record(vec![
                    field("center", leaf(lpc_model::xy_shape())),
                    field("radius", leaf(lpc_model::positive_f32_shape())),
                ]),
            ),
            variant(
                "square",
                record(vec![
                    field("origin", leaf(lpc_model::xy_shape())),
                    field("size", leaf(lpc_model::xy_shape())),
                ]),
            ),
            variant(
                "path_points",
                record(vec![
                    field(
                        "points",
                        map(
                            SlotMapKeyShape::U32,
                            <MappingPoint as SlotRecordShape>::slot_record_shape(),
                        ),
                    ),
                    field("path", path_spec_shape()),
                ]),
            ),
            variant("disabled", unit()),
        ],
    }
}

fn path_spec_shape() -> SlotShape {
    use lpc_model::slot::shape::{field, leaf, record, unit, value, variant};

    SlotShape::Enum {
        meta: lpc_model::SlotMeta::empty(),
        variants: vec![
            variant(
                "ring_array",
                record(vec![
                    field("ring_lamp_counts", leaf(lpc_model::u32_list_shape())),
                    field("semantic_ring_lamp_counts", leaf(ring_lamp_counts_shape())),
                    field("clockwise", value(lpc_model::LpType::Bool)),
                ]),
            ),
            variant("manual", unit()),
        ],
    }
}
