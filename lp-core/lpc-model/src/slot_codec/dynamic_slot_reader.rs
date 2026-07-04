use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::{
    DynamicSlotObject, SlotData, SlotDataMutAccess, SlotFactoryError, SlotMapKey, SlotMapKeyShape,
    SlotMutAccess, SlotMutationError, SlotShape, SlotShapeId, SlotShapeLookup, SlotShapeRegistry,
    SlotVariantShape, create_dynamic_slot_data, current_revision,
};

use super::{ObjectReader, SyntaxError, SyntaxEventSource, ValueReader, read_lp_value};

pub fn read_dynamic_slot<S>(
    registry: &SlotShapeRegistry,
    shape_id: SlotShapeId,
    value: ValueReader<'_, '_, S>,
) -> Result<Box<dyn SlotMutAccess>, SyntaxError>
where
    S: SyntaxEventSource,
{
    let shape = owned_shape_for_id(registry, shape_id)?;
    let mut object = registry
        .create_default(shape_id)
        .map_err(factory_error_to_syntax)?;
    apply_reader_to_slot(object.data_mut(), &shape, registry, value)?;
    Ok(object)
}

pub fn read_dynamic_slot_from_object<S>(
    registry: &SlotShapeRegistry,
    shape_id: SlotShapeId,
    object: ObjectReader<'_, '_, S>,
) -> Result<Box<dyn SlotMutAccess>, SyntaxError>
where
    S: SyntaxEventSource,
{
    let shape = owned_shape_for_id(registry, shape_id)?;
    let mut slot = registry
        .create_default(shape_id)
        .map_err(factory_error_to_syntax)?;
    apply_object_reader_to_slot(slot.data_mut(), &shape, registry, object)?;
    Ok(slot)
}

pub fn read_dynamic_slot_data<S>(
    registry: &SlotShapeRegistry,
    shape_id: SlotShapeId,
    value: ValueReader<'_, '_, S>,
) -> Result<SlotData, SyntaxError>
where
    S: SyntaxEventSource,
{
    let shape = owned_shape_for_id(registry, shape_id)?;
    let data = create_dynamic_slot_data(registry, &shape).map_err(factory_error_to_syntax)?;
    let mut object = DynamicSlotObject::new(shape_id, data);
    apply_reader_to_slot(object.data_mut(), &shape, registry, value)?;
    Ok(object.into_data())
}

pub fn apply_reader_to_slot<S>(
    data: SlotDataMutAccess<'_>,
    shape: &SlotShape,
    registry: &SlotShapeRegistry,
    value: ValueReader<'_, '_, S>,
) -> Result<(), SyntaxError>
where
    S: SyntaxEventSource,
{
    match shape {
        SlotShape::Ref { id } => {
            let shape = owned_shape_for_id(registry, *id)
                .map_err(|_| syntax_error(format!("missing referenced slot shape: {id}")))?;
            apply_reader_to_slot(data, &shape, registry, value)
        }
        SlotShape::Unit { .. } => value.skip_value(),
        SlotShape::Value { shape } => read_value(data, &shape.ty, value),
        SlotShape::Record { fields, .. } => {
            let object = value.object()?;
            read_record_object(data, fields, registry, object)
        }
        SlotShape::Map {
            key, value: item, ..
        } => read_map(data, *key, item, registry, value),
        SlotShape::Enum {
            encoding, variants, ..
        } => read_enum(data, encoding, variants, registry, value),
        SlotShape::Option { some, .. } => read_option(data, some, registry, value),
        SlotShape::Custom { codec, .. } => read_custom_slot(data, *codec, registry, value),
    }
}

