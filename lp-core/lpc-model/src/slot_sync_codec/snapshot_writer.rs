use alloc::format;

use crate::slot_codec::custom_slot_codec::snapshot_custom_slot_data;
use crate::slot_codec::{SlotValueWriter, SlotWrite, SlotWriteError, SlotWriter, write_lp_value};
use crate::{
    SlotDataAccess, SlotMapKey, SlotMapKeyShape, SlotShape, SlotShapeId, SlotShapeRegistry,
};

pub fn write_slot_snapshot_json<W>(
    registry: &SlotShapeRegistry,
    id: SlotShapeId,
    data: SlotDataAccess<'_>,
    out: W,
) -> Result<W, SlotWriteError<W::Error>>
where
    W: SlotWrite,
{
    let mut writer = SlotWriter::new(out);
    write_slot_snapshot_value(registry, id, data, writer.value())?;
    Ok(writer.into_inner())
}

pub fn write_slot_snapshot_value<W>(
    registry: &SlotShapeRegistry,
    id: SlotShapeId,
    data: SlotDataAccess<'_>,
    value: SlotValueWriter<'_, W>,
) -> Result<(), SlotWriteError<W::Error>>
where
    W: SlotWrite,
{
    let shape = registry
        .get(&id)
        .ok_or_else(|| invalid_slot_data(format!("missing slot shape: {id}")))?;
    write_shape(registry, shape, data, value)
}

pub fn write_slot_snapshot_shape_value<W>(
    registry: &SlotShapeRegistry,
    shape: &SlotShape,
    data: SlotDataAccess<'_>,
    value: SlotValueWriter<'_, W>,
) -> Result<(), SlotWriteError<W::Error>>
where
    W: SlotWrite,
{
    write_shape(registry, shape, data, value)
}

fn write_shape<W>(
    registry: &SlotShapeRegistry,
    shape: &SlotShape,
    data: SlotDataAccess<'_>,
    value: SlotValueWriter<'_, W>,
) -> Result<(), SlotWriteError<W::Error>>
where
    W: SlotWrite,
{
    match shape {
        SlotShape::Ref { id } => write_slot_snapshot_value(registry, *id, data, value),
        SlotShape::Unit { .. } => write_unit(data, value),
        SlotShape::Value { shape } => write_value(&shape.ty, data, value),
        SlotShape::Record { fields, .. } => write_record(registry, fields, data, value),
        SlotShape::Map {
            key, value: shape, ..
        } => write_map(registry, *key, shape, data, value),
        SlotShape::Enum { variants, .. } => write_enum(registry, variants, data, value),
        SlotShape::Option { some, .. } => write_option(registry, some, data, value),
        SlotShape::Custom { codec, shape, .. } => {
            write_custom(registry, *codec, shape, data, value)
        }
    }
}

fn write_custom<W>(
    registry: &SlotShapeRegistry,
    codec: SlotShapeId,
    shape: &SlotShape,
    data: SlotDataAccess<'_>,
    value: SlotValueWriter<'_, W>,
) -> Result<(), SlotWriteError<W::Error>>
where
    W: SlotWrite,
{
    let SlotDataAccess::Custom(custom) = data else {
        return Err(invalid_slot_data("slot data does not match custom shape"));
    };
    let data = snapshot_custom_slot_data(codec, custom).map_err(invalid_slot_data)?;
    write_shape(registry, shape, data, value)
}

fn write_unit<W>(
    data: SlotDataAccess<'_>,
    value: SlotValueWriter<'_, W>,
) -> Result<(), SlotWriteError<W::Error>>
where
    W: SlotWrite,
{
    let SlotDataAccess::Unit(revision) = data else {
        return Err(invalid_slot_data("slot data does not match unit shape"));
    };
    let mut object = value.object()?;
    object.prop("kind")?.string("unit")?;
    object.prop("revision")?.i64(revision.as_i64())?;
    object.finish()
}

fn write_value<W>(
    ty: &crate::LpType,
    data: SlotDataAccess<'_>,
    value: SlotValueWriter<'_, W>,
) -> Result<(), SlotWriteError<W::Error>>
where
    W: SlotWrite,
{
    let SlotDataAccess::Value(slot_value) = data else {
        return Err(invalid_slot_data("slot data does not match value shape"));
    };
    let lp_value = slot_value.value();
    let mut object = value.object()?;
    object.prop("kind")?.string("value")?;
    object
        .prop("changed_at")?
        .i64(slot_value.changed_at().as_i64())?;
    write_lp_value(object.prop("value")?, ty, &lp_value)?;
    object.finish()
}

