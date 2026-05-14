use std::collections::BTreeMap;

use lpc_model::{
    FieldSlot, MapSlot, PositiveF32, PositiveF32Slot, Revision, SlotDataAccess, SlotEnumAccess,
    SlotEnumOption, SlotEnumShape, SlotMapKeyShape, SlotMapValueAccess, SlotMeta, SlotRecordAccess,
    SlotShape, SlotShapeId, SlotValue, SlotValueShape, ToLpValue, ValueEditorHint, ValueRootError,
    ValueSlot, Xy, XySlot, current_revision,
};

/// Fixture-to-texture mapping authored on a fixture definition.
#[derive(Clone, Debug, PartialEq)]
pub enum MappingConfig {
    Disabled {
        variant_revision: Revision,
    },
    Square {
        variant_revision: Revision,
        origin: XySlot,
        size: XySlot,
    },
    PathPoints {
        variant_revision: Revision,
        paths: MapSlot<u32, PathSpec>,
        sample_diameter: PositiveF32Slot,
    },
}

/// Specifies one path for a fixture.
#[derive(Clone, Debug, PartialEq)]
pub enum PathSpec {
    RingArray {
        variant_revision: Revision,
        center: XySlot,
        diameter: PositiveF32Slot,
        start_ring_inclusive: ValueSlot<u32>,
        end_ring_exclusive: ValueSlot<u32>,
        ring_lamp_counts: MapSlot<u32, ValueSlot<u32>>,
        offset_angle: ValueSlot<f32>,
        order: ValueSlot<RingOrder>,
    },
    Manual {
        variant_revision: Revision,
    },
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RingOrder {
    #[default]
    InnerFirst,
    OuterFirst,
}

impl MappingConfig {
    pub fn disabled() -> Self {
        Self::Disabled {
            variant_revision: current_revision(),
        }
    }

    pub fn square() -> Self {
        Self::square_from_codec([0.1, 0.2], [0.8, 0.7])
    }

    pub fn square_from_codec(origin: [f32; 2], size: [f32; 2]) -> Self {
        Self::Square {
            variant_revision: current_revision(),
            origin: XySlot::new(Xy(origin)),
            size: XySlot::new(Xy(size)),
        }
    }

    pub fn path_points_default() -> Self {
        let mut paths = BTreeMap::new();
        paths.insert(
            0,
            PathSpec::ring_array_counts(
                [0.5, 0.5],
                1.0,
                0,
                2,
                &[1, 96],
                0.0,
                RingOrder::InnerFirst,
            ),
        );
        Self::path_points(MapSlot::new(paths), 2.0)
    }

    pub fn path_points(paths: MapSlot<u32, PathSpec>, sample_diameter: f32) -> Self {
        Self::PathPoints {
            variant_revision: current_revision(),
            paths,
            sample_diameter: PositiveF32Slot::new(PositiveF32(sample_diameter)),
        }
    }

    pub fn set_ring_lamp_counts(&mut self, counts: Vec<u32>) -> bool {
        let Self::PathPoints { paths, .. } = self else {
            return false;
        };
        let Some(path) = paths.entries.get_mut(&0) else {
            return false;
        };
        path.set_ring_lamp_counts(counts)
    }

    pub fn square_fields(&self) -> Option<([f32; 2], [f32; 2])> {
        let Self::Square { origin, size, .. } = self else {
            return None;
        };
        Some((origin.value().0, size.value().0))
    }

    pub fn path_points_fields(&self) -> Option<(&MapSlot<u32, PathSpec>, f32)> {
        let Self::PathPoints {
            paths,
            sample_diameter,
            ..
        } = self
        else {
            return None;
        };
        Some((paths, sample_diameter.value().0))
    }
}

impl SlotEnumShape for MappingConfig {
    fn slot_enum_shape() -> SlotShape {
        mapping_shape()
    }
}

impl SlotEnumAccess for MappingConfig {
    fn variant_revision(&self) -> Revision {
        match self {
            Self::Disabled { variant_revision }
            | Self::Square {
                variant_revision, ..
            }
            | Self::PathPoints {
                variant_revision, ..
            } => *variant_revision,
        }
    }

    fn variant(&self) -> &str {
        match self {
            Self::Disabled { .. } => "disabled",
            Self::Square { .. } => "square",
            Self::PathPoints { .. } => "path_points",
        }
    }

