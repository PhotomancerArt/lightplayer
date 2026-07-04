use alloc::format;
use alloc::string::{String, ToString};

use crate::{
    SlotAccess, SlotDataAccess, SlotEnumEncoding, SlotFieldShape, SlotMapKey, SlotShape,
    SlotShapeId, SlotShapeLookup, SlotShapeRegistry, SlotVariantShape,
};

use super::{SlotValueWriter, SlotWrite, SlotWriteError, SlotWriter, write_lp_value};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SlotDataWriteError {
    MissingShape(SlotShapeId),
    MissingReferencedShape(SlotShapeId),
    ShapeDataMismatch { message: String },
    UnknownVariant { variant: String },
    UnsupportedEnumPayload { message: String },
}

impl SlotDataWriteError {
    fn mismatch(message: impl Into<String>) -> Self {
        Self::ShapeDataMismatch {
            message: message.into(),
        }
    }

    fn unsupported_enum_payload(message: impl Into<String>) -> Self {
        Self::UnsupportedEnumPayload {
            message: message.into(),
        }
    }
}

impl core::fmt::Display for SlotDataWriteError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::MissingShape(id) => write!(f, "missing slot shape: {id}"),
            Self::MissingReferencedShape(id) => write!(f, "missing referenced slot shape: {id}"),
            Self::ShapeDataMismatch { message } => f.write_str(message),
            Self::UnknownVariant { variant } => {
                write!(f, "enum variant {variant:?} is missing from slot shape")
            }
            Self::UnsupportedEnumPayload { message } => f.write_str(message),
        }
    }
}

impl core::error::Error for SlotDataWriteError {}

pub fn write_dynamic_slot_json<W>(
    registry: &SlotShapeRegistry,
    root: &dyn SlotAccess,
    out: W,
) -> Result<W, SlotWriteError<W::Error>>
where
    W: SlotWrite,
{
    let mut writer = SlotWriter::new(out);
    write_slot_data_json_value(registry, root.shape_id(), root.data(), writer.value())?;
    Ok(writer.into_inner())
}

/// Pretty-printed variant of [`write_dynamic_slot_json`] for authored files.
pub fn write_dynamic_slot_json_pretty<W>(
    registry: &SlotShapeRegistry,
    root: &dyn SlotAccess,
    out: W,
) -> Result<W, SlotWriteError<W::Error>>
where
    W: SlotWrite,
{
    let mut writer = SlotWriter::new_pretty(out);
    write_slot_data_json_value(registry, root.shape_id(), root.data(), writer.value())?;
    Ok(writer.into_inner())
}

pub fn write_slot_data_json_value<W>(
    registry: &SlotShapeRegistry,
    id: SlotShapeId,
    data: SlotDataAccess<'_>,
    value: SlotValueWriter<'_, W>,
) -> Result<(), SlotWriteError<W::Error>>
where
    W: SlotWrite,
{
    let shape = registry
        .get_shape(id)
        .ok_or_else(|| json_data_error(SlotDataWriteError::MissingShape(id)))?
        .to_owned_shape();
    write_shape_json(value, &shape, data, registry)
}

