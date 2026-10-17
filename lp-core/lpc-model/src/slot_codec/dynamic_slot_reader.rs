//! Read authored JSON into slot data, walking the slot's shape.
//!
//! The walk is over [`SlotShapeView`]s: a static catalog shape is read where
//! it lies, never copied into an owned [`SlotShape`] tree first. Only the
//! sub-shape a default-creating mutation needs (an option's `some`, a map's
//! item, the chosen enum variant) is made owned, at the moment it is needed —
//! and a typed slot ignores even that. Building the whole owned tree per read
//! (labels, descriptions, every variant) was most of a project load's shape
//! cost on the C6 (`lp2025/2026-10-16-0848-tree-store-adoption`, A8).

use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};

use crate::{
    DynamicSlotObject, SlotData, SlotDataMutAccess, SlotEnumEncodingView, SlotFactoryError,
    SlotMapKey, SlotMapKeyShape, SlotMutAccess, SlotMutationError, SlotShape, SlotShapeId,
    SlotShapeLookup, SlotShapeRegistry, SlotShapeView, StaticSlotShapeDescriptor,
    create_dynamic_slot_data, current_revision,
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
    let shape = shape_view_for_id(registry, shape_id)?;
    let mut object = registry
        .create_default(shape_id)
        .map_err(factory_error_to_syntax)?;
    apply_reader_to_view(object.data_mut(), shape, registry, value)?;
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
    let shape = shape_view_for_id(registry, shape_id)?;
    let mut slot = registry
        .create_default(shape_id)
        .map_err(factory_error_to_syntax)?;
    apply_object_reader_to_view(slot.data_mut(), shape, registry, object)?;
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
    // Creating dynamic data takes an owned shape; the read itself walks it
    // borrowed.
    let shape = shape_view_for_id(registry, shape_id)?.to_owned_shape();
    let data = create_dynamic_slot_data(registry, &shape).map_err(factory_error_to_syntax)?;
    let mut object = DynamicSlotObject::new(shape_id, data);
    apply_reader_to_view(
        object.data_mut(),
        SlotShapeView::Dynamic(&shape),
        registry,
        value,
    )?;
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
    apply_reader_to_view(data, SlotShapeView::Dynamic(shape), registry, value)
}

fn apply_reader_to_view<S>(
    data: SlotDataMutAccess<'_>,
    shape: SlotShapeView<'_>,
    registry: &SlotShapeRegistry,
    value: ValueReader<'_, '_, S>,
) -> Result<(), SyntaxError>
where
    S: SyntaxEventSource,
{
    match shape {
        SlotShapeView::Static(StaticSlotShapeDescriptor::Ref { id })
        | SlotShapeView::Dynamic(SlotShape::Ref { id }) => {
            let shape = registry
                .get_shape(*id)
                .ok_or_else(|| syntax_error(format!("missing referenced slot shape: {id}")))?;
            apply_reader_to_view(data, shape, registry, value)
        }
        SlotShapeView::Static(StaticSlotShapeDescriptor::Unit { .. })
        | SlotShapeView::Dynamic(SlotShape::Unit { .. }) => value.skip_value(),
        SlotShapeView::Static(StaticSlotShapeDescriptor::Value { shape }) => {
            read_value(data, &shape.ty.to_owned_type(), value)
        }
        SlotShapeView::Dynamic(SlotShape::Value { shape }) => read_value(data, &shape.ty, value),
        SlotShapeView::Static(StaticSlotShapeDescriptor::Record { .. })
        | SlotShapeView::Dynamic(SlotShape::Record { .. }) => {
            let object = value.object()?;
            read_record_object(data, shape, registry, object)
        }
        SlotShapeView::Static(StaticSlotShapeDescriptor::Map { key, .. })
        | SlotShapeView::Dynamic(SlotShape::Map { key, .. }) => {
            let item = shape.map_value().expect("matched a map shape");
            let object = value.object()?;
            read_map_object(data, *key, item, registry, object)
        }
        SlotShapeView::Static(StaticSlotShapeDescriptor::Enum { .. })
        | SlotShapeView::Dynamic(SlotShape::Enum { .. }) => read_enum(data, shape, registry, value),
        SlotShapeView::Static(StaticSlotShapeDescriptor::Option { .. })
        | SlotShapeView::Dynamic(SlotShape::Option { .. }) => {
            let some = shape.option_some().expect("matched an option shape");
            read_option(data, some, registry, value)
        }
        SlotShapeView::Static(StaticSlotShapeDescriptor::Custom { codec, .. })
        | SlotShapeView::Dynamic(SlotShape::Custom { codec, .. }) => {
            read_custom_slot(data, *codec, registry, value)
        }
    }
}