fn apply_object_reader_to_slot<S>(
    data: SlotDataMutAccess<'_>,
    shape: &SlotShape,
    registry: &SlotShapeRegistry,
    object: ObjectReader<'_, '_, S>,
) -> Result<(), SyntaxError>
where
    S: SyntaxEventSource,
{
    match shape {
        SlotShape::Ref { id } => {
            let shape = owned_shape_for_id(registry, *id)
                .map_err(|_| syntax_error(format!("missing referenced slot shape: {id}")))?;
            apply_object_reader_to_slot(data, &shape, registry, object)
        }
        SlotShape::Unit { .. } => {
            let SlotDataMutAccess::Unit(_) = data else {
                object.finish()?;
                return Err(syntax_error("shape expected a unit slot"));
            };
            object.finish()
        }
        SlotShape::Record { fields, .. } => read_record_object(data, fields, registry, object),
        SlotShape::Map {
            key, value: item, ..
        } => read_map_object(data, *key, item, registry, object),
        SlotShape::Enum {
            encoding, variants, ..
        } => {
            let SlotDataMutAccess::Enum(en) = data else {
                object.finish()?;
                return Err(syntax_error("shape expected an enum slot"));
            };
            match encoding {
                crate::SlotEnumEncoding::Tagged { field } => {
                    read_tagged_enum_object(en, field.as_str(), variants, registry, object)
                }
                crate::SlotEnumEncoding::External => {
                    read_external_enum_object(en, variants, registry, object)
                }
            }
        }
        SlotShape::Option { some, .. } => {
            let SlotDataMutAccess::Option(option) = data else {
                object.finish()?;
                return Err(syntax_error("shape expected an option slot"));
            };
            option
                .set_some_default(current_revision(), registry, some)
                .map_err(mutation_error_to_syntax)?;
            let Some(data) = option.data_mut() else {
                return Err(syntax_error(
                    "option default creation did not create a value",
                ));
            };
            apply_object_reader_to_slot(data, some, registry, object)
        }
        SlotShape::Value { .. } => {
            object.finish()?;
            Err(syntax_error("shape expected a scalar value, found object"))
        }
        SlotShape::Custom { .. } => {
            object.finish()?;
            Err(syntax_error(
                "custom slot reader cannot start from an already-open object",
            ))
        }
    }
}

fn read_value<S>(
    data: SlotDataMutAccess<'_>,
    ty: &crate::LpType,
    value: ValueReader<'_, '_, S>,
) -> Result<(), SyntaxError>
where
    S: SyntaxEventSource,
{
    let SlotDataMutAccess::Value(slot) = data else {
        value.skip_value()?;
        return Err(syntax_error("shape expected a value slot"));
    };
    let value = read_lp_value(ty, value)?;
    slot.set_lp_value(current_revision(), value)
        .map_err(mutation_error_to_syntax)
}

fn read_record_object<S>(
    data: SlotDataMutAccess<'_>,
    fields: &[crate::SlotFieldShape],
    registry: &SlotShapeRegistry,
    mut object: ObjectReader<'_, '_, S>,
) -> Result<(), SyntaxError>
where
    S: SyntaxEventSource,
{
    let SlotDataMutAccess::Record(record) = data else {
        object.finish()?;
        return Err(syntax_error("shape expected a record slot"));
    };
    let expected = field_names(fields);

    while let Some(mut prop) = object.next_prop()? {
        let Some(index) = fields
            .iter()
            .position(|field| field.name.as_str() == prop.name())
        else {
            return Err(prop.unknown_field(prop.name(), &expected));
        };
        let Some(field_data) = record.field_mut(index) else {
            return Err(syntax_error(format!(
                "record slot is missing field {:?}",
                fields[index].name.as_str()
            )));
        };
        apply_reader_to_slot(field_data, &fields[index].shape, registry, prop.value())?;
    }

    Ok(())
}

fn read_map<S>(
    data: SlotDataMutAccess<'_>,
    key_shape: SlotMapKeyShape,
    item_shape: &SlotShape,
    registry: &SlotShapeRegistry,
    value: ValueReader<'_, '_, S>,
) -> Result<(), SyntaxError>
where
    S: SyntaxEventSource,
{
    let object = value.object()?;
    read_map_object(data, key_shape, item_shape, registry, object)
}

fn read_map_object<S>(
    data: SlotDataMutAccess<'_>,
    key_shape: SlotMapKeyShape,
    item_shape: &SlotShape,
    registry: &SlotShapeRegistry,
    mut object: ObjectReader<'_, '_, S>,
) -> Result<(), SyntaxError>
where
    S: SyntaxEventSource,
{
    let SlotDataMutAccess::Map(map) = data else {
        object.finish()?;
        return Err(syntax_error("shape expected a map slot"));
    };
    while let Some(mut prop) = object.next_prop()? {
        let key = parse_map_key(key_shape, prop.name())
            .map_err(|message| prop.unknown_field(prop.name(), &[message.as_str()]))?;
        map.insert_default(current_revision(), &key, registry, item_shape)
            .map_err(mutation_error_to_syntax)?;
        let Some(item_data) = map.get_mut(&key) else {
            return Err(syntax_error(
                "map default insertion did not create an entry",
            ));
        };
        apply_reader_to_slot(item_data, item_shape, registry, prop.value())?;
    }

    Ok(())
}