fn write_shape_json<W>(
    value: SlotValueWriter<'_, W>,
    shape: &SlotShape,
    data: SlotDataAccess<'_>,
    registry: &SlotShapeRegistry,
) -> Result<(), SlotWriteError<W::Error>>
where
    W: SlotWrite,
{
    match shape {
        SlotShape::Ref { id } => {
            let shape = registry
                .get_shape(*id)
                .ok_or_else(|| json_data_error(SlotDataWriteError::MissingReferencedShape(*id)))?
                .to_owned_shape();
            write_shape_json(value, &shape, data, registry)
        }
        SlotShape::Unit { .. } => match data {
            SlotDataAccess::Unit(_) => value.object()?.finish(),
            other => Err(json_mismatch(format!(
                "slot shape expected unit data, got {}",
                data_kind(other)
            ))),
        },
        SlotShape::Value { shape } => match data {
            SlotDataAccess::Value(slot) => write_lp_value(value, &shape.ty, &slot.value()),
            other => Err(json_mismatch(format!(
                "slot shape expected value data, got {}",
                data_kind(other)
            ))),
        },
        SlotShape::Record { fields, .. } => match data {
            SlotDataAccess::Record(record) => {
                let mut object = value.object()?;
                write_record_fields_json(&mut object, fields, record, registry)?;
                object.finish()
            }
            other => Err(json_mismatch(format!(
                "slot shape expected record data, got {}",
                data_kind(other)
            ))),
        },
        SlotShape::Map {
            key: _,
            value: item_shape,
            ..
        } => match data {
            SlotDataAccess::Map(map) => {
                let mut object = value.object()?;
                for key in map.keys() {
                    let key_text = map_key_text(&key);
                    let item = map.get(&key).ok_or_else(|| {
                        json_mismatch(format!("map key {key_text:?} disappeared during write"))
                    })?;
                    write_shape_json(object.prop(&key_text)?, item_shape, item, registry)?;
                }
                object.finish()
            }
            other => Err(json_mismatch(format!(
                "slot shape expected map data, got {}",
                data_kind(other)
            ))),
        },
        SlotShape::Enum {
            encoding, variants, ..
        } => match data {
            SlotDataAccess::Enum(en) => write_enum_json(value, encoding, variants, en, registry),
            other => Err(json_mismatch(format!(
                "slot shape expected enum data, got {}",
                data_kind(other)
            ))),
        },
        SlotShape::Option { some, .. } => match data {
            SlotDataAccess::Option(option) => match option.data() {
                Some(data) => write_shape_json(value, some, data, registry),
                None => value.null(),
            },
            other => Err(json_mismatch(format!(
                "slot shape expected option data, got {}",
                data_kind(other)
            ))),
        },
        SlotShape::Custom { codec, .. } => match data {
            SlotDataAccess::Custom(custom) => {
                write_custom_slot_json(value, *codec, custom, registry)
            }
            other => Err(json_mismatch(format!(
                "slot shape expected custom data, got {}",
                data_kind(other)
            ))),
        },
    }
}

fn write_record_fields_json<W>(
    object: &mut super::SlotObjectWriter<'_, W>,
    fields: &[SlotFieldShape],
    record: &dyn crate::SlotRecordAccess,
    registry: &SlotShapeRegistry,
) -> Result<(), SlotWriteError<W::Error>>
where
    W: SlotWrite,
{
    for (index, field) in fields.iter().enumerate() {
        let data = record.field(index).ok_or_else(|| {
            json_mismatch(format!(
                "record data is missing field {:?}",
                field.name.as_str()
            ))
        })?;
        if should_omit_field(&field.shape, data, registry) {
            continue;
        }
        write_shape_json(
            object.prop(field.name.as_str())?,
            &field.shape,
            data,
            registry,
        )?;
    }
    Ok(())
}

fn write_enum_payload_json<W>(
    object: &mut super::SlotObjectWriter<'_, W>,
    shape: &SlotShape,
    data: SlotDataAccess<'_>,
    registry: &SlotShapeRegistry,
) -> Result<(), SlotWriteError<W::Error>>
where
    W: SlotWrite,
{
    match shape {
        SlotShape::Ref { id } => {
            let shape = registry
                .get_shape(*id)
                .ok_or_else(|| json_data_error(SlotDataWriteError::MissingReferencedShape(*id)))?
                .to_owned_shape();
            write_enum_payload_json(object, &shape, data, registry)
        }
        SlotShape::Record { fields, .. } => match data {
            SlotDataAccess::Record(record) => {
                write_record_fields_json(object, fields, record, registry)
            }
            other => Err(json_mismatch(format!(
                "enum record payload expected record data, got {}",
                data_kind(other)
            ))),
        },
        SlotShape::Unit { .. } => match data {
            SlotDataAccess::Unit(_) => Ok(()),
            other => Err(json_mismatch(format!(
                "enum unit payload expected unit data, got {}",
                data_kind(other)
            ))),
        },
        _ => Err(json_data_error(
            SlotDataWriteError::unsupported_enum_payload(
                "dynamic enum writer only supports record and unit variant payloads",
            ),
        )),
    }
}

fn write_enum_json<W>(
    value: SlotValueWriter<'_, W>,
    encoding: &SlotEnumEncoding,
    variants: &[SlotVariantShape],
    en: &dyn crate::SlotEnumAccess,
    registry: &SlotShapeRegistry,
) -> Result<(), SlotWriteError<W::Error>>
where
    W: SlotWrite,
{
    let variant_name = en.variant();
    let variant = find_variant(variants, variant_name).map_err(json_data_error)?;
    match encoding {
        SlotEnumEncoding::Tagged { field } => {
            let mut object = value.object()?;
            object.prop(field.as_str())?.string(variant_name)?;
            write_enum_payload_json(&mut object, &variant.shape, en.data(), registry)?;
            object.finish()
        }
        SlotEnumEncoding::External => {
            let mut object = value.object()?;
            write_shape_json(
                object.prop(variant_name)?,
                &variant.shape,
                en.data(),
                registry,
            )?;
            object.finish()
        }
    }
}

