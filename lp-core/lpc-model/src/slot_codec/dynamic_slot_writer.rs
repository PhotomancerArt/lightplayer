use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::{
    ControlProduct, LpType, LpValue, ModelEnumVariant, ModelStructMember, ProductKind, ProductRef,
    ResourceDomain, ResourceRef, SlotAccess, SlotDataAccess, SlotEnumEncoding, SlotFieldShape,
    SlotMapKey, SlotShape, SlotShapeId, SlotShapeRegistry, SlotVariantShape, VisualProduct,
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
        .get(&id)
        .ok_or_else(|| json_data_error(SlotDataWriteError::MissingShape(id)))?;
    write_shape_json(value, shape, data, registry)
}

pub fn write_dynamic_slot_toml(
    registry: &SlotShapeRegistry,
    root: &dyn SlotAccess,
) -> Result<toml::Value, SlotDataWriteError> {
    write_slot_data_toml_value(registry, root.shape_id(), root.data())
}

pub fn write_slot_data_toml_value(
    registry: &SlotShapeRegistry,
    id: SlotShapeId,
    data: SlotDataAccess<'_>,
) -> Result<toml::Value, SlotDataWriteError> {
    let shape = registry
        .get(&id)
        .ok_or(SlotDataWriteError::MissingShape(id))?;
    write_shape_toml(shape, data, registry)
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
                .get(id)
                .ok_or_else(|| json_data_error(SlotDataWriteError::MissingReferencedShape(*id)))?;
            write_shape_json(value, shape, data, registry)
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
                .get(id)
                .ok_or_else(|| json_data_error(SlotDataWriteError::MissingReferencedShape(*id)))?;
            write_enum_payload_json(object, shape, data, registry)
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
            .get(id)
            .is_some_and(|shape| should_omit_field(shape, data, registry)),
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

fn write_shape_toml(
    shape: &SlotShape,
    data: SlotDataAccess<'_>,
    registry: &SlotShapeRegistry,
) -> Result<toml::Value, SlotDataWriteError> {
    match shape {
        SlotShape::Ref { id } => {
            let shape = registry
                .get(id)
                .ok_or(SlotDataWriteError::MissingReferencedShape(*id))?;
            write_shape_toml(shape, data, registry)
        }
        SlotShape::Unit { .. } => match data {
            SlotDataAccess::Unit(_) => Ok(toml_table(toml::Table::new())),
            other => Err(SlotDataWriteError::mismatch(format!(
                "slot shape expected unit data, got {}",
                data_kind(other)
            ))),
        },
        SlotShape::Value { shape } => match data {
            SlotDataAccess::Value(slot) => write_lp_value_toml(&shape.ty, &slot.value()),
            other => Err(SlotDataWriteError::mismatch(format!(
                "slot shape expected value data, got {}",
                data_kind(other)
            ))),
        },
        SlotShape::Record { fields, .. } => match data {
            SlotDataAccess::Record(record) => {
                let mut table = toml::Table::new();
                write_record_fields_toml(&mut table, fields, record, registry)?;
                Ok(toml_table(table))
            }
            other => Err(SlotDataWriteError::mismatch(format!(
                "slot shape expected record data, got {}",
                data_kind(other)
            ))),
        },
        SlotShape::Map {
            value: item_shape, ..
        } => match data {
            SlotDataAccess::Map(map) => {
                let mut table = toml::Table::new();
                for key in map.keys() {
                    let key_text = map_key_text(&key);
                    let item = map.get(&key).ok_or_else(|| {
                        SlotDataWriteError::mismatch(format!(
                            "map key {key_text:?} disappeared during write"
                        ))
                    })?;
                    table.insert(key_text, write_shape_toml(item_shape, item, registry)?);
                }
                Ok(toml_table(table))
            }
            other => Err(SlotDataWriteError::mismatch(format!(
                "slot shape expected map data, got {}",
                data_kind(other)
            ))),
        },
        SlotShape::Enum {
            encoding, variants, ..
        } => match data {
            SlotDataAccess::Enum(en) => write_enum_toml(encoding, variants, en, registry),
            other => Err(SlotDataWriteError::mismatch(format!(
                "slot shape expected enum data, got {}",
                data_kind(other)
            ))),
        },
        SlotShape::Option { some, .. } => match data {
            SlotDataAccess::Option(option) => match option.data() {
                Some(data) => write_shape_toml(some, data, registry),
                None => Ok(toml_table(toml::Table::new())),
            },
            other => Err(SlotDataWriteError::mismatch(format!(
                "slot shape expected option data, got {}",
                data_kind(other)
            ))),
        },
        SlotShape::Custom { codec, .. } => match data {
            SlotDataAccess::Custom(custom) => write_custom_slot_toml(*codec, custom, registry),
            other => Err(SlotDataWriteError::mismatch(format!(
                "slot shape expected custom data, got {}",
                data_kind(other)
            ))),
        },
    }
}

fn write_record_fields_toml(
    table: &mut toml::Table,
    fields: &[SlotFieldShape],
    record: &dyn crate::SlotRecordAccess,
    registry: &SlotShapeRegistry,
) -> Result<(), SlotDataWriteError> {
    for (index, field) in fields.iter().enumerate() {
        let data = record.field(index).ok_or_else(|| {
            SlotDataWriteError::mismatch(format!(
                "record data is missing field {:?}",
                field.name.as_str()
            ))
        })?;
        if should_omit_field(&field.shape, data, registry) {
            continue;
        }
        table.insert(
            field.name.as_str().to_string(),
            write_shape_toml(&field.shape, data, registry)?,
        );
    }
    Ok(())
}

fn write_enum_payload_toml(
    table: &mut toml::Table,
    shape: &SlotShape,
    data: SlotDataAccess<'_>,
    registry: &SlotShapeRegistry,
) -> Result<(), SlotDataWriteError> {
    match shape {
        SlotShape::Ref { id } => {
            let shape = registry
                .get(id)
                .ok_or(SlotDataWriteError::MissingReferencedShape(*id))?;
            write_enum_payload_toml(table, shape, data, registry)
        }
        SlotShape::Record { fields, .. } => match data {
            SlotDataAccess::Record(record) => {
                write_record_fields_toml(table, fields, record, registry)
            }
            other => Err(SlotDataWriteError::mismatch(format!(
                "enum record payload expected record data, got {}",
                data_kind(other)
            ))),
        },
        SlotShape::Unit { .. } => match data {
            SlotDataAccess::Unit(_) => Ok(()),
            other => Err(SlotDataWriteError::mismatch(format!(
                "enum unit payload expected unit data, got {}",
                data_kind(other)
            ))),
        },
        _ => Err(SlotDataWriteError::unsupported_enum_payload(
            "dynamic enum writer only supports record and unit variant payloads",
        )),
    }
}

fn write_enum_toml(
    encoding: &SlotEnumEncoding,
    variants: &[SlotVariantShape],
    en: &dyn crate::SlotEnumAccess,
    registry: &SlotShapeRegistry,
) -> Result<toml::Value, SlotDataWriteError> {
    let variant_name = en.variant();
    let variant = find_variant(variants, variant_name)?;
    match encoding {
        SlotEnumEncoding::Tagged { field } => {
            let mut table = toml::Table::new();
            table.insert(
                field.as_str().to_string(),
                toml::Value::String(variant_name.to_string()),
            );
            write_enum_payload_toml(&mut table, &variant.shape, en.data(), registry)?;
            Ok(toml_table(table))
        }
        SlotEnumEncoding::External => {
            let mut table = toml::Table::new();
            table.insert(
                variant_name.to_string(),
                write_shape_toml(&variant.shape, en.data(), registry)?,
            );
            Ok(toml_table(table))
        }
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

fn write_lp_value_toml(ty: &LpType, value: &LpValue) -> Result<toml::Value, SlotDataWriteError> {
    match (ty, value) {
        (LpType::Any, value) => write_untyped_lp_value_toml(value),
        (LpType::String, LpValue::String(value)) => Ok(toml::Value::String(value.clone())),
        (LpType::I32, LpValue::I32(value)) => Ok(toml::Value::Integer(i64::from(*value))),
        (LpType::U32, LpValue::U32(value)) => Ok(toml::Value::Integer(i64::from(*value))),
        (LpType::F32, LpValue::F32(value)) if value.is_finite() => {
            Ok(toml::Value::Float(f64::from(*value)))
        }
        (LpType::Bool, LpValue::Bool(value)) => Ok(toml::Value::Boolean(*value)),
        (LpType::Vec2, LpValue::Vec2(values)) => write_f32_array_toml(values),
        (LpType::Vec3, LpValue::Vec3(values)) => write_f32_array_toml(values),
        (LpType::Vec4, LpValue::Vec4(values)) => write_f32_array_toml(values),
        (LpType::IVec2, LpValue::IVec2(values)) => write_i32_array_toml(values),
        (LpType::IVec3, LpValue::IVec3(values)) => write_i32_array_toml(values),
        (LpType::IVec4, LpValue::IVec4(values)) => write_i32_array_toml(values),
        (LpType::UVec2, LpValue::UVec2(values)) => write_u32_array_toml(values),
        (LpType::UVec3, LpValue::UVec3(values)) => write_u32_array_toml(values),
        (LpType::UVec4, LpValue::UVec4(values)) => write_u32_array_toml(values),
        (LpType::BVec2, LpValue::BVec2(values)) => write_bool_array_toml(values),
        (LpType::BVec3, LpValue::BVec3(values)) => write_bool_array_toml(values),
        (LpType::BVec4, LpValue::BVec4(values)) => write_bool_array_toml(values),
        (LpType::Mat2x2, LpValue::Mat2x2(values)) => write_f32_matrix_toml(values),
        (LpType::Mat3x3, LpValue::Mat3x3(values)) => write_f32_matrix_toml(values),
        (LpType::Mat4x4, LpValue::Mat4x4(values)) => write_f32_matrix_toml(values),
        (LpType::Array(item, len), LpValue::Array(values)) if values.len() == *len => {
            write_lp_array_toml(item, values)
        }
        (LpType::List(item), LpValue::Array(values)) => write_lp_array_toml(item, values),
        (LpType::Struct { fields, .. }, LpValue::Struct { fields: values, .. }) => {
            write_lp_struct_toml(fields, values)
        }
        (LpType::Enum { variants, .. }, LpValue::Enum { variant, payload }) => {
            write_lp_enum_toml(variants, *variant, payload.as_deref())
        }
        (LpType::Resource, LpValue::Resource(resource)) => write_resource_ref_toml(resource),
        (LpType::Product(ProductKind::Visual), LpValue::Product(ProductRef::Visual(product))) => {
            write_visual_product_toml(product)
        }
        (LpType::Product(ProductKind::Control), LpValue::Product(ProductRef::Control(product))) => {
            write_control_product_toml(product)
        }
        _ => Err(SlotDataWriteError::mismatch(format!(
            "value {value:?} does not match type {ty:?}"
        ))),
    }
}

fn write_untyped_lp_value_toml(value: &LpValue) -> Result<toml::Value, SlotDataWriteError> {
    match value {
        LpValue::Unset => {
            let mut table = toml::Table::new();
            table.insert("kind".to_string(), toml::Value::String("unset".to_string()));
            Ok(toml_table(table))
        }
        LpValue::String(value) => Ok(toml::Value::String(value.clone())),
        LpValue::I32(value) => Ok(toml::Value::Integer(i64::from(*value))),
        LpValue::U32(value) => Ok(toml::Value::Integer(i64::from(*value))),
        LpValue::F32(value) if value.is_finite() => Ok(toml::Value::Float(f64::from(*value))),
        LpValue::Bool(value) => Ok(toml::Value::Boolean(*value)),
        LpValue::Vec2(values) => write_f32_array_toml(values),
        LpValue::Vec3(values) => write_f32_array_toml(values),
        LpValue::Vec4(values) => write_f32_array_toml(values),
        LpValue::IVec2(values) => write_i32_array_toml(values),
        LpValue::IVec3(values) => write_i32_array_toml(values),
        LpValue::IVec4(values) => write_i32_array_toml(values),
        LpValue::UVec2(values) => write_u32_array_toml(values),
        LpValue::UVec3(values) => write_u32_array_toml(values),
        LpValue::UVec4(values) => write_u32_array_toml(values),
        LpValue::BVec2(values) => write_bool_array_toml(values),
        LpValue::BVec3(values) => write_bool_array_toml(values),
        LpValue::BVec4(values) => write_bool_array_toml(values),
        LpValue::Mat2x2(values) => write_f32_matrix_toml(values),
        LpValue::Mat3x3(values) => write_f32_matrix_toml(values),
        LpValue::Mat4x4(values) => write_f32_matrix_toml(values),
        LpValue::Array(values) => values
            .iter()
            .map(write_untyped_lp_value_toml)
            .collect::<Result<Vec<_>, _>>()
            .map(toml::Value::Array),
        LpValue::Struct { fields, .. } => {
            let mut table = toml::Table::new();
            for (name, value) in fields {
                table.insert(name.clone(), write_untyped_lp_value_toml(value)?);
            }
            Ok(toml_table(table))
        }
        LpValue::Enum { variant, payload } => {
            let mut table = toml::Table::new();
            table.insert(
                "variant".to_string(),
                toml::Value::Integer(i64::from(*variant)),
            );
            if let Some(payload) = payload {
                table.insert("payload".to_string(), write_untyped_lp_value_toml(payload)?);
            }
            Ok(toml_table(table))
        }
        LpValue::Resource(resource) => write_resource_ref_toml(resource),
        LpValue::Product(ProductRef::Visual(product)) => write_visual_product_toml(product),
        LpValue::Product(ProductRef::Control(product)) => write_control_product_toml(product),
        LpValue::F32(_) => Err(SlotDataWriteError::mismatch("non-finite f32 value")),
    }
}

fn write_lp_array_toml(
    item_ty: &LpType,
    values: &[LpValue],
) -> Result<toml::Value, SlotDataWriteError> {
    values
        .iter()
        .map(|value| write_lp_value_toml(item_ty, value))
        .collect::<Result<Vec<_>, _>>()
        .map(toml::Value::Array)
}

fn write_lp_struct_toml(
    fields: &[ModelStructMember],
    values: &[(String, LpValue)],
) -> Result<toml::Value, SlotDataWriteError> {
    let mut table = toml::Table::new();
    for field in fields {
        let Some((_, value)) = values.iter().find(|(name, _)| name == &field.name) else {
            return Err(SlotDataWriteError::mismatch(format!(
                "value struct is missing field {:?}",
                field.name
            )));
        };
        table.insert(field.name.clone(), write_lp_value_toml(&field.ty, value)?);
    }
    Ok(toml_table(table))
}

fn write_lp_enum_toml(
    variants: &[ModelEnumVariant],
    variant_index: u32,
    payload: Option<&LpValue>,
) -> Result<toml::Value, SlotDataWriteError> {
    let variant = variants.get(variant_index as usize).ok_or_else(|| {
        SlotDataWriteError::mismatch(format!(
            "enum variant index {variant_index} is out of range"
        ))
    })?;
    let mut table = toml::Table::new();
    table.insert(
        "kind".to_string(),
        toml::Value::String(variant.name.clone()),
    );
    match (&variant.payload, payload) {
        (Some(payload_ty), Some(payload)) => {
            table.insert(
                "payload".to_string(),
                write_lp_value_toml(payload_ty, payload)?,
            );
        }
        (Some(_), None) => {
            return Err(SlotDataWriteError::mismatch(format!(
                "enum variant {:?} requires a payload",
                variant.name
            )));
        }
        (None, Some(_)) => {
            return Err(SlotDataWriteError::mismatch(format!(
                "enum variant {:?} does not accept a payload",
                variant.name
            )));
        }
        (None, None) => {}
    }
    Ok(toml_table(table))
}

fn write_resource_ref_toml(resource: &ResourceRef) -> Result<toml::Value, SlotDataWriteError> {
    let mut table = toml::Table::new();
    table.insert(
        "domain".to_string(),
        toml::Value::String(resource_domain_name(resource.domain).to_string()),
    );
    table.insert(
        "id".to_string(),
        toml::Value::Integer(i64::from(resource.id)),
    );
    Ok(toml_table(table))
}

fn write_visual_product_toml(product: &VisualProduct) -> Result<toml::Value, SlotDataWriteError> {
    let mut table = toml::Table::new();
    table.insert(
        "kind".to_string(),
        toml::Value::String("visual".to_string()),
    );
    table.insert(
        "node".to_string(),
        toml::Value::Integer(i64::from(product.node().as_u32())),
    );
    table.insert(
        "output".to_string(),
        toml::Value::Integer(i64::from(product.output())),
    );
    Ok(toml_table(table))
}

fn write_control_product_toml(product: &ControlProduct) -> Result<toml::Value, SlotDataWriteError> {
    let mut table = toml::Table::new();
    let extent = product.preferred_extent();
    let mut extent_table = toml::Table::new();
    extent_table.insert(
        "rows".to_string(),
        toml::Value::Integer(i64::from(extent.rows)),
    );
    extent_table.insert(
        "samples_per_row".to_string(),
        toml::Value::Integer(i64::from(extent.samples_per_row)),
    );
    table.insert(
        "kind".to_string(),
        toml::Value::String("control".to_string()),
    );
    table.insert(
        "node".to_string(),
        toml::Value::Integer(i64::from(product.node().as_u32())),
    );
    table.insert(
        "output".to_string(),
        toml::Value::Integer(i64::from(product.output())),
    );
    table.insert("preferred_extent".to_string(), toml_table(extent_table));
    Ok(toml_table(table))
}

fn write_f32_array_toml<const N: usize>(
    values: &[f32; N],
) -> Result<toml::Value, SlotDataWriteError> {
    values
        .iter()
        .map(|value| {
            if value.is_finite() {
                Ok(toml::Value::Float(f64::from(*value)))
            } else {
                Err(SlotDataWriteError::mismatch("non-finite f32 value"))
            }
        })
        .collect::<Result<Vec<_>, _>>()
        .map(toml::Value::Array)
}

fn write_i32_array_toml<const N: usize>(
    values: &[i32; N],
) -> Result<toml::Value, SlotDataWriteError> {
    Ok(toml::Value::Array(
        values
            .iter()
            .map(|value| toml::Value::Integer(i64::from(*value)))
            .collect(),
    ))
}

fn write_u32_array_toml<const N: usize>(
    values: &[u32; N],
) -> Result<toml::Value, SlotDataWriteError> {
    Ok(toml::Value::Array(
        values
            .iter()
            .map(|value| toml::Value::Integer(i64::from(*value)))
            .collect(),
    ))
}

fn write_bool_array_toml<const N: usize>(
    values: &[bool; N],
) -> Result<toml::Value, SlotDataWriteError> {
    Ok(toml::Value::Array(
        values
            .iter()
            .map(|value| toml::Value::Boolean(*value))
            .collect(),
    ))
}

fn write_f32_matrix_toml<const N: usize>(
    values: &[[f32; N]; N],
) -> Result<toml::Value, SlotDataWriteError> {
    values
        .iter()
        .map(write_f32_array_toml)
        .collect::<Result<Vec<_>, _>>()
        .map(toml::Value::Array)
}

fn map_key_text(key: &SlotMapKey) -> String {
    match key {
        SlotMapKey::String(value) => value.clone(),
        SlotMapKey::I32(value) => value.to_string(),
        SlotMapKey::U32(value) => value.to_string(),
    }
}

fn resource_domain_name(domain: ResourceDomain) -> &'static str {
    match domain {
        ResourceDomain::Unset => "unset",
        ResourceDomain::RuntimeBuffer => "runtime_buffer",
    }
}

fn toml_table(table: toml::Table) -> toml::Value {
    toml::Value::Table(table)
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

fn write_custom_slot_toml(
    codec: SlotShapeId,
    data: &dyn crate::SlotCustomAccess,
    registry: &SlotShapeRegistry,
) -> Result<toml::Value, SlotDataWriteError> {
    if data.custom_codec_id() != codec {
        return Err(SlotDataWriteError::mismatch(format!(
            "slot data custom codec {} does not match shape codec {codec}",
            data.custom_codec_id()
        )));
    }
    crate::slot_codec::custom_slot_codec::write_custom_slot_toml(codec, data, registry)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        LpType, LpValue, ModelEnumVariant, ProductKind, ProductRef, Revision, SlotData, SlotMapDyn,
        SlotName, SlotOptionDyn, SlotRecord, SlotVariantShape, WithRevision,
        slot::shape::{enum_external, field, map, option, record, unit, value},
    };
    use alloc::boxed::Box;
    use alloc::collections::BTreeMap;
    use alloc::vec;

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
            BTreeMap::from([
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

    #[test]
    fn dynamic_slot_writer_writes_records_to_toml() {
        let (registry, shape_id, data) = record_fixture();
        let toml = write_slot_data_toml_value(&registry, shape_id, data.access()).unwrap();

        assert_eq!(toml["pin"].as_integer(), Some(18));
        assert_eq!(toml["name"].as_str(), Some("main"));
    }

    #[test]
    fn dynamic_slot_writer_writes_enums_to_toml() {
        let (registry, shape_id, data) = enum_fixture();
        let toml = write_slot_data_toml_value(&registry, shape_id, data.access()).unwrap();

        assert_eq!(toml["kind"].as_str(), Some("square"));
        assert_eq!(toml["size"].as_float(), Some(0.5));
    }

    #[test]
    fn dynamic_slot_writer_writes_external_value_enums_to_toml() {
        let (registry, shape_id, data) = external_value_enum_fixture();
        let toml = write_slot_data_toml_value(&registry, shape_id, data.access()).unwrap();

        assert_eq!(toml["file"].as_str(), Some("compute.glsl"));
    }

    #[test]
    fn dynamic_slot_writer_writes_external_record_enums_to_toml() {
        let (registry, shape_id, data) = external_record_enum_fixture();
        let toml = write_slot_data_toml_value(&registry, shape_id, data.access()).unwrap();

        assert_eq!(toml["point"]["x"].as_integer(), Some(10));
        assert_eq!(toml["point"]["y"].as_integer(), Some(11));
    }

    #[test]
    fn dynamic_slot_writer_omits_none_toml_fields() {
        let shape_id = SlotShapeId::from_static_name("test.WriterTomlNone");
        let mut registry = SlotShapeRegistry::default();
        registry
            .register_dynamic_shape(
                shape_id,
                record(vec![field("name", option(value(LpType::String)))]),
            )
            .unwrap();
        let data = SlotData::Record(SlotRecord::new(vec![SlotData::Option(
            SlotOptionDyn::none_with_version(Revision::default()),
        )]));

        let toml = write_slot_data_toml_value(&registry, shape_id, data.access()).unwrap();

        assert!(toml.as_table().unwrap().is_empty());
    }

    #[test]
    fn dynamic_slot_writer_writes_root_none_toml() {
        let shape_id = SlotShapeId::from_static_name("test.WriterTomlRootNone");
        let mut registry = SlotShapeRegistry::default();
        registry
            .register_dynamic_shape(shape_id, option(value(LpType::String)))
            .unwrap();
        let data = SlotData::Option(SlotOptionDyn::none_with_version(Revision::default()));

        let toml = write_slot_data_toml_value(&registry, shape_id, data.access()).unwrap();

        assert!(toml.as_table().unwrap().is_empty());
    }

    #[test]
    fn dynamic_slot_writer_writes_product_toml_leaves() {
        let value = LpValue::Product(ProductRef::visual(VisualProduct::new(
            crate::NodeId::new(3),
            2,
        )));
        let toml = write_lp_value_toml(&LpType::Product(ProductKind::Visual), &value).unwrap();

        assert_eq!(toml["kind"].as_str(), Some("visual"));
        assert_eq!(toml["node"].as_integer(), Some(3));
        assert_eq!(toml["output"].as_integer(), Some(2));
    }

    #[test]
    fn dynamic_slot_writer_writes_enum_toml_leaves() {
        let ty = LpType::Enum {
            name: Some("Endpoint".to_string()),
            variants: vec![
                ModelEnumVariant {
                    name: "Unset".to_string(),
                    payload: None,
                },
                ModelEnumVariant {
                    name: "Value".to_string(),
                    payload: Some(LpType::F32),
                },
            ],
        };
        let value = LpValue::Enum {
            variant: 1,
            payload: Some(Box::new(LpValue::F32(0.75))),
        };

        let toml = write_lp_value_toml(&ty, &value).unwrap();

        assert_eq!(toml["kind"].as_str(), Some("Value"));
        assert_eq!(toml["payload"].as_float(), Some(0.75));
    }

    #[test]
    fn dynamic_slot_writer_reports_shape_data_mismatch_toml() {
        let shape_id = SlotShapeId::from_static_name("test.WriterTomlMismatch");
        let mut registry = SlotShapeRegistry::default();
        registry
            .register_dynamic_shape(shape_id, value(LpType::Bool))
            .unwrap();
        let data = SlotData::Unit {
            revision: Revision::default(),
        };

        let error = write_slot_data_toml_value(&registry, shape_id, data.access()).unwrap_err();

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