    fn data(&self) -> SlotDataAccess<'_> {
        match self {
            Self::Disabled { variant_revision } => SlotDataAccess::Unit(*variant_revision),
            Self::Square { .. } | Self::PathPoints { .. } => SlotDataAccess::Record(self),
        }
    }
}

impl SlotRecordAccess for MappingConfig {
    fn field(&self, index: usize) -> Option<SlotDataAccess<'_>> {
        match self {
            Self::Disabled { .. } => None,
            Self::Square { origin, size, .. } => match index {
                0 => Some(SlotDataAccess::Value(origin)),
                1 => Some(SlotDataAccess::Value(size)),
                _ => None,
            },
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
            variant_revision: current_revision(),
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

    pub fn manual() -> Self {
        Self::Manual {
            variant_revision: current_revision(),
        }
    }

    fn set_ring_lamp_counts(&mut self, counts: Vec<u32>) -> bool {
        let Self::RingArray {
            ring_lamp_counts, ..
        } = self
        else {
            return false;
        };
        let entries = counts
            .into_iter()
            .enumerate()
            .map(|(index, count)| (index as u32, ValueSlot::new(count)))
            .collect();
        *ring_lamp_counts = MapSlot::new(entries);
        true
    }

    pub fn ring_array_fields(
        &self,
    ) -> Option<(
        [f32; 2],
        f32,
        u32,
        u32,
        &MapSlot<u32, ValueSlot<u32>>,
        f32,
        RingOrder,
    )> {
        let Self::RingArray {
            center,
            diameter,
            start_ring_inclusive,
            end_ring_exclusive,
            ring_lamp_counts,
            offset_angle,
            order,
            ..
        } = self
        else {
            return None;
        };
        Some((
            center.value().0,
            diameter.value().0,
            *start_ring_inclusive.value(),
            *end_ring_exclusive.value(),
            ring_lamp_counts,
            *offset_angle.value(),
            *order.value(),
        ))
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
                variant_revision, ..
            }
            | Self::Manual { variant_revision } => *variant_revision,
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
            Self::Manual { variant_revision } => SlotDataAccess::Unit(*variant_revision),
        }
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
            other => Err(ValueRootError::new(format!("unknown ring order {other:?}"))),
        }
    }
}

impl ToLpValue for RingOrder {
    fn to_lp_value(&self) -> lpc_model::LpValue {
        lpc_model::LpValue::String(self.as_str().to_string())
    }
}

impl lpc_model::FromLpValue for RingOrder {
    fn from_lp_value(value: &lpc_model::LpValue) -> Result<Self, ValueRootError> {
        match value {
            lpc_model::LpValue::String(value) => Self::parse(value),
            other => Err(ValueRootError::new(format!(
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
            ty: lpc_model::LpType::String,
            meta: SlotMeta::empty(),
            editor: ValueEditorHint::Dropdown {
                options: vec![
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
        variants: vec![
            variant("disabled", SlotShape::unit()),
            variant(
                "square",
                record(vec![
                    field("origin", leaf(Xy::value_shape())),
                    field("size", leaf(Xy::value_shape())),
                ]),
            ),
            variant(
                "path_points",
                record(vec![
                    field(
                        "paths",
                        map(
                            SlotMapKeyShape::U32,
                            <PathSpec as SlotEnumShape>::slot_enum_shape(),
                        ),
                    ),
                    field("sample_diameter", leaf(PositiveF32::value_shape())),
                ]),
            ),
        ],
    }
}

fn path_spec_shape() -> SlotShape {
    use lpc_model::slot::shape::{field, leaf, map, record, value, variant};

    SlotShape::Enum {
        meta: SlotMeta::empty(),
        variants: vec![
            variant(
                "ring_array",
                record(vec![
                    field("center", leaf(Xy::value_shape())),
                    field("diameter", leaf(PositiveF32::value_shape())),
                    field("start_ring_inclusive", value(lpc_model::LpType::U32)),
                    field("end_ring_exclusive", value(lpc_model::LpType::U32)),
                    field(
                        "ring_lamp_counts",
                        map(SlotMapKeyShape::U32, value(lpc_model::LpType::U32)),
                    ),
                    field("offset_angle", value(lpc_model::LpType::F32)),
                    field("order", leaf(RingOrder::value_shape())),
                ]),
            ),
            variant("manual", SlotShape::unit()),
        ],
    }
}