fn should_omit_field(
    shape: &SlotShape,
    data: SlotDataAccess<'_>,
    registry: &SlotShapeRegistry,
) -> bool {
    match (shape, data) {
        (SlotShape::Ref { id }, data) => registry
            .get_shape(*id)
            .is_some_and(|shape| should_omit_field(&shape.to_owned_shape(), data, registry)),
        (_, SlotDataAccess::Option(option)) => option.data().is_none(),
        (SlotShape::Record { fields, .. }, SlotDataAccess::Record(record)) => {
            fields.iter().enumerate().all(|(index, field)| {
                record
                    .field(index)
                    .is_some_and(|data| should_omit_field(&field.shape, data, registry))
            })
        }
        (SlotShape::Map { .. }, SlotDataAccess::Map(map)) => map.keys().is_empty(),
        (SlotShape::Unit { .. }, SlotDataAccess::Unit(_)) => true,
        _ => false,
    }
}

fn find_variant<'a>(
    variants: &'a [SlotVariantShape],
    variant_name: &str,
) -> Result<&'a SlotVariantShape, SlotDataWriteError> {
    variants
        .iter()
        .find(|variant| variant.name.as_str() == variant_name)
        .ok_or_else(|| SlotDataWriteError::UnknownVariant {
            variant: variant_name.to_string(),
        })
}

fn map_key_text(key: &SlotMapKey) -> String {
    match key {
        SlotMapKey::String(value) => value.clone(),
        SlotMapKey::I32(value) => value.to_string(),
        SlotMapKey::U32(value) => value.to_string(),
    }
}

fn json_data_error<E>(error: SlotDataWriteError) -> SlotWriteError<E> {
    SlotWriteError::InvalidSlotData(error.to_string())
}

fn json_mismatch<E>(message: impl Into<String>) -> SlotWriteError<E> {
    json_data_error(SlotDataWriteError::mismatch(message))
}