fn read_enum<S>(
    data: SlotDataMutAccess<'_>,
    encoding: &crate::SlotEnumEncoding,
    variants: &[SlotVariantShape],
    registry: &SlotShapeRegistry,
    value: ValueReader<'_, '_, S>,
) -> Result<(), SyntaxError>
where
    S: SyntaxEventSource,
{
    let SlotDataMutAccess::Enum(en) = data else {
        value.skip_value()?;
        return Err(syntax_error("shape expected an enum slot"));
    };
    match encoding {
        crate::SlotEnumEncoding::Tagged { field } => {
            read_tagged_enum(en, field.as_str(), variants, registry, value)
        }
        crate::SlotEnumEncoding::External => read_external_enum(en, variants, registry, value),
    }
}

fn read_tagged_enum<S>(
    en: &mut dyn crate::SlotEnumDefaultVariant,
    discriminator: &str,
    variants: &[SlotVariantShape],
    registry: &SlotShapeRegistry,
    value: ValueReader<'_, '_, S>,
) -> Result<(), SyntaxError>
where
    S: SyntaxEventSource,
{
    let object = value.object()?;
    read_tagged_enum_object(en, discriminator, variants, registry, object)
}

fn read_tagged_enum_object<S>(
    en: &mut dyn crate::SlotEnumDefaultVariant,
    discriminator: &str,
    variants: &[SlotVariantShape],
    registry: &SlotShapeRegistry,
    mut object: ObjectReader<'_, '_, S>,
) -> Result<(), SyntaxError>
where
    S: SyntaxEventSource,
{
    let expected = variant_names(variants);
    let variant_name = object.expect_discriminator(discriminator, &expected)?;
    let variant = variants
        .iter()
        .find(|variant| variant.name.as_str() == variant_name)
        .ok_or_else(|| syntax_error("validated enum variant was not found"))?;

    en.set_variant_default_with_shape(current_revision(), &variant_name, registry, variants)
        .map_err(mutation_error_to_syntax)?;
    let data = en.data_mut();
    read_enum_payload_object(data, &variant.shape, registry, object)
}

fn read_external_enum<S>(
    en: &mut dyn crate::SlotEnumDefaultVariant,
    variants: &[SlotVariantShape],
    registry: &SlotShapeRegistry,
    value: ValueReader<'_, '_, S>,
) -> Result<(), SyntaxError>
where
    S: SyntaxEventSource,
{
    let object = value.object()?;
    read_external_enum_object(en, variants, registry, object)
}

fn read_external_enum_object<S>(
    en: &mut dyn crate::SlotEnumDefaultVariant,
    variants: &[SlotVariantShape],
    registry: &SlotShapeRegistry,
    mut object: ObjectReader<'_, '_, S>,
) -> Result<(), SyntaxError>
where
    S: SyntaxEventSource,
{
    let expected = variant_names(variants);
    let Some(mut prop) = object.next_prop()? else {
        return Err(syntax_error(format!(
            "external enum expected exactly one variant property. Expected one of: {}.",
            expected.join(", ")
        )));
    };
    let variant_name = prop.name().to_string();
    let variant = variants
        .iter()
        .find(|variant| variant.name.as_str() == variant_name)
        .ok_or_else(|| prop.unknown_field(&variant_name, &expected))?;

    en.set_variant_default_with_shape(current_revision(), &variant_name, registry, variants)
        .map_err(mutation_error_to_syntax)?;
    let data = en.data_mut();
    apply_reader_to_slot(data, &variant.shape, registry, prop.value())?;
    drop(prop);

    if let Some(prop) = object.next_prop()? {
        let name = prop.name().to_string();
        return Err(prop.unknown_field(&name, &[]));
    }
    Ok(())
}

fn read_enum_payload_object<S>(
    data: SlotDataMutAccess<'_>,
    shape: &SlotShape,
    registry: &SlotShapeRegistry,
    object: ObjectReader<'_, '_, S>,
) -> Result<(), SyntaxError>
where
    S: SyntaxEventSource,
{
    match shape {
        SlotShape::Ref { id } => {
            let shape = owned_shape_for_id(registry, *id)
                .map_err(|_| syntax_error(format!("missing referenced slot shape: {id}")))?;
            read_enum_payload_object(data, &shape, registry, object)
        }
        SlotShape::Record { fields, .. } => read_record_object(data, fields, registry, object),
        SlotShape::Unit { .. } => object.finish(),
        _ => Err(syntax_error(
            "dynamic enum reader only supports record and unit variant payloads",
        )),
    }
}

