use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::slot_codec::{JsonSyntaxSource, SyntaxError, SyntaxEventSource, ValueReader};
use crate::{
    Revision, SlotData, SlotEnum, SlotMapDyn, SlotMapKey, SlotMapKeyShape, SlotName, SlotOptionDyn,
    SlotRecord, SlotShape, SlotShapeId, SlotShapeLookup, SlotShapeRegistry, SlotShapeView,
    WithRevision,
};

pub fn read_slot_snapshot_json(
    registry: &SlotShapeRegistry,
    id: SlotShapeId,
    json: &str,
) -> Result<SlotData, SyntaxError> {
    let shape = registry
        .get_shape(id)
        .ok_or_else(|| error(format!("missing slot shape: {id}")))?;
    let mut reader = crate::slot_codec::SlotReader::new(JsonSyntaxSource::new(json)?, registry);
    read_shape(registry, shape, reader.value())
}

pub fn read_slot_snapshot_shape_json(
    registry: &SlotShapeRegistry,
    shape: &SlotShape,
    json: &str,
) -> Result<SlotData, SyntaxError> {
    let mut reader = crate::slot_codec::SlotReader::new(JsonSyntaxSource::new(json)?, registry);
    read_shape(registry, SlotShapeView::Dynamic(shape), reader.value())
}

fn read_shape<S>(
    registry: &SlotShapeRegistry,
    shape: SlotShapeView<'_>,
    value: ValueReader<'_, '_, S>,
) -> Result<SlotData, SyntaxError>
where
    S: SyntaxEventSource,
{
    if let Some(id) = shape.ref_id() {
        let shape = registry
            .get_shape(id)
            .ok_or_else(|| error(format!("missing slot shape: {id}")))?;
        return read_shape(registry, shape, value);
    }

    if let Some(shape) = shape.custom_shape() {
        return read_shape(registry, shape, value);
    }

    let expected_kind = shape_kind(shape);
    let mut object = value.object()?;
    let actual_kind = object.expect_discriminator("kind", &[expected_kind])?;
    if actual_kind != expected_kind {
        return Err(error(format!(
            "expected sync snapshot kind `{expected_kind}`, found `{actual_kind}`"
        )));
    }

    if shape.is_unit() {
        read_unit(object)
    } else if let Some(value_shape) = shape.value_shape() {
        read_value(object, &value_shape.ty_owned())
    } else if shape.record_fields_len().is_some() {
        read_record(registry, object, shape)
    } else if let (Some(key), Some(value_shape)) = (shape.map_key(), shape.map_value()) {
        read_map(registry, object, key, value_shape)
    } else if shape.is_enum() {
        read_enum(registry, object, shape)
    } else if let Some(some) = shape.option_some() {
        read_option(registry, object, some)
    } else {
        unreachable!("custom and ref shapes are projected above")
    }
}

fn read_unit<S>(
    mut object: crate::slot_codec::ObjectReader<'_, '_, S>,
) -> Result<SlotData, SyntaxError>
where
    S: SyntaxEventSource,
{
    let mut revision = None;
    while let Some(mut prop) = object.next_prop()? {
        match prop.name() {
            "revision" => revision = Some(read_revision(prop.value())?),
            other => return Err(prop.unknown_field(other, &["kind", "revision"])),
        }
    }
    Ok(SlotData::Unit {
        revision: required(revision, &object, "revision")?,
    })
}

fn read_value<S>(
    mut object: crate::slot_codec::ObjectReader<'_, '_, S>,
    ty: &crate::LpType,
) -> Result<SlotData, SyntaxError>
where
    S: SyntaxEventSource,
{
    let mut changed_at = None;
    let mut value = None;
    while let Some(mut prop) = object.next_prop()? {
        match prop.name() {
            "changed_at" => changed_at = Some(read_revision(prop.value())?),
            "value" => value = Some(crate::slot_codec::read_lp_value(ty, prop.value())?),
            other => return Err(prop.unknown_field(other, &["kind", "changed_at", "value"])),
        }
    }
    Ok(SlotData::Value(WithRevision::new(
        required(changed_at, &object, "changed_at")?,
        required(value, &object, "value")?,
    )))
}

