use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::{
    SlotDataMutAccess, SlotFactoryError, SlotMapKey, SlotMapKeyShape, SlotMutAccess,
    SlotMutationError, SlotShape, SlotShapeId, SlotShapeRegistry, SlotVariantShape,
    current_revision,
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
    let shape = registry
        .get(&shape_id)
        .ok_or_else(|| syntax_error(format!("missing slot shape: {shape_id}")))?;
    let mut object = registry
        .create_default(shape_id)
        .map_err(factory_error_to_syntax)?;
    apply_reader_to_slot(object.data_mut(), shape, registry, value)?;
    Ok(object)
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
            let shape = registry
                .get(id)
                .ok_or_else(|| syntax_error(format!("missing referenced slot shape: {id}")))?;
            apply_reader_to_slot(data, shape, registry, value)
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
        SlotShape::Enum { variants, .. } => read_enum(data, variants, registry, value),
        SlotShape::Option { some, .. } => read_option(data, some, registry, value),
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
    let SlotDataMutAccess::Map(map) = data else {
        value.skip_value()?;
        return Err(syntax_error("shape expected a map slot"));
    };
    let mut object = value.object()?;

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
    let mut object = value.object()?;
    let expected = variant_names(variants);
    let variant_name = object.expect_discriminator("kind", &expected)?;
    let variant = variants
        .iter()
        .find(|variant| variant.name.as_str() == variant_name)
        .ok_or_else(|| syntax_error("validated enum variant was not found"))?;

    en.set_variant_default_with_shape(current_revision(), &variant_name, registry, variants)
        .map_err(mutation_error_to_syntax)?;
    let data = en.data_mut();
    read_enum_payload_object(data, &variant.shape, registry, object)
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
            let shape = registry
                .get(id)
                .ok_or_else(|| syntax_error(format!("missing referenced slot shape: {id}")))?;
            read_enum_payload_object(data, shape, registry, object)
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