fn apply_object_reader_to_view<S>(
    data: SlotDataMutAccess<'_>,
    shape: SlotShapeView<'_>,
    registry: &SlotShapeRegistry,
    object: ObjectReader<'_, '_, S>,
) -> Result<(), SyntaxError>
where
    S: SyntaxEventSource,
{
    match shape {
        SlotShapeView::Static(StaticSlotShapeDescriptor::Ref { id })
        | SlotShapeView::Dynamic(SlotShape::Ref { id }) => {
            let shape = registry
                .get_shape(*id)
                .ok_or_else(|| syntax_error(format!("missing referenced slot shape: {id}")))?;
            apply_object_reader_to_view(data, shape, registry, object)
        }
        SlotShapeView::Static(StaticSlotShapeDescriptor::Unit { .. })
        | SlotShapeView::Dynamic(SlotShape::Unit { .. }) => {
            let SlotDataMutAccess::Unit(_) = data else {
                object.finish()?;
                return Err(syntax_error("shape expected a unit slot"));
            };
            object.finish()
        }
        SlotShapeView::Static(StaticSlotShapeDescriptor::Record { .. })
        | SlotShapeView::Dynamic(SlotShape::Record { .. }) => {
            read_record_object(data, shape, registry, object)
        }
        SlotShapeView::Static(StaticSlotShapeDescriptor::Map { key, .. })
        | SlotShapeView::Dynamic(SlotShape::Map { key, .. }) => {
            let item = shape.map_value().expect("matched a map shape");
            read_map_object(data, *key, item, registry, object)
        }
        SlotShapeView::Static(StaticSlotShapeDescriptor::Enum { .. })
        | SlotShapeView::Dynamic(SlotShape::Enum { .. }) => {
            let SlotDataMutAccess::Enum(en) = data else {
                object.finish()?;
                return Err(syntax_error("shape expected an enum slot"));
            };
            match shape.enum_encoding().expect("matched an enum shape") {
                SlotEnumEncodingView::Tagged(field) => {
                    read_tagged_enum_object(en, field, shape, registry, object)
                }
                SlotEnumEncodingView::External => {
                    read_external_enum_object(en, shape, registry, object)
                }
            }
        }
        SlotShapeView::Static(StaticSlotShapeDescriptor::Option { .. })
        | SlotShapeView::Dynamic(SlotShape::Option { .. }) => {
            let some = shape.option_some().expect("matched an option shape");
            let SlotDataMutAccess::Option(option) = data else {
                object.finish()?;
                return Err(syntax_error("shape expected an option slot"));
            };
            option
                .set_some_default(current_revision(), registry, &some.to_owned_shape())
                .map_err(mutation_error_to_syntax)?;
            let Some(data) = option.data_mut() else {
                return Err(syntax_error(
                    "option default creation did not create a value",
                ));
            };
            apply_object_reader_to_view(data, some, registry, object)
        }
        SlotShapeView::Static(StaticSlotShapeDescriptor::Value { .. })
        | SlotShapeView::Dynamic(SlotShape::Value { .. }) => {
            object.finish()?;
            Err(syntax_error("shape expected a scalar value, found object"))
        }
        SlotShapeView::Static(StaticSlotShapeDescriptor::Custom { .. })
        | SlotShapeView::Dynamic(SlotShape::Custom { .. }) => {
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

/// `shape` is a record shape (the callers matched it).
fn read_record_object<S>(
    data: SlotDataMutAccess<'_>,
    shape: SlotShapeView<'_>,
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

    while let Some(mut prop) = object.next_prop()? {
        let Some((index, field)) = shape.record_field_by_str(prop.name()) else {
            let expected = shape.record_field_names();
            return Err(prop.unknown_field(prop.name(), &expected));
        };
        // D2 (no authored Debug values): a Debug-role field is session-only
        // and must never adopt an authored def-file value as its base. Warn
        // and leave `prop` unconsumed — `PropReader::drop` skips its JSON
        // value — so the field keeps whatever `create_default`/factory value
        // it already has (alpha posture: warn, don't hard-reject).
        if field.role() == crate::SlotRole::Debug {
            log::warn!(
                "ignoring authored value for debug-role field `{}`: Debug slots are \
                 session-only and never adopt an authored value as their base",
                field.name_str()
            );
            continue;
        }
        let Some(field_data) = record.field_mut(index) else {
            return Err(syntax_error(format!(
                "record slot is missing field {:?}",
                field.name_str()
            )));
        };
        apply_reader_to_view(field_data, field.shape(), registry, prop.value())?;
    }

    Ok(())
}

fn read_map_object<S>(
    data: SlotDataMutAccess<'_>,
    key_shape: SlotMapKeyShape,
    item_shape: SlotShapeView<'_>,
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
    // Owned once per map, and only if it has an entry: a dynamic map builds
    // each entry's default from it.
    let mut owned_item: Option<SlotShape> = None;
    while let Some(mut prop) = object.next_prop()? {
        let key = parse_map_key(key_shape, prop.name())
            .map_err(|message| prop.unknown_field(prop.name(), &[message.as_str()]))?;
        let item = owned_item.get_or_insert_with(|| item_shape.to_owned_shape());
        map.insert_default(current_revision(), &key, registry, item)
            .map_err(mutation_error_to_syntax)?;
        let Some(item_data) = map.get_mut(&key) else {
            return Err(syntax_error(
                "map default insertion did not create an entry",
            ));
        };
        apply_reader_to_view(item_data, item_shape, registry, prop.value())?;
    }

    Ok(())
}

/// `shape` is an enum shape (the caller matched it).
fn read_enum<S>(
    data: SlotDataMutAccess<'_>,
    shape: SlotShapeView<'_>,
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
    let object = value.object()?;
    match shape.enum_encoding().expect("matched an enum shape") {
        SlotEnumEncodingView::Tagged(field) => {
            read_tagged_enum_object(en, field, shape, registry, object)
        }
        SlotEnumEncodingView::External => read_external_enum_object(en, shape, registry, object),
    }
}

fn read_tagged_enum_object<S>(
    en: &mut dyn crate::SlotEnumDefaultVariant,
    discriminator: &str,
    shape: SlotShapeView<'_>,
    registry: &SlotShapeRegistry,
    mut object: ObjectReader<'_, '_, S>,
) -> Result<(), SyntaxError>
where
    S: SyntaxEventSource,
{
    let expected = shape.enum_variant_names();
    let variant_name = object.expect_discriminator(discriminator, &expected)?;
    let variant = shape
        .enum_variant_by_str(&variant_name)
        .ok_or_else(|| syntax_error("validated enum variant was not found"))?;

    set_variant_default(en, &variant_name, variant, registry)?;
    let data = en.data_mut();
    read_enum_payload_object(data, variant.shape(), registry, object)
}

fn read_external_enum_object<S>(
    en: &mut dyn crate::SlotEnumDefaultVariant,
    shape: SlotShapeView<'_>,
    registry: &SlotShapeRegistry,
    mut object: ObjectReader<'_, '_, S>,
) -> Result<(), SyntaxError>
where
    S: SyntaxEventSource,
{
    let Some(mut prop) = object.next_prop()? else {
        return Err(syntax_error(format!(
            "external enum expected exactly one variant property. Expected one of: {}.",
            shape.enum_variant_names().join(", ")
        )));
    };
    let variant_name = prop.name().to_string();
    let Some(variant) = shape.enum_variant_by_str(&variant_name) else {
        let expected = shape.enum_variant_names();
        return Err(prop.unknown_field(&variant_name, &expected));
    };

    set_variant_default(en, &variant_name, variant, registry)?;
    let data = en.data_mut();
    apply_reader_to_view(data, variant.shape(), registry, prop.value())?;
    drop(prop);

    if let Some(prop) = object.next_prop()? {
        let name = prop.name().to_string();
        return Err(prop.unknown_field(&name, &[]));
    }
    Ok(())
}

/// Switch `en` to `variant`'s default. The mutation finds the variant by
/// name in the slice it is handed, so it is handed the one variant (owned),
/// never the whole enum.
fn set_variant_default(
    en: &mut dyn crate::SlotEnumDefaultVariant,
    variant_name: &str,
    variant: crate::SlotVariantShapeView<'_>,
    registry: &SlotShapeRegistry,
) -> Result<(), SyntaxError> {
    let owned = [variant.to_owned_variant()];
    en.set_variant_default_with_shape(current_revision(), variant_name, registry, &owned)
        .map_err(mutation_error_to_syntax)
}

fn read_enum_payload_object<S>(
    data: SlotDataMutAccess<'_>,
    shape: SlotShapeView<'_>,
    registry: &SlotShapeRegistry,
    object: ObjectReader<'_, '_, S>,
) -> Result<(), SyntaxError>
where
    S: SyntaxEventSource,
{
    match shape {
        SlotShapeView::Static(StaticSlotShapeDescriptor::Ref { id })
        | SlotShapeView::Dynamic(SlotShape::Ref { id }) => {
            let shape = registry
                .get_shape(*id)
                .ok_or_else(|| syntax_error(format!("missing referenced slot shape: {id}")))?;
            read_enum_payload_object(data, shape, registry, object)
        }
        SlotShapeView::Static(StaticSlotShapeDescriptor::Record { .. })
        | SlotShapeView::Dynamic(SlotShape::Record { .. }) => {
            read_record_object(data, shape, registry, object)
        }
        SlotShapeView::Static(StaticSlotShapeDescriptor::Unit { .. })
        | SlotShapeView::Dynamic(SlotShape::Unit { .. }) => object.finish(),
        _ => Err(syntax_error(
            "dynamic enum reader only supports record and unit variant payloads",
        )),
    }
}

fn read_option<S>(
    data: SlotDataMutAccess<'_>,
    some_shape: SlotShapeView<'_>,
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
        .set_some_default(current_revision(), registry, &some_shape.to_owned_shape())
        .map_err(mutation_error_to_syntax)?;
    let Some(data) = option.data_mut() else {
        return Err(syntax_error(
            "option default creation did not create a value",
        ));
    };
    apply_reader_to_view(data, some_shape, registry, value)
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

fn shape_view_for_id(
    registry: &SlotShapeRegistry,
    id: SlotShapeId,
) -> Result<SlotShapeView<'_>, SyntaxError> {
    registry
        .get_shape(id)
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

    /// D2 (no authored Debug values): a Debug-role field's authored value is
    /// ignored on parse — the field keeps its factory default rather than
    /// adopting the def-file content — and unrelated sibling fields still
    /// read normally.
    #[test]
    fn dynamic_slot_reader_ignores_authored_debug_role_fields() {
        let shape_id = crate::SlotShapeId::from_static_name("test.DynamicDebugIgnored");
        let mut registry = SlotShapeRegistry::default();
        registry
            .register_dynamic_shape(
                shape_id,
                shape::record(vec![
                    shape::field("pin", shape::value(LpType::U32)),
                    shape::field_with_role(
                        "rate",
                        shape::value(LpType::F32),
                        crate::SlotRole::Debug,
                    ),
                ]),
            )
            .unwrap();

        let object = read_json(&registry, shape_id, r#"{"pin":18,"rate":9.5}"#);

        let SlotDataAccess::Record(record) = object.data() else {
            panic!("expected record");
        };
        assert_eq!(
            record_value(record, 0),
            LpValue::U32(18),
            "sibling field reads normally"
        );
        assert_eq!(
            record_value(record, 1),
            LpValue::F32(0.0),
            "authored debug-role value must not become the base; factory default wins"
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