fn read_record<S>(
    registry: &SlotShapeRegistry,
    mut object: crate::slot_codec::ObjectReader<'_, '_, S>,
    shape: SlotShapeView<'_>,
) -> Result<SlotData, SyntaxError>
where
    S: SyntaxEventSource,
{
    let mut fields_revision = None;
    let mut field_data = None;
    while let Some(mut prop) = object.next_prop()? {
        match prop.name() {
            "fields_revision" => fields_revision = Some(read_revision(prop.value())?),
            "fields" => field_data = Some(read_fields(registry, prop.value(), shape)?),
            other => return Err(prop.unknown_field(other, &["kind", "fields_revision", "fields"])),
        }
    }
    Ok(SlotData::Record(SlotRecord::with_revision(
        required(fields_revision, &object, "fields_revision")?,
        required(field_data, &object, "fields")?,
    )))
}

fn read_fields<S>(
    registry: &SlotShapeRegistry,
    value: ValueReader<'_, '_, S>,
    shape: SlotShapeView<'_>,
) -> Result<Vec<SlotData>, SyntaxError>
where
    S: SyntaxEventSource,
{
    let fields_len = shape.record_fields_len().expect("record shape has fields");
    let mut items = Vec::new();
    let mut array = value.array()?;
    while let Some(item) = array.next_item()? {
        let index = items.len();
        let field = shape
            .record_field(index)
            .ok_or_else(|| error(format!("too many record fields: expected {fields_len}")))?;
        items.push(read_shape(registry, field.shape(), item)?);
    }
    if items.len() != fields_len {
        return Err(error(format!(
            "missing record fields: expected {}, found {}",
            fields_len,
            items.len()
        )));
    }
    Ok(items)
}

fn read_map<S>(
    registry: &SlotShapeRegistry,
    mut object: crate::slot_codec::ObjectReader<'_, '_, S>,
    key_shape: SlotMapKeyShape,
    value_shape: SlotShapeView<'_>,
) -> Result<SlotData, SyntaxError>
where
    S: SyntaxEventSource,
{
    let mut keys_revision = None;
    let mut entries = None;
    while let Some(mut prop) = object.next_prop()? {
        match prop.name() {
            "keys_revision" => keys_revision = Some(read_revision(prop.value())?),
            "entries" => {
                entries = Some(read_entries(
                    registry,
                    prop.value(),
                    key_shape,
                    value_shape,
                )?)
            }
            other => return Err(prop.unknown_field(other, &["kind", "keys_revision", "entries"])),
        }
    }
    Ok(SlotData::Map(SlotMapDyn::with_revision(
        required(keys_revision, &object, "keys_revision")?,
        required(entries, &object, "entries")?,
    )))
}

fn read_entries<S>(
    registry: &SlotShapeRegistry,
    value: ValueReader<'_, '_, S>,
    key_shape: SlotMapKeyShape,
    value_shape: SlotShapeView<'_>,
) -> Result<BTreeMap<SlotMapKey, SlotData>, SyntaxError>
where
    S: SyntaxEventSource,
{
    let mut entries = BTreeMap::new();
    let mut array = value.array()?;
    while let Some(item) = array.next_item()? {
        let (key, data) = read_entry(registry, item, key_shape, value_shape)?;
        entries.insert(key, data);
    }
    Ok(entries)
}

fn read_entry<S>(
    registry: &SlotShapeRegistry,
    value: ValueReader<'_, '_, S>,
    key_shape: SlotMapKeyShape,
    value_shape: SlotShapeView<'_>,
) -> Result<(SlotMapKey, SlotData), SyntaxError>
where
    S: SyntaxEventSource,
{
    let mut object = value.object()?;
    let mut key = None;
    let mut data = None;
    while let Some(mut prop) = object.next_prop()? {
        match prop.name() {
            "key" => key = Some(read_map_key(prop.value(), key_shape)?),
            "data" => data = Some(read_shape(registry, value_shape, prop.value())?),
            other => return Err(prop.unknown_field(other, &["key", "data"])),
        }
    }
    Ok((
        required(key, &object, "key")?,
        required(data, &object, "data")?,
    ))
}