fn read_option<S>(
    data: SlotDataMutAccess<'_>,
    some_shape: &SlotShape,
    registry: &SlotShapeRegistry,
    value: ValueReader<'_, '_, S>,
) -> Result<(), SyntaxError>
where
    S: SyntaxEventSource,
{
    let SlotDataMutAccess::Option(option) = data else {
        value.skip_value()?;
        return Err(syntax_error("shape expected an option slot"));
    };
    option
        .set_some_default(current_revision(), registry, some_shape)
        .map_err(mutation_error_to_syntax)?;
    let Some(data) = option.data_mut() else {
        return Err(syntax_error(
            "option default creation did not create a value",
        ));
    };
    apply_reader_to_slot(data, some_shape, registry, value)
}

fn read_custom_slot<S>(
    data: SlotDataMutAccess<'_>,
    codec: SlotShapeId,
    registry: &SlotShapeRegistry,
    value: ValueReader<'_, '_, S>,
) -> Result<(), SyntaxError>
where
    S: SyntaxEventSource,
{
    let SlotDataMutAccess::Custom(custom) = data else {
        value.skip_value()?;
        return Err(syntax_error(format!(
            "shape expected custom slot codec {codec}"
        )));
    };
    if custom.custom_codec_id() != codec {
        value.skip_value()?;
        return Err(syntax_error(format!(
            "slot data custom codec {} does not match shape codec {codec}",
            custom.custom_codec_id()
        )));
    }
    crate::slot_codec::custom_slot_codec::read_custom_slot(codec, custom, registry, value)
}

fn parse_map_key(shape: SlotMapKeyShape, raw: &str) -> Result<SlotMapKey, String> {
    match shape {
        SlotMapKeyShape::String => Ok(SlotMapKey::String(String::from(raw))),
        SlotMapKeyShape::I32 => raw
            .parse::<i32>()
            .map(SlotMapKey::I32)
            .map_err(|_| String::from("i32 map key")),
        SlotMapKeyShape::U32 => raw
            .parse::<u32>()
            .map(SlotMapKey::U32)
            .map_err(|_| String::from("u32 map key")),
    }
}

fn field_names(fields: &[crate::SlotFieldShape]) -> Vec<&str> {
    fields.iter().map(|field| field.name.as_str()).collect()
}

fn variant_names(variants: &[SlotVariantShape]) -> Vec<&str> {
    variants
        .iter()
        .map(|variant| variant.name.as_str())
        .collect()
}

fn owned_shape_for_id(
    registry: &SlotShapeRegistry,
    id: SlotShapeId,
) -> Result<SlotShape, SyntaxError> {
    registry
        .get_shape(id)
        .map(|shape| shape.to_owned_shape())
        .ok_or_else(|| syntax_error(format!("missing slot shape: {id}")))
}

fn factory_error_to_syntax(error: SlotFactoryError) -> SyntaxError {
    syntax_error(error.to_string())
}

fn mutation_error_to_syntax(error: SlotMutationError) -> SyntaxError {
    syntax_error(error.to_string())
}

