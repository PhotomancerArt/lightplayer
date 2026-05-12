//! Direct JSON writer for borrowed slot data.

use lpc_model::{SlotDataAccess, SlotShape, SlotShapeId, SlotShapeRegistry};

use crate::json::json_write::JsonWrite;
use crate::json::json_writer::{JsonValue, JsonWriterError};

/// Write borrowed slot data in the same JSON shape as [`lpc_model::SlotData`].
///
/// This walks `SlotShape + SlotDataAccess` directly so firmware project reads do
/// not need to allocate an owned `SlotData` tree before serialization.
pub fn write_slot_data_json<W>(
    value: JsonValue<'_, W>,
    shape_id: &SlotShapeId,
    data: SlotDataAccess<'_>,
    registry: &SlotShapeRegistry,
) -> Result<(), JsonWriterError<W::Error>>
where
    W: JsonWrite,
{
    let shape = registry.get(shape_id).expect("slot shape is registered");
    write_slot_shape_data_json(value, shape, data, registry)
}

fn write_slot_shape_data_json<W>(
    value: JsonValue<'_, W>,
    shape: &SlotShape,
    data: SlotDataAccess<'_>,
    registry: &SlotShapeRegistry,
) -> Result<(), JsonWriterError<W::Error>>
where
    W: JsonWrite,
{
    match (shape, data) {
        (SlotShape::Ref { id }, data) => write_slot_data_json(value, id, data, registry),
        (SlotShape::Unit { .. }, SlotDataAccess::Unit(revision)) => {
            let mut object = value.object()?;
            object.prop("kind")?.string("unit")?;
            object.prop("revision")?.serde(&revision)?;
            object.finish()
        }
        (SlotShape::Value { .. }, SlotDataAccess::Value(slot_value)) => {
            let mut object = value.object()?;
            object.prop("kind")?.string("value")?;
            object.prop("value")?.serde(&slot_value.value())?;
            object.prop("changed_at")?.serde(&slot_value.changed_at())?;
            object.finish()
        }
        (SlotShape::Record { fields, .. }, SlotDataAccess::Record(record)) => {
            let mut object = value.object()?;
            object.prop("kind")?.string("record")?;
            object
                .prop("fields_revision")?
                .serde(&record.fields_revision())?;
            let mut field_values = object.prop("fields")?.array()?;
            for (index, field) in fields.iter().enumerate() {
                write_slot_shape_data_json(
                    field_values.item()?,
                    &field.shape,
                    record.field(index).expect("record field exists"),
                    registry,
                )?;
            }
            field_values.finish()?;
            object.finish()
        }
        (SlotShape::Map { value: item, .. }, SlotDataAccess::Map(map)) => {
            let mut object = value.object()?;
            object.prop("kind")?.string("map")?;
            object.prop("keys_revision")?.serde(&map.keys_revision())?;
            let mut entries = object.prop("entries")?.array()?;
            for key in map.keys() {
                let mut entry = entries.item()?.object()?;
                entry.prop("key")?.serde(&key)?;
                write_slot_shape_data_json(
                    entry.prop("data")?,
                    item,
                    map.get(&key).expect("map entry exists"),
                    registry,
                )?;
                entry.finish()?;
            }
            entries.finish()?;
            object.finish()
        }
        (SlotShape::Enum { variants, .. }, SlotDataAccess::Enum(en)) => {
            let variant = variants
                .iter()
                .find(|variant| variant.name.as_str() == en.variant())
                .expect("enum variant exists in shape");
            let mut object = value.object()?;
            object.prop("kind")?.string("enum")?;
            object
                .prop("variant_revision")?
                .serde(&en.variant_revision())?;
            object.prop("variant")?.string(en.variant())?;
            write_slot_shape_data_json(object.prop("data")?, &variant.shape, en.data(), registry)?;
            object.finish()
        }
        (SlotShape::Option { some, .. }, SlotDataAccess::Option(option)) => {
            let mut object = value.object()?;
            object.prop("kind")?.string("option")?;
            object
                .prop("presence_revision")?
                .serde(&option.presence_revision())?;
            match option.data() {
                Some(data) => {
                    write_slot_shape_data_json(object.prop("data")?, some, data, registry)?
                }
                None => object.prop("data")?.null()?,
            }
            object.finish()
        }
        _ => panic!("slot shape/data mismatch"),
    }
}