fn read_enum<S>(
    registry: &SlotShapeRegistry,
    mut object: crate::slot_codec::ObjectReader<'_, '_, S>,
    shape: SlotShapeView<'_>,
) -> Result<SlotData, SyntaxError>
where
    S: SyntaxEventSource,
{
    let mut variant_revision = None;
    let mut variant_name = None;
    let mut data = None;
    while let Some(mut prop) = object.next_prop()? {
        match prop.name() {
            "variant_revision" => variant_revision = Some(read_revision(prop.value())?),
            "variant" => variant_name = Some(SlotName::parse(&prop.value().string()?)?),
            "data" => {
                let name = variant_name
                    .as_ref()
                    .ok_or_else(|| error("enum `variant` must appear before `data`"))?;
                let variant = shape
                    .enum_variant_by_name(name)
                    .ok_or_else(|| error(format!("unknown enum variant {name}")))?;
                data = Some(read_shape(registry, variant.shape(), prop.value())?);
            }
            other => {
                return Err(
                    prop.unknown_field(other, &["kind", "variant_revision", "variant", "data"])
                );
            }
        }
    }
    Ok(SlotData::Enum(SlotEnum::with_version(
        required(variant_revision, &object, "variant_revision")?,
        required(variant_name, &object, "variant")?,
        required(data, &object, "data")?,
    )))
}

fn read_option<S>(
    registry: &SlotShapeRegistry,
    mut object: crate::slot_codec::ObjectReader<'_, '_, S>,
    some_shape: SlotShapeView<'_>,
) -> Result<SlotData, SyntaxError>
where
    S: SyntaxEventSource,
{
    let mut presence_revision = None;
    let mut present = None;
    let mut data = None;
    while let Some(mut prop) = object.next_prop()? {
        match prop.name() {
            "presence_revision" => presence_revision = Some(read_revision(prop.value())?),
            "present" => present = Some(prop.value().bool()?),
            "data" => {
                if present == Some(false) {
                    return Err(error("option data was provided for a non-present option"));
                }
                data = Some(read_shape(registry, some_shape, prop.value())?);
            }
            other => {
                return Err(
                    prop.unknown_field(other, &["kind", "presence_revision", "present", "data"])
                );
            }
        }
    }
    let presence_revision = required(presence_revision, &object, "presence_revision")?;
    match required(present, &object, "present")? {
        true => Ok(SlotData::Option(SlotOptionDyn::some_with_version(
            presence_revision,
            required(data, &object, "data")?,
        ))),
        false => Ok(SlotData::Option(SlotOptionDyn::none_with_version(
            presence_revision,
        ))),
    }
}

fn read_map_key<S>(
    value: ValueReader<'_, '_, S>,
    shape: SlotMapKeyShape,
) -> Result<SlotMapKey, SyntaxError>
where
    S: SyntaxEventSource,
{
    match shape {
        SlotMapKeyShape::String => value.string().map(SlotMapKey::String),
        SlotMapKeyShape::I32 => value.i32().map(SlotMapKey::I32),
        SlotMapKeyShape::U32 => value.u32().map(SlotMapKey::U32),
    }
}

fn read_revision<S>(value: ValueReader<'_, '_, S>) -> Result<Revision, SyntaxError>
where
    S: SyntaxEventSource,
{
    Ok(Revision::new(value.i64()?))
}

fn shape_kind(shape: SlotShapeView<'_>) -> &'static str {
    if shape.is_unit() {
        "unit"
    } else if shape.value_shape().is_some() {
        "value"
    } else if shape.record_fields_len().is_some() {
        "record"
    } else if shape.map_value().is_some() {
        "map"
    } else if shape.is_enum() {
        "enum"
    } else if shape.option_some().is_some() {
        "option"
    } else {
        unreachable!("custom and ref shapes are projected before reading")
    }
}

fn required<T, S>(
    value: Option<T>,
    object: &crate::slot_codec::ObjectReader<'_, '_, S>,
    name: &str,
) -> Result<T, SyntaxError>
where
    S: SyntaxEventSource,
{
    value.ok_or_else(|| object.missing_required_field(name))
}

fn error(message: impl Into<String>) -> SyntaxError {
    SyntaxError::new(String::new(), None, message)
}

impl From<crate::SlotNameError> for SyntaxError {
    fn from(error: crate::SlotNameError) -> Self {
        self::error(error.to_string())
    }
}