fn syntax_error(message: impl Into<String>) -> SyntaxError {
    SyntaxError::new("", None, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        LpType, LpValue, SlotDataAccess, SlotMapKeyShape, SlotShapeRegistry, SlotVariantShape,
        slot::shape,
        slot_codec::{JsonSyntaxSource, SlotReader},
    };
    use alloc::vec;

    #[test]
    fn dynamic_slot_reader_reads_record_values() {
        let shape_id = crate::SlotShapeId::from_static_name("test.DynamicRecord");
        let mut registry = SlotShapeRegistry::default();
        registry
            .register_dynamic_shape(
                shape_id,
                shape::record(vec![
                    shape::field("pin", shape::value(LpType::U32)),
                    shape::field("name", shape::value(LpType::String)),
                ]),
            )
            .unwrap();

        let object = read_json(&registry, shape_id, r#"{"pin":18,"name":"main"}"#);

        let SlotDataAccess::Record(record) = object.data() else {
            panic!("expected record");
        };
        assert_eq!(record_value(record, 0), LpValue::U32(18));
        assert_eq!(
            record_value(record, 1),
            LpValue::String(String::from("main"))
        );
    }

    #[test]
    fn dynamic_slot_reader_leaves_missing_fields_at_defaults() {
        let shape_id = crate::SlotShapeId::from_static_name("test.DynamicDefaults");
        let mut registry = SlotShapeRegistry::default();
        registry
            .register_dynamic_shape(
                shape_id,
                shape::record(vec![
                    shape::field("pin", shape::value(LpType::U32)),
                    shape::field("name", shape::value(LpType::String)),
                ]),
            )
            .unwrap();

        let object = read_json(&registry, shape_id, r#"{"name":"main"}"#);

        let SlotDataAccess::Record(record) = object.data() else {
            panic!("expected record");
        };
        assert_eq!(record_value(record, 0), LpValue::U32(0));
        assert_eq!(
            record_value(record, 1),
            LpValue::String(String::from("main"))
        );
    }

    #[test]
    fn dynamic_slot_reader_reads_maps() {
        let shape_id = crate::SlotShapeId::from_static_name("test.DynamicMap");
        let mut registry = SlotShapeRegistry::default();
        registry
            .register_dynamic_shape(
                shape_id,
                shape::record(vec![shape::field(
                    "items",
                    SlotShape::Map {
                        meta: crate::SlotMeta::empty(),
                        key: SlotMapKeyShape::String,
                        value: Box::new(shape::value(LpType::U32)),
                    },
                )]),
            )
            .unwrap();

        let object = read_json(&registry, shape_id, r#"{"items":{"a":1,"b":2}}"#);

        let SlotDataAccess::Record(record) = object.data() else {
            panic!("expected record");
        };
        let SlotDataAccess::Map(map) = record.field(0).unwrap() else {
            panic!("expected map");
        };
        assert_eq!(
            map.get(&SlotMapKey::String(String::from("a")))
                .and_then(slot_value),
            Some(LpValue::U32(1))
        );
    }

    #[test]
    fn dynamic_slot_reader_reads_enums() {
        let shape_id = crate::SlotShapeId::from_static_name("test.DynamicEnum");
        let mut registry = SlotShapeRegistry::default();
        registry
            .register_dynamic_shape(
                shape_id,
                SlotShape::Enum {
                    meta: crate::SlotMeta::empty(),
                    encoding: crate::SlotEnumEncoding::default(),
                    variants: vec![
                        SlotVariantShape::new(
                            "square",
                            shape::record(vec![shape::field("size", shape::value(LpType::F32))]),
                        )
                        .unwrap(),
                    ],
                },
            )
            .unwrap();

        let object = read_json(&registry, shape_id, r#"{"kind":"square","size":0.5}"#);

        let SlotDataAccess::Enum(en) = object.data() else {
            panic!("expected enum");
        };
        assert_eq!(en.variant(), "square");
        let SlotDataAccess::Record(record) = en.data() else {
            panic!("expected enum payload record");
        };
        assert_eq!(record_value(record, 0), LpValue::F32(0.5));
    }

    #[test]
    fn dynamic_slot_reader_reads_external_value_enums() {
        let shape_id = crate::SlotShapeId::from_static_name("test.DynamicExternalValueEnum");
        let mut registry = SlotShapeRegistry::default();
        registry
            .register_dynamic_shape(
                shape_id,
                shape::enum_external(vec![
                    SlotVariantShape::new("file", shape::value(LpType::String)).unwrap(),
                    SlotVariantShape::new("inline", shape::value(LpType::String)).unwrap(),
                ]),
            )
            .unwrap();

        let object = read_json(&registry, shape_id, r#"{"file":"compute.glsl"}"#);

        let SlotDataAccess::Enum(en) = object.data() else {
            panic!("expected enum");
        };
        assert_eq!(en.variant(), "file");
        assert_eq!(
            slot_value(en.data()),
            Some(LpValue::String(String::from("compute.glsl")))
        );
    }

    #[test]
    fn dynamic_slot_reader_reads_external_record_enums_from_json() {
        let shape_id = crate::SlotShapeId::from_static_name("test.DynamicExternalRecordEnum");
        let mut registry = SlotShapeRegistry::default();
        registry
            .register_dynamic_shape(
                shape_id,
                shape::enum_external(vec![
                    SlotVariantShape::new(
                        "point",
                        shape::record(vec![
                            shape::field("x", shape::value(LpType::I32)),
                            shape::field("y", shape::value(LpType::I32)),
                        ]),
                    )
                    .unwrap(),
                ]),
            )
            .unwrap();
        let object = read_json(&registry, shape_id, r#"{"point":{"x":10,"y":11}}"#);

        let SlotDataAccess::Enum(en) = object.data() else {
            panic!("expected enum");
        };
        assert_eq!(en.variant(), "point");
        let SlotDataAccess::Record(record) = en.data() else {
            panic!("expected record payload");
        };
        assert_eq!(record_value(record, 0), LpValue::I32(10));
        assert_eq!(record_value(record, 1), LpValue::I32(11));
    }

    #[test]
    fn dynamic_slot_reader_rejects_external_enum_without_variant_property() {
        let (registry, shape_id) = external_unit_enum_registry("test.DynamicExternalEmpty");
        let mut reader = SlotReader::new(JsonSyntaxSource::new(r#"{}"#).unwrap(), &registry);

        let Err(error) = read_dynamic_slot(&registry, shape_id, reader.value()) else {
            panic!("expected external enum empty error");
        };

        assert!(error.message().contains("exactly one variant property"));
    }

    #[test]
    fn dynamic_slot_reader_rejects_external_enum_with_multiple_variant_properties() {
        let (registry, shape_id) = external_unit_enum_registry("test.DynamicExternalMany");
        let mut reader = SlotReader::new(
            JsonSyntaxSource::new(r#"{"a":{},"b":{}}"#).unwrap(),
            &registry,
        );

        let Err(error) = read_dynamic_slot(&registry, shape_id, reader.value()) else {
            panic!("expected external enum multiple property error");
        };

        assert!(error.message().contains("b"));
    }

    #[test]
    fn dynamic_slot_reader_rejects_unknown_external_enum_variant() {
        let (registry, shape_id) = external_unit_enum_registry("test.DynamicExternalUnknown");
        let mut reader = SlotReader::new(
            JsonSyntaxSource::new(r#"{"missing":{}}"#).unwrap(),
            &registry,
        );

        let Err(error) = read_dynamic_slot(&registry, shape_id, reader.value()) else {
            panic!("expected external enum unknown variant error");
        };

        assert!(error.message().contains("missing"));
        assert!(error.message().contains("a"));
    }

    #[test]
    fn dynamic_slot_reader_reports_unknown_fields() {
        let shape_id = crate::SlotShapeId::from_static_name("test.DynamicUnknown");
        let mut registry = SlotShapeRegistry::default();
        registry
            .register_dynamic_shape(
                shape_id,
                shape::record(vec![shape::field("pin", shape::value(LpType::U32))]),
            )
            .unwrap();
        let mut reader = SlotReader::new(
            JsonSyntaxSource::new(r#"{"surprise":18}"#).unwrap(),
            &registry,
        );

        let Err(error) = read_dynamic_slot(&registry, shape_id, reader.value()) else {
            panic!("expected unknown field error");
        };

        assert!(error.message().contains("surprise"));
        assert!(error.message().contains("pin"));
    }

    fn read_json(
        registry: &SlotShapeRegistry,
        shape_id: crate::SlotShapeId,
        json: &str,
    ) -> Box<dyn SlotMutAccess> {
        let mut reader = SlotReader::new(JsonSyntaxSource::new(json).unwrap(), registry);
        read_dynamic_slot(registry, shape_id, reader.value()).unwrap()
    }

    fn external_unit_enum_registry(name: &str) -> (SlotShapeRegistry, crate::SlotShapeId) {
        let shape_id = crate::SlotShapeId::from_static_name(name);
        let mut registry = SlotShapeRegistry::default();
        registry
            .register_dynamic_shape(
                shape_id,
                shape::enum_external(vec![
                    SlotVariantShape::new("a", shape::unit()).unwrap(),
                    SlotVariantShape::new("b", shape::unit()).unwrap(),
                ]),
            )
            .unwrap();
        (registry, shape_id)
    }

    fn record_value(record: &dyn crate::SlotRecordAccess, index: usize) -> LpValue {
        record.field(index).and_then(slot_value).unwrap()
    }

    fn slot_value(data: SlotDataAccess<'_>) -> Option<LpValue> {
        match data {
            SlotDataAccess::Value(value) => Some(value.value()),
            _ => None,
        }
    }
}
