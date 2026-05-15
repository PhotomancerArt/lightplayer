use std::collections::BTreeMap;

use lpc_model::{
    FieldSlot, FieldSlotMut, MapSlot, PositiveF32, PositiveF32Slot, Revision, SlotDataAccess,
    SlotDataMutAccess, SlotEnumAccess, SlotEnumDefaultVariant, SlotEnumMutAccess, SlotEnumOption,
    SlotEnumShape, SlotMapKeyShape, SlotMapValueAccess, SlotMapValueMutAccess, SlotMeta,
    SlotMutationError, SlotRecordAccess, SlotRecordMutAccess, SlotShape, SlotShapeId, SlotValue,
    SlotValueShape, ToLpValue, ValueEditorHint, ValueRootError, ValueSlot, Xy, XySlot,
    current_revision,
    slot_codec::{
        ObjectReader, SlotCodec, SlotValueWriter, SlotWrite, SlotWriteError, SyntaxError,
        SyntaxEventSource, ValueReader,
    },
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
        Self::Square {
            variant_revision: current_revision(),
            origin: XySlot::new(Xy([0.1, 0.2])),
            size: XySlot::new(Xy([0.8, 0.7])),
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

    pub fn default_variant(revision: Revision, variant: &str) -> Result<Self, SlotMutationError> {
        match variant {
            "disabled" => Ok(Self::Disabled {
                variant_revision: revision,
            }),
            "square" => Ok(Self::Square {
                variant_revision: revision,
                origin: XySlot::default(),
                size: XySlot::default(),
            }),
            "path_points" => Ok(Self::PathPoints {
                variant_revision: revision,
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

impl Default for MappingConfig {
    fn default() -> Self {
        Self::default_variant(current_revision(), "disabled")
            .expect("default MappingConfig variant is valid")
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

impl SlotEnumMutAccess for MappingConfig {
    fn variant_revision(&self) -> Revision {
        SlotEnumAccess::variant_revision(self)
    }

    fn variant(&self) -> &str {
        SlotEnumAccess::variant(self)
    }

    fn data_mut(&mut self) -> SlotDataMutAccess<'_> {
        match self {
            Self::Disabled { variant_revision } => SlotDataMutAccess::Unit(variant_revision),
            Self::Square { .. } | Self::PathPoints { .. } => SlotDataMutAccess::Record(self),
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

impl SlotEnumDefaultVariant for MappingConfig {
    fn set_variant_default(
        &mut self,
        revision: Revision,
        variant: &str,
    ) -> Result<(), SlotMutationError> {
        *self = Self::default_variant(revision, variant)?;
        Ok(())
    }
}

impl SlotRecordMutAccess for MappingConfig {
    fn field_mut(&mut self, index: usize) -> Option<SlotDataMutAccess<'_>> {
        match self {
            Self::Disabled { .. } => None,
            Self::Square { origin, size, .. } => match index {
                0 => Some(SlotDataMutAccess::Value(origin)),
                1 => Some(SlotDataMutAccess::Value(size)),
                _ => None,
            },
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

impl SlotMapValueAccess for MappingConfig {
    fn slot_data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Enum(self)
    }
}

impl SlotMapValueMutAccess for MappingConfig {
    fn slot_data_mut(&mut self) -> SlotDataMutAccess<'_> {
        SlotDataMutAccess::Enum(self)
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

impl FieldSlotMut for MappingConfig {
    fn slot_field_data_mut(&mut self) -> SlotDataMutAccess<'_> {
        SlotDataMutAccess::Enum(self)
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

    pub fn default_variant(revision: Revision, variant: &str) -> Result<Self, SlotMutationError> {
        match variant {
            "ring_array" => Ok(Self::RingArray {
                variant_revision: revision,
                center: XySlot::default(),
                diameter: PositiveF32Slot::default(),
                start_ring_inclusive: ValueSlot::default(),
                end_ring_exclusive: ValueSlot::default(),
                ring_lamp_counts: MapSlot::default(),
                offset_angle: ValueSlot::default(),
                order: ValueSlot::default(),
            }),
            "manual" => Ok(Self::Manual {
                variant_revision: revision,
            }),
            other => Err(SlotMutationError::unknown_variant(format!(
                "unknown PathSpec variant {other:?}; expected one of: ring_array, manual"
            ))),
        }
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

impl Default for PathSpec {
    fn default() -> Self {
        Self::default_variant(current_revision(), "ring_array")
            .expect("default PathSpec variant is valid")
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

impl SlotEnumMutAccess for PathSpec {
    fn variant_revision(&self) -> Revision {
        SlotEnumAccess::variant_revision(self)
    }

    fn variant(&self) -> &str {
        SlotEnumAccess::variant(self)
    }

    fn data_mut(&mut self) -> SlotDataMutAccess<'_> {
        match self {
            Self::RingArray { .. } => SlotDataMutAccess::Record(self),
            Self::Manual { variant_revision } => SlotDataMutAccess::Unit(variant_revision),
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

impl SlotEnumDefaultVariant for PathSpec {
    fn set_variant_default(
        &mut self,
        revision: Revision,
        variant: &str,
    ) -> Result<(), SlotMutationError> {
        *self = Self::default_variant(revision, variant)?;
        Ok(())
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
            Self::Manual { .. } => None,
        }
    }
}

impl SlotMapValueAccess for PathSpec {
    fn slot_data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Enum(self)
    }
}

impl SlotMapValueMutAccess for PathSpec {
    fn slot_data_mut(&mut self) -> SlotDataMutAccess<'_> {
        SlotDataMutAccess::Enum(self)
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

impl FieldSlotMut for PathSpec {
    fn slot_field_data_mut(&mut self) -> SlotDataMutAccess<'_> {
        SlotDataMutAccess::Enum(self)
    }
}

impl SlotCodec for MappingConfig {
    fn read_slot<S>(value: ValueReader<'_, '_, S>) -> Result<Self, SyntaxError>
    where
        S: SyntaxEventSource,
    {
        read_mapping_config(value)
    }

    fn write_slot<W>(&self, value: SlotValueWriter<'_, W>) -> Result<(), SlotWriteError<W::Error>>
    where
        W: SlotWrite,
    {
        write_mapping_config(value, self)
    }
}

impl SlotCodec for PathSpec {
    fn read_slot<S>(value: ValueReader<'_, '_, S>) -> Result<Self, SyntaxError>
    where
        S: SyntaxEventSource,
    {
        read_path_spec(value)
    }

    fn write_slot<W>(&self, value: SlotValueWriter<'_, W>) -> Result<(), SlotWriteError<W::Error>>
    where
        W: SlotWrite,
    {
        write_path_spec(value, self)
    }
}

fn read_mapping_config<S>(value: ValueReader<'_, '_, S>) -> Result<MappingConfig, SyntaxError>
where
    S: SyntaxEventSource,
{
    let mut object = value.object()?;
    let kind = object.expect_discriminator("kind", &["disabled", "square", "path_points"])?;
    match kind.as_str() {
        "disabled" => {
            object.finish()?;
            Ok(MappingConfig::disabled())
        }
        "square" => read_mapping_square_body(object),
        "path_points" => read_mapping_path_points_body(object),
        _ => unreachable!("expect_discriminator validated variants"),
    }
}

fn read_mapping_square_body<S>(
    mut object: ObjectReader<'_, '_, S>,
) -> Result<MappingConfig, SyntaxError>
where
    S: SyntaxEventSource,
{
    const FIELDS: &[&str] = &["kind", "origin", "size"];
    let mut origin = None;
    let mut size = None;
    while let Some(mut prop) = object.next_prop()? {
        match prop.name() {
            "origin" => origin = Some(prop.value().f32_array()?),
            "size" => size = Some(prop.value().f32_array()?),
            other => return Err(prop.unknown_field(other, FIELDS)),
        }
    }
    Ok(MappingConfig::Square {
        variant_revision: current_revision(),
        origin: XySlot::new(Xy(
            origin.ok_or_else(|| object.missing_required_field("origin"))?
        )),
        size: XySlot::new(Xy(
            size.ok_or_else(|| object.missing_required_field("size"))?
        )),
    })
}

fn read_mapping_path_points_body<S>(
    mut object: ObjectReader<'_, '_, S>,
) -> Result<MappingConfig, SyntaxError>
where
    S: SyntaxEventSource,
{
    const FIELDS: &[&str] = &["kind", "paths", "sample_diameter"];
    let mut paths = None;
    let mut sample_diameter = None;
    while let Some(mut prop) = object.next_prop()? {
        match prop.name() {
            "paths" => paths = Some(prop.value().u32_key_map(read_path_spec)?),
            "sample_diameter" => sample_diameter = Some(prop.value().f32()?),
            other => return Err(prop.unknown_field(other, FIELDS)),
        }
    }
    Ok(MappingConfig::path_points(
        MapSlot::new(paths.ok_or_else(|| object.missing_required_field("paths"))?),
        sample_diameter.ok_or_else(|| object.missing_required_field("sample_diameter"))?,
    ))
}

fn read_path_spec<S>(value: ValueReader<'_, '_, S>) -> Result<PathSpec, SyntaxError>
where
    S: SyntaxEventSource,
{
    let mut object = value.object()?;
    let kind = object.expect_discriminator("kind", &["ring_array", "manual"])?;
    match kind.as_str() {
        "manual" => {
            object.finish()?;
            Ok(PathSpec::manual())
        }
        "ring_array" => read_ring_array_body(object),
        _ => unreachable!("expect_discriminator validated variants"),
    }
}

fn read_ring_array_body<S>(mut object: ObjectReader<'_, '_, S>) -> Result<PathSpec, SyntaxError>
where
    S: SyntaxEventSource,
{
    const FIELDS: &[&str] = &[
        "kind",
        "center",
        "diameter",
        "start_ring_inclusive",
        "end_ring_exclusive",
        "ring_lamp_counts",
        "offset_angle",
        "order",
    ];
    let mut center = None;
    let mut diameter = None;
    let mut start_ring_inclusive = None;
    let mut end_ring_exclusive = None;
    let mut ring_lamp_counts = None;
    let mut offset_angle = None;
    let mut order = None;
    while let Some(mut prop) = object.next_prop()? {
        match prop.name() {
            "center" => center = Some(prop.value().f32_array()?),
            "diameter" => diameter = Some(prop.value().f32()?),
            "start_ring_inclusive" => start_ring_inclusive = Some(prop.value().u32()?),
            "end_ring_exclusive" => end_ring_exclusive = Some(prop.value().u32()?),
            "ring_lamp_counts" => {
                ring_lamp_counts = Some(
                    prop.value()
                        .u32_key_map(|value| value.u32().map(ValueSlot::new))?,
                )
            }
            "offset_angle" => offset_angle = Some(prop.value().f32()?),
            "order" => {
                let text = prop.value().string()?;
                order = Some(RingOrder::parse(&text).unwrap_or_default());
            }
            other => return Err(prop.unknown_field(other, FIELDS)),
        }
    }
    Ok(PathSpec::ring_array(
        center.ok_or_else(|| object.missing_required_field("center"))?,
        diameter.ok_or_else(|| object.missing_required_field("diameter"))?,
        start_ring_inclusive
            .ok_or_else(|| object.missing_required_field("start_ring_inclusive"))?,
        end_ring_exclusive.ok_or_else(|| object.missing_required_field("end_ring_exclusive"))?,
        MapSlot::new(
            ring_lamp_counts.ok_or_else(|| object.missing_required_field("ring_lamp_counts"))?,
        ),
        offset_angle.ok_or_else(|| object.missing_required_field("offset_angle"))?,
        order.ok_or_else(|| object.missing_required_field("order"))?,
    ))
}

fn write_mapping_config<W>(
    value: SlotValueWriter<'_, W>,
    mapping: &MappingConfig,
) -> Result<(), SlotWriteError<W::Error>>
where
    W: SlotWrite,
{
    let mut object = value.object()?;
    match SlotEnumAccess::variant(mapping) {
        "disabled" => {
            object.prop("kind")?.string("disabled")?;
        }
        "square" => {
            let (origin, size) = mapping.square_fields().unwrap();
            object.prop("kind")?.string("square")?;
            object.prop("origin")?.f32_array(&origin)?;
            object.prop("size")?.f32_array(&size)?;
        }
        "path_points" => {
            let (paths, sample_diameter) = mapping.path_points_fields().unwrap();
            object.prop("kind")?.string("path_points")?;
            object
                .prop("paths")?
                .u32_key_map(&paths.entries, |value, path| write_path_spec(value, path))?;
            object.prop("sample_diameter")?.f32(sample_diameter)?;
        }
        _ => unreachable!("known mapping variant"),
    }
    object.finish()
}

fn write_path_spec<W>(
    value: SlotValueWriter<'_, W>,
    path: &PathSpec,
) -> Result<(), SlotWriteError<W::Error>>
where
    W: SlotWrite,
{
    let mut object = value.object()?;
    match SlotEnumAccess::variant(path) {
        "manual" => object.prop("kind")?.string("manual")?,
        "ring_array" => {
            let (
                center,
                diameter,
                start_ring_inclusive,
                end_ring_exclusive,
                ring_lamp_counts,
                offset_angle,
                order,
            ) = path.ring_array_fields().unwrap();
            object.prop("kind")?.string("ring_array")?;
            object.prop("center")?.f32_array(&center)?;
            object.prop("diameter")?.f32(diameter)?;
            object
                .prop("start_ring_inclusive")?
                .u32(start_ring_inclusive)?;
            object.prop("end_ring_exclusive")?.u32(end_ring_exclusive)?;
            object
                .prop("ring_lamp_counts")?
                .u32_key_map(&ring_lamp_counts.entries, |value, count| {
                    value.u32(*count.value())
                })?;
            object.prop("offset_angle")?.f32(offset_angle)?;
            object.prop("order")?.string(order.as_str())?;
        }
        _ => unreachable!("known path variant"),
    }
    object.finish()
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
