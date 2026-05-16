use std::collections::BTreeMap;

use lpc_model::{
    EnumSlot, MapSlot, PositiveF32, PositiveF32Slot, Revision, SlotDataAccess, SlotDataMutAccess,
    SlotEnumOption, SlotEnumShape, SlotMapKeyShape, SlotMeta, SlotMutationError, SlotRecordAccess,
    SlotRecordMutAccess, SlotShape, SlotShapeId, SlotValue, SlotValueShape, SlottedEnum,
    SlottedEnumMut, ToLpValue, ValueEditorHint, ValueRootError, ValueSlot, Xy, XySlot,
};

/// Fixture-to-texture mapping authored on a fixture definition.
#[derive(Clone, Debug, PartialEq)]
pub enum MappingConfig {
    Disabled,
    Square {
        origin: XySlot,
        size: XySlot,
    },
    PathPoints {
        paths: MapSlot<u32, EnumSlot<PathSpec>>,
        sample_diameter: PositiveF32Slot,
    },
}

/// Specifies one path for a fixture.
#[derive(Clone, Debug, PartialEq)]
pub enum PathSpec {
    RingArray {
        center: XySlot,
        diameter: PositiveF32Slot,
        start_ring_inclusive: ValueSlot<u32>,
        end_ring_exclusive: ValueSlot<u32>,
        ring_lamp_counts: MapSlot<u32, ValueSlot<u32>>,
        offset_angle: ValueSlot<f32>,
        order: ValueSlot<RingOrder>,
    },
    Manual,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RingOrder {
    #[default]
    InnerFirst,
    OuterFirst,
}

impl MappingConfig {
    pub fn disabled() -> Self {
        Self::Disabled
    }

    pub fn square() -> Self {
        Self::Square {
            origin: XySlot::new(Xy([0.1, 0.2])),
            size: XySlot::new(Xy([0.8, 0.7])),
        }
    }

    pub fn path_points_default() -> Self {
        let mut paths = BTreeMap::new();
        paths.insert(
            0,
            EnumSlot::new(PathSpec::ring_array_counts(
                [0.5, 0.5],
                1.0,
                0,
                2,
                &[1, 96],
                0.0,
                RingOrder::InnerFirst,
            )),
        );
        Self::path_points(MapSlot::new(paths), 2.0)
    }

    pub fn path_points(paths: MapSlot<u32, EnumSlot<PathSpec>>, sample_diameter: f32) -> Self {
        Self::PathPoints {
            paths,
            sample_diameter: PositiveF32Slot::new(PositiveF32(sample_diameter)),
        }
    }

    pub fn default_variant(variant: &str) -> Result<Self, SlotMutationError> {
        match variant {
            "disabled" => Ok(Self::Disabled),
            "square" => Ok(Self::Square {
                origin: XySlot::default(),
                size: XySlot::default(),
            }),
            "path_points" => Ok(Self::PathPoints {
                paths: MapSlot::default(),
                sample_diameter: PositiveF32Slot::default(),
            }),
            other => Err(SlotMutationError::unknown_variant(format!(
                "unknown MappingConfig variant {other:?}; expected one of: disabled, square, path_points"
            ))),
        }
    }

    pub fn set_ring_lamp_counts(&mut self, counts: Vec<u32>) -> bool {
        let Self::PathPoints { paths, .. } = self else {
            return false;
        };
        let Some(path) = paths.entries.get_mut(&0) else {
            return false;
        };
        path.value_mut().set_ring_lamp_counts(counts)
    }

    pub fn square_fields(&self) -> Option<([f32; 2], [f32; 2])> {
        let Self::Square { origin, size, .. } = self else {
            return None;
        };
        Some((origin.value().0, size.value().0))
    }

    pub fn path_points_fields(&self) -> Option<(&MapSlot<u32, EnumSlot<PathSpec>>, f32)> {
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

impl Default for MappingConfig {
    fn default() -> Self {
        Self::default_variant("disabled").expect("default MappingConfig variant is valid")
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
            Self::Disabled => "disabled",
            Self::Square { .. } => "square",
            Self::PathPoints { .. } => "path_points",
        }
    }

    fn data(&self) -> SlotDataAccess<'_> {
        match self {
            Self::Disabled => SlotDataAccess::Unit(Revision::default()),
            Self::Square { .. } | Self::PathPoints { .. } => SlotDataAccess::Record(self),
        }
    }
}

impl SlottedEnumMut for MappingConfig {
    fn data_mut(&mut self) -> SlotDataMutAccess<'_> {
        match self {
            Self::Disabled => SlotDataMutAccess::Record(self),
            Self::Square { .. } | Self::PathPoints { .. } => SlotDataMutAccess::Record(self),
        }
    }

    fn set_variant_default(&mut self, variant: &str) -> Result<(), SlotMutationError> {
        *self = Self::default_variant(variant)?;
        Ok(())
    }
}

impl SlotRecordAccess for MappingConfig {
    fn field(&self, index: usize) -> Option<SlotDataAccess<'_>> {
        match self {
            Self::Disabled => None,
            Self::Square { origin, size } => match index {
                0 => Some(SlotDataAccess::Value(origin)),
                1 => Some(SlotDataAccess::Value(size)),
                _ => None,
            },
            Self::PathPoints {
                paths,
                sample_diameter,
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
            Self::Disabled => None,
            Self::Square { origin, size } => match index {
                0 => Some(SlotDataMutAccess::Value(origin)),
                1 => Some(SlotDataMutAccess::Value(size)),
                _ => None,
            },
            Self::PathPoints {
                paths,
                sample_diameter,
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
            "manual" => Ok(Self::Manual),
            other => Err(SlotMutationError::unknown_variant(format!(
                "unknown PathSpec variant {other:?}; expected one of: ring_array, manual"
            ))),
        }
    }

    pub fn manual() -> Self {
        Self::Manual
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
            Self::Manual => "manual",
        }
    }

    fn data(&self) -> SlotDataAccess<'_> {
        match self {
            Self::RingArray { .. } => SlotDataAccess::Record(self),
            Self::Manual => SlotDataAccess::Unit(Revision::default()),
        }
    }
}

impl SlottedEnumMut for PathSpec {
    fn data_mut(&mut self) -> SlotDataMutAccess<'_> {
        match self {
            Self::RingArray { .. } => SlotDataMutAccess::Record(self),
            Self::Manual => SlotDataMutAccess::Record(self),
        }
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
            Self::Manual => None,
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
            Self::Manual => None,
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