fn write_record<W>(
    registry: &SlotShapeRegistry,
    fields: &[crate::SlotFieldShape],
    data: SlotDataAccess<'_>,
    value: SlotValueWriter<'_, W>,
) -> Result<(), SlotWriteError<W::Error>>
where
    W: SlotWrite,
{
    let SlotDataAccess::Record(record) = data else {
        return Err(invalid_slot_data("slot data does not match record shape"));
    };
    let mut object = value.object()?;
    object.prop("kind")?.string("record")?;
    object
        .prop("fields_revision")?
        .i64(record.fields_revision().as_i64())?;
    let mut field_values = object.prop("fields")?.array()?;
    for (index, field) in fields.iter().enumerate() {
        let field_data = record
            .field(index)
            .ok_or_else(|| invalid_slot_data(format!("missing record field {}", field.name)))?;
        write_shape(registry, &field.shape, field_data, field_values.item()?)?;
    }
    field_values.finish()?;
    object.finish()
}

fn write_map<W>(
    registry: &SlotShapeRegistry,
    key_shape: SlotMapKeyShape,
    value_shape: &SlotShape,
    data: SlotDataAccess<'_>,
    value: SlotValueWriter<'_, W>,
) -> Result<(), SlotWriteError<W::Error>>
where
    W: SlotWrite,
{
    let SlotDataAccess::Map(map) = data else {
        return Err(invalid_slot_data("slot data does not match map shape"));
    };
    let mut object = value.object()?;
    object.prop("kind")?.string("map")?;
    object
        .prop("keys_revision")?
        .i64(map.keys_revision().as_i64())?;
    let mut entries = object.prop("entries")?.array()?;
    for key in map.keys() {
        let entry_data = map
            .get(&key)
            .ok_or_else(|| invalid_slot_data("missing map entry data"))?;
        let mut entry = entries.item()?.object()?;
        write_map_key(entry.prop("key")?, key_shape, &key)?;
        write_shape(registry, value_shape, entry_data, entry.prop("data")?)?;
        entry.finish()?;
    }
    entries.finish()?;
    object.finish()
}

fn write_enum<W>(
    registry: &SlotShapeRegistry,
    variants: &[crate::SlotVariantShape],
    data: SlotDataAccess<'_>,
    value: SlotValueWriter<'_, W>,
) -> Result<(), SlotWriteError<W::Error>>
where
    W: SlotWrite,
{
    let SlotDataAccess::Enum(en) = data else {
        return Err(invalid_slot_data("slot data does not match enum shape"));
    };
    let variant = variants
        .iter()
        .find(|variant| variant.name.as_str() == en.variant())
        .ok_or_else(|| invalid_slot_data(format!("unknown enum variant {}", en.variant())))?;
    let mut object = value.object()?;
    object.prop("kind")?.string("enum")?;
    object
        .prop("variant_revision")?
        .i64(en.variant_revision().as_i64())?;
    object.prop("variant")?.string(en.variant())?;
    write_shape(registry, &variant.shape, en.data(), object.prop("data")?)?;
    object.finish()
}

fn write_option<W>(
    registry: &SlotShapeRegistry,
    some_shape: &SlotShape,
    data: SlotDataAccess<'_>,
    value: SlotValueWriter<'_, W>,
) -> Result<(), SlotWriteError<W::Error>>
where
    W: SlotWrite,
{
    let SlotDataAccess::Option(option) = data else {
        return Err(invalid_slot_data("slot data does not match option shape"));
    };
    let mut object = value.object()?;
    object.prop("kind")?.string("option")?;
    object
        .prop("presence_revision")?
        .i64(option.presence_revision().as_i64())?;
    match option.data() {
        Some(data) => {
            object.prop("present")?.bool(true)?;
            write_shape(registry, some_shape, data, object.prop("data")?)?;
        }
        None => {
            object.prop("present")?.bool(false)?;
        }
    }
    object.finish()
}

fn write_map_key<W>(
    value: SlotValueWriter<'_, W>,
    shape: SlotMapKeyShape,
    key: &SlotMapKey,
) -> Result<(), SlotWriteError<W::Error>>
where
    W: SlotWrite,
{
    match (shape, key) {
        (SlotMapKeyShape::String, SlotMapKey::String(value_key)) => value.string(value_key),
        (SlotMapKeyShape::I32, SlotMapKey::I32(value_key)) => value.i32(*value_key),
        (SlotMapKeyShape::U32, SlotMapKey::U32(value_key)) => value.u32(*value_key),
        _ => Err(invalid_slot_data("map key does not match map key shape")),
    }
}

fn invalid_slot_data<E>(message: impl Into<alloc::string::String>) -> SlotWriteError<E> {
    SlotWriteError::InvalidSlotData(message.into())
}