fn data_kind(data: SlotDataAccess<'_>) -> &'static str {
    match data {
        SlotDataAccess::Unit(_) => "unit",
        SlotDataAccess::Value(_) => "value",
        SlotDataAccess::Record(_) => "record",
        SlotDataAccess::Map(_) => "map",
        SlotDataAccess::Enum(_) => "enum",
        SlotDataAccess::Option(_) => "option",
        SlotDataAccess::Custom(_) => "custom",
    }
}

fn write_custom_slot_json<W>(
    value: SlotValueWriter<'_, W>,
    codec: SlotShapeId,
    data: &dyn crate::SlotCustomAccess,
    registry: &SlotShapeRegistry,
) -> Result<(), SlotWriteError<W::Error>>
where
    W: SlotWrite,
{
    if data.custom_codec_id() != codec {
        return Err(json_mismatch(format!(
            "slot data custom codec {} does not match shape codec {codec}",
            data.custom_codec_id()
        )));
    }
    crate::slot_codec::custom_slot_codec::write_custom_slot_json(codec, data, registry, value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        LpType, LpValue, Revision, SlotData, SlotMapDyn, SlotName, SlotOptionDyn, SlotRecord,
        SlotVariantShape, WithRevision,
        slot::shape::{enum_external, field, map, option, record, unit, value},
    };
    use alloc::vec;
    use alloc::vec::Vec;
    use lp_collection::VecMap;

    #[test]
    fn dynamic_slot_writer_writes_records_to_json() {
        let (registry, shape_id, data) = record_fixture();
        let json = write_json(&registry, shape_id, data.access());

        assert_eq!(json, r#"{"pin":18,"name":"main"}"#);
    }

    #[test]
    fn dynamic_slot_writer_writes_maps_to_json() {
        let shape_id = SlotShapeId::from_static_name("test.WriterMap");
        let mut registry = SlotShapeRegistry::default();
        registry
            .register_dynamic_shape(
                shape_id,
                record(vec![field(
                    "items",
                    map(crate::SlotMapKeyShape::String, value(LpType::U32)),
                )]),
            )
            .unwrap();
        let data = SlotData::Record(SlotRecord::new(vec![SlotData::Map(SlotMapDyn::new(
            VecMap::from([
                (
                    SlotMapKey::String("a".to_string()),
                    SlotData::Value(WithRevision::new(Revision::default(), LpValue::U32(1))),
                ),
                (
                    SlotMapKey::String("b".to_string()),
                    SlotData::Value(WithRevision::new(Revision::default(), LpValue::U32(2))),
                ),
            ]),
        ))]));

        let json = write_json(&registry, shape_id, data.access());

        assert_eq!(json, r#"{"items":{"a":1,"b":2}}"#);
    }

    #[test]
    fn dynamic_slot_writer_writes_enums_to_json() {
        let (registry, shape_id, data) = enum_fixture();
        let json = write_json(&registry, shape_id, data.access());

        assert_eq!(json, r#"{"kind":"square","size":0.5}"#);
    }

    #[test]
    fn dynamic_slot_writer_writes_external_value_enums_to_json() {
        let (registry, shape_id, data) = external_value_enum_fixture();
        let json = write_json(&registry, shape_id, data.access());

        assert_eq!(json, r#"{"file":"compute.glsl"}"#);
    }

    #[test]
    fn dynamic_slot_writer_writes_external_record_enums_to_json() {
        let (registry, shape_id, data) = external_record_enum_fixture();
        let json = write_json(&registry, shape_id, data.access());

        assert_eq!(json, r#"{"point":{"x":10,"y":11}}"#);
    }

    #[test]
    fn dynamic_slot_writer_writes_external_unit_enums_to_json() {
        let (registry, shape_id, data) = external_unit_enum_fixture();
        let json = write_json(&registry, shape_id, data.access());

        assert_eq!(json, r#"{"disabled":{}}"#);
    }

    #[test]
    fn dynamic_slot_writer_omits_none_and_empty_json_fields() {
        let shape_id = SlotShapeId::from_static_name("test.WriterOption");
        let mut registry = SlotShapeRegistry::default();
        registry
            .register_dynamic_shape(
                shape_id,
                record(vec![
                    field("name", option(value(LpType::String))),
                    field("empty", record(vec![])),
                ]),
            )
            .unwrap();
        let data = SlotData::Record(SlotRecord::new(vec![
            SlotData::Option(SlotOptionDyn::none_with_version(Revision::default())),
            SlotData::Record(SlotRecord::new(vec![])),
        ]));

        let json = write_json(&registry, shape_id, data.access());

        assert_eq!(json, "{}");
    }

    #[test]
    fn dynamic_slot_writer_writes_root_none_json() {
        let shape_id = SlotShapeId::from_static_name("test.WriterRootNone");
        let mut registry = SlotShapeRegistry::default();
        registry
            .register_dynamic_shape(shape_id, option(value(LpType::String)))
            .unwrap();
        let data = SlotData::Option(SlotOptionDyn::none_with_version(Revision::default()));

        let json = write_json(&registry, shape_id, data.access());

        assert_eq!(json, "null");
    }

    #[test]
    fn dynamic_slot_writer_resolves_refs_json() {
        let target_id = SlotShapeId::from_static_name("test.WriterRefTarget");
        let root_id = SlotShapeId::from_static_name("test.WriterRefRoot");
        let mut registry = SlotShapeRegistry::default();
        registry
            .register_dynamic_shape(target_id, value(LpType::Bool))
            .unwrap();
        registry
            .register_dynamic_shape(root_id, SlotShape::reference(target_id))
            .unwrap();
        let data = SlotData::Value(WithRevision::new(Revision::default(), LpValue::Bool(true)));

        let json = write_json(&registry, root_id, data.access());

        assert_eq!(json, "true");
    }

    #[test]
    fn dynamic_slot_writer_reports_shape_data_mismatch_json() {
        let shape_id = SlotShapeId::from_static_name("test.WriterMismatch");
        let mut registry = SlotShapeRegistry::default();
        registry
            .register_dynamic_shape(shape_id, value(LpType::Bool))
            .unwrap();
        let data = SlotData::Unit {
            revision: Revision::default(),
        };

        let mut out = Vec::new();
        let mut writer = SlotWriter::new(&mut out);
        let error = write_slot_data_json_value(&registry, shape_id, data.access(), writer.value())
            .unwrap_err();

        assert!(error.to_string().contains("expected value data"));
    }

    fn record_fixture() -> (SlotShapeRegistry, SlotShapeId, SlotData) {
        let shape_id = SlotShapeId::from_static_name("test.WriterRecord");
        let mut registry = SlotShapeRegistry::default();
        registry
            .register_dynamic_shape(
                shape_id,
                record(vec![
                    field("pin", value(LpType::U32)),
                    field("name", value(LpType::String)),
                ]),
            )
            .unwrap();
        let data = SlotData::Record(SlotRecord::new(vec![
            SlotData::Value(WithRevision::new(Revision::default(), LpValue::U32(18))),
            SlotData::Value(WithRevision::new(
                Revision::default(),
                LpValue::String("main".to_string()),
            )),
        ]));
        (registry, shape_id, data)
    }

    fn enum_fixture() -> (SlotShapeRegistry, SlotShapeId, SlotData) {
        let shape_id = SlotShapeId::from_static_name("test.WriterEnum");
        let mut registry = SlotShapeRegistry::default();
        registry
            .register_dynamic_shape(
                shape_id,
                SlotShape::Enum {
                    meta: crate::SlotMeta::empty(),
                    encoding: crate::SlotEnumEncoding::default(),
                    variants: vec![
                        SlotVariantShape::new("disabled", unit()).unwrap(),
                        SlotVariantShape::new(
                            "square",
                            record(vec![field("size", value(LpType::F32))]),
                        )
                        .unwrap(),
                    ],
                },
            )
            .unwrap();
        let data = SlotData::Enum(crate::SlotEnum::new(
            SlotName::parse("square").unwrap(),
            SlotData::Record(SlotRecord::new(vec![SlotData::Value(WithRevision::new(
                Revision::default(),
                LpValue::F32(0.5),
            ))])),
        ));
        (registry, shape_id, data)
    }

    fn external_value_enum_fixture() -> (SlotShapeRegistry, SlotShapeId, SlotData) {
        let shape_id = SlotShapeId::from_static_name("test.WriterExternalValueEnum");
        let mut registry = SlotShapeRegistry::default();
        registry
            .register_dynamic_shape(
                shape_id,
                enum_external(vec![
                    SlotVariantShape::new("file", value(LpType::String)).unwrap(),
                    SlotVariantShape::new("inline", value(LpType::String)).unwrap(),
                ]),
            )
            .unwrap();
        let data = SlotData::Enum(crate::SlotEnum::new(
            SlotName::parse("file").unwrap(),
            SlotData::Value(WithRevision::new(
                Revision::default(),
                LpValue::String("compute.glsl".to_string()),
            )),
        ));
        (registry, shape_id, data)
    }

    fn external_record_enum_fixture() -> (SlotShapeRegistry, SlotShapeId, SlotData) {
        let shape_id = SlotShapeId::from_static_name("test.WriterExternalRecordEnum");
        let mut registry = SlotShapeRegistry::default();
        registry
            .register_dynamic_shape(
                shape_id,
                enum_external(vec![
                    SlotVariantShape::new(
                        "point",
                        record(vec![
                            field("x", value(LpType::I32)),
                            field("y", value(LpType::I32)),
                        ]),
                    )
                    .unwrap(),
                ]),
            )
            .unwrap();
        let data = SlotData::Enum(crate::SlotEnum::new(
            SlotName::parse("point").unwrap(),
            SlotData::Record(SlotRecord::new(vec![
                SlotData::Value(WithRevision::new(Revision::default(), LpValue::I32(10))),
                SlotData::Value(WithRevision::new(Revision::default(), LpValue::I32(11))),
            ])),
        ));
        (registry, shape_id, data)
    }

    fn external_unit_enum_fixture() -> (SlotShapeRegistry, SlotShapeId, SlotData) {
        let shape_id = SlotShapeId::from_static_name("test.WriterExternalUnitEnum");
        let mut registry = SlotShapeRegistry::default();
        registry
            .register_dynamic_shape(
                shape_id,
                enum_external(vec![SlotVariantShape::new("disabled", unit()).unwrap()]),
            )
            .unwrap();
        let data = SlotData::Enum(crate::SlotEnum::new(
            SlotName::parse("disabled").unwrap(),
            SlotData::Unit {
                revision: Revision::default(),
            },
        ));
        (registry, shape_id, data)
    }

    fn write_json(
        registry: &SlotShapeRegistry,
        shape_id: SlotShapeId,
        data: SlotDataAccess<'_>,
    ) -> String {
        let mut out = Vec::new();
        let mut writer = SlotWriter::new(&mut out);
        write_slot_data_json_value(registry, shape_id, data, writer.value()).unwrap();
        String::from_utf8(out).unwrap()
    }
}
