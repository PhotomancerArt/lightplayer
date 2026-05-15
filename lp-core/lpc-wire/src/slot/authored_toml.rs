//! Slot-shape-driven authored TOML conversion.
//!
//! This module is intentionally generic over slot shape/data. It does not know
//! about concrete LightPlayer domain structs; persisted root metadata such as
//! `kind` remains a caller-owned loader concern.

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;

use lpc_model::{
    LpType, LpValue, Revision, SlotData, SlotDataAccess, SlotEnum, SlotMapDyn, SlotMapKey,
    SlotMapKeyShape, SlotOptionDyn, SlotRecord, SlotShape, SlotShapeRegistry, WithRevision,
    current_revision,
};
use toml::Value;

/// Error returned by slot authored TOML conversion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlotTomlError {
    path: String,
    message: String,
}

impl SlotTomlError {
    fn new(path: &Path, message: impl Into<String>) -> Self {
        Self {
            path: path.to_string(),
            message: message.into(),
        }
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for SlotTomlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.path.is_empty() {
            f.write_str(&self.message)
        } else {
            write!(f, "{}: {}", self.path, self.message)
        }
    }
}

impl core::error::Error for SlotTomlError {}

/// Decode authored TOML into owned slot data for `shape`.
pub fn decode_slot_data_toml(
    shape: &SlotShape,
    value: &Value,
    registry: &SlotShapeRegistry,
) -> Result<SlotData, SlotTomlError> {
    decode_shape(shape, value, registry, &mut Path::default(), &[])
}

/// Decode authored TOML while ignoring loader-owned fields on the current
/// record/table, such as top-level `kind`.
pub fn decode_slot_data_toml_with_ignored_fields(
    shape: &SlotShape,
    value: &Value,
    registry: &SlotShapeRegistry,
    ignored_fields: &[&str],
) -> Result<SlotData, SlotTomlError> {
    decode_shape(shape, value, registry, &mut Path::default(), ignored_fields)
}

/// Encode owned slot data into authored TOML.
pub fn encode_slot_data_toml(
    shape: &SlotShape,
    data: &SlotData,
    registry: &SlotShapeRegistry,
) -> Result<Value, SlotTomlError> {
    encode_shape(shape, data.access(), registry, &mut Path::default())
}

/// Encode borrowed slot data into authored TOML.
pub fn encode_slot_data_access_toml(
    shape: &SlotShape,
    data: SlotDataAccess<'_>,
    registry: &SlotShapeRegistry,
) -> Result<Value, SlotTomlError> {
    encode_shape(shape, data, registry, &mut Path::default())
}

fn decode_shape(
    shape: &SlotShape,
    value: &Value,
    registry: &SlotShapeRegistry,
    path: &mut Path,
    ignored_fields: &[&str],
) -> Result<SlotData, SlotTomlError> {
    match shape {
        SlotShape::Ref { id } => {
            let shape = registry
                .get(id)
                .ok_or_else(|| SlotTomlError::new(path, format!("missing slot shape {id}")))?;
            decode_shape(shape, value, registry, path, ignored_fields)
        }
        SlotShape::Unit { .. } => Ok(SlotData::Unit {
            revision: current_revision(),
        }),
        SlotShape::Value { shape } => Ok(SlotData::Value(WithRevision::new(
            current_revision(),
            decode_lp_value(&shape.ty, value, path)?,
        ))),
        SlotShape::Record { fields, .. } => {
            let table = value
                .as_table()
                .ok_or_else(|| SlotTomlError::new(path, "expected table"))?;
            let mut field_values = Vec::with_capacity(fields.len());
            for field in fields {
                let name = field.name.as_str();
                match table.get(name) {
                    Some(field_value) => {
                        path.push_field(name);
                        let decoded = decode_shape(&field.shape, field_value, registry, path, &[])?;
                        path.pop();
                        field_values.push(decoded);
                    }
                    None => field_values.push(missing_field_data(&field.shape, registry, path)?),
                }
            }
            reject_unknown_fields(table, fields, ignored_fields, path)?;
            Ok(SlotData::Record(SlotRecord::with_revision(
                Revision::default(),
                field_values,
            )))
        }
        SlotShape::Map {
            key,
            value: item_shape,
            ..
        } => {
            let table = value
                .as_table()
                .ok_or_else(|| SlotTomlError::new(path, "expected table"))?;
            let mut entries = BTreeMap::new();
            for (key_text, item_value) in table {
                let map_key = decode_map_key(*key, key_text, path)?;
                path.push_key(key_text);
                let decoded = decode_shape(item_shape, item_value, registry, path, &[])?;
                path.pop();
                entries.insert(map_key, decoded);
            }
            Ok(SlotData::Map(SlotMapDyn::new(entries)))
        }
        SlotShape::Enum { variants, .. } => {
            let table = value
                .as_table()
                .ok_or_else(|| SlotTomlError::new(path, "expected enum table"))?;
            let variant_name = table
                .get("kind")
                .and_then(Value::as_str)
                .ok_or_else(|| SlotTomlError::new(path, "expected enum discriminator `kind`"))?;
            let variant = variants
                .iter()
                .find(|variant| variant.name.as_str() == variant_name)
                .ok_or_else(|| {
                    SlotTomlError::new(path, format!("unknown enum variant {variant_name:?}"))
                })?;
            path.push_field(variant_name);
            let data = decode_shape(&variant.shape, value, registry, path, &["kind"])?;
            path.pop();
            Ok(SlotData::Enum(SlotEnum::with_version(
                current_revision(),
                variant.name.clone(),
                data,
            )))
        }
        SlotShape::Option { some, .. } => Ok(SlotData::Option(SlotOptionDyn::some(decode_shape(
            some,
            value,
            registry,
            path,
            ignored_fields,
        )?))),
    }
}

fn missing_field_data(
    shape: &SlotShape,
    registry: &SlotShapeRegistry,
    path: &Path,
) -> Result<SlotData, SlotTomlError> {
    match shape {
        SlotShape::Ref { id } => {
            let shape = registry
                .get(id)
                .ok_or_else(|| SlotTomlError::new(path, format!("missing slot shape {id}")))?;
            missing_field_data(shape, registry, path)
        }
        SlotShape::Unit { .. } => Ok(SlotData::Unit {
            revision: current_revision(),
        }),
        SlotShape::Map { .. } => Ok(SlotData::Map(SlotMapDyn::new(BTreeMap::new()))),
        SlotShape::Option { .. } => Ok(SlotData::Option(SlotOptionDyn::none())),
        _ => Err(SlotTomlError::new(path, "missing required field")),
    }
}

fn reject_unknown_fields(
    table: &toml::Table,
    fields: &[lpc_model::SlotFieldShape],
    ignored_fields: &[&str],
    path: &Path,
) -> Result<(), SlotTomlError> {
    for key in table.keys() {
        if ignored_fields.iter().any(|ignored| *ignored == key) {
            continue;
        }
        if !fields.iter().any(|field| field.name.as_str() == key) {
            return Err(SlotTomlError::new(
                path,
                format!("unknown authored field {key:?}"),
            ));
        }
    }
    Ok(())
}

fn encode_shape(
    shape: &SlotShape,
    data: SlotDataAccess<'_>,
    registry: &SlotShapeRegistry,
    path: &mut Path,
) -> Result<Value, SlotTomlError> {
    match (shape, data) {
        (SlotShape::Ref { id }, data) => {
            let shape = registry
                .get(id)
                .ok_or_else(|| SlotTomlError::new(path, format!("missing slot shape {id}")))?;
            encode_shape(shape, data, registry, path)
        }
        (SlotShape::Unit { .. }, SlotDataAccess::Unit(_)) => Ok(Value::Table(toml::Table::new())),
        (SlotShape::Value { shape }, SlotDataAccess::Value(value)) => {
            encode_lp_value(&shape.ty, &value.value(), path)
        }
        (SlotShape::Record { fields, .. }, SlotDataAccess::Record(record)) => {
            let mut table = toml::Table::new();
            for (index, field) in fields.iter().enumerate() {
                let Some(field_data) = record.field(index) else {
                    return Err(SlotTomlError::new(
                        path,
                        format!("missing record data for field {:?}", field.name.as_str()),
                    ));
                };
                if matches_none_option(field_data) {
                    continue;
                }
                path.push_field(field.name.as_str());
                let value = encode_shape(&field.shape, field_data, registry, path)?;
                path.pop();
                table.insert(field.name.as_str().to_string(), value);
            }
            Ok(Value::Table(table))
        }
        (SlotShape::Map { value, .. }, SlotDataAccess::Map(map)) => {
            let mut table = toml::Table::new();
            for key in map.keys() {
                let authored_key = encode_map_key(&key);
                let data = map
                    .get(&key)
                    .ok_or_else(|| SlotTomlError::new(path, "map key disappeared during encode"))?;
                path.push_key(&authored_key);
                let value = encode_shape(value, data, registry, path)?;
                path.pop();
                table.insert(authored_key, value);
            }
            Ok(Value::Table(table))
        }
        (SlotShape::Enum { variants, .. }, SlotDataAccess::Enum(en)) => {
            let variant = variants
                .iter()
                .find(|variant| variant.name.as_str() == en.variant())
                .ok_or_else(|| SlotTomlError::new(path, "enum variant missing from shape"))?;
            path.push_field(en.variant());
            let encoded = encode_shape(&variant.shape, en.data(), registry, path)?;
            path.pop();
            let mut table = match encoded {
                Value::Table(table) => table,
                _ => toml::Table::new(),
            };
            table.insert("kind".to_string(), Value::String(en.variant().to_string()));
            Ok(Value::Table(table))
        }
        (SlotShape::Option { some, .. }, SlotDataAccess::Option(option)) => match option.data() {
            Some(data) => encode_shape(some, data, registry, path),
            None => Ok(Value::Table(toml::Table::new())),
        },
        _ => Err(SlotTomlError::new(path, "slot shape/data mismatch")),
    }
}

fn matches_none_option(data: SlotDataAccess<'_>) -> bool {
    matches!(data, SlotDataAccess::Option(option) if option.data().is_none())
}

fn decode_lp_value(ty: &LpType, value: &Value, path: &Path) -> Result<LpValue, SlotTomlError> {
    match ty {
        LpType::String => value
            .as_str()
            .map(|value| LpValue::String(value.to_string()))
            .ok_or_else(|| SlotTomlError::new(path, "expected string")),
        LpType::I32 => integer(value, path)
            .and_then(|value| {
                i32::try_from(value).map_err(|_| SlotTomlError::new(path, "expected i32"))
            })
            .map(LpValue::I32),
        LpType::U32 => integer(value, path)
            .and_then(|value| {
                u32::try_from(value).map_err(|_| SlotTomlError::new(path, "expected u32"))
            })
            .map(LpValue::U32),
        LpType::F32 => float(value, path).map(LpValue::F32),
        LpType::Bool => value
            .as_bool()
            .map(LpValue::Bool)
            .ok_or_else(|| SlotTomlError::new(path, "expected bool")),
        LpType::Vec2 => {
            fixed_f32_array(value, 2, path).map(|values| LpValue::Vec2([values[0], values[1]]))
        }
        LpType::Vec3 => fixed_f32_array(value, 3, path)
            .map(|values| LpValue::Vec3([values[0], values[1], values[2]])),
        LpType::Mat2x2 => fixed_f32_matrix(value, path).map(LpValue::Mat2x2),
        LpType::Mat3x3 => fixed_f32_matrix(value, path).map(LpValue::Mat3x3),
        LpType::Mat4x4 => fixed_f32_matrix(value, path).map(LpValue::Mat4x4),
        LpType::Struct { name, fields } => {
            let table = value
                .as_table()
                .ok_or_else(|| SlotTomlError::new(path, "expected struct table"))?;
            let mut out = Vec::with_capacity(fields.len());
            for field in fields {
                let field_value = table.get(&field.name).ok_or_else(|| {
                    SlotTomlError::new(path, format!("missing struct field {:?}", field.name))
                })?;
                out.push((
                    field.name.clone(),
                    decode_lp_value(&field.ty, field_value, path)?,
                ));
            }
            Ok(LpValue::Struct {
                name: name.clone(),
                fields: out,
            })
        }
        LpType::List(item) => {
            let values = array(value, path)?
                .iter()
                .map(|value| decode_lp_value(item, value, path))
                .collect::<Result<_, _>>()?;
            Ok(LpValue::Array(values))
        }
        LpType::Array(item, len) => {
            let values = array(value, path)?;
            if values.len() != *len {
                return Err(SlotTomlError::new(
                    path,
                    format!("expected array length {len}"),
                ));
            }
            let values = values
                .iter()
                .map(|value| decode_lp_value(item, value, path))
                .collect::<Result<_, _>>()?;
            Ok(LpValue::Array(values))
        }
        other => Err(SlotTomlError::new(
            path,
            format!("unsupported TOML value type {other:?}"),
        )),
    }
}

fn encode_lp_value(ty: &LpType, value: &LpValue, path: &Path) -> Result<Value, SlotTomlError> {
    match (ty, value) {
        (LpType::String, LpValue::String(value)) => Ok(Value::String(value.clone())),
        (LpType::I32, LpValue::I32(value)) => Ok(Value::Integer(i64::from(*value))),
        (LpType::U32, LpValue::U32(value)) => Ok(Value::Integer(i64::from(*value))),
        (LpType::F32, LpValue::F32(value)) => Ok(Value::Float(f64::from(*value))),
        (LpType::Bool, LpValue::Bool(value)) => Ok(Value::Boolean(*value)),
        (LpType::Vec2, LpValue::Vec2(values)) => Ok(Value::Array(
            values
                .iter()
                .map(|value| Value::Float(f64::from(*value)))
                .collect(),
        )),
        (LpType::Vec3, LpValue::Vec3(values)) => Ok(Value::Array(
            values
                .iter()
                .map(|value| Value::Float(f64::from(*value)))
                .collect(),
        )),
        (LpType::Mat2x2, LpValue::Mat2x2(values)) => Ok(encode_f32_matrix(values)),
        (LpType::Mat3x3, LpValue::Mat3x3(values)) => Ok(encode_f32_matrix(values)),
        (LpType::Mat4x4, LpValue::Mat4x4(values)) => Ok(encode_f32_matrix(values)),
        (LpType::Struct { fields, .. }, LpValue::Struct { fields: values, .. }) => {
            let mut table = toml::Table::new();
            for field in fields {
                let Some((_, value)) = values.iter().find(|(name, _)| name == &field.name) else {
                    return Err(SlotTomlError::new(
                        path,
                        format!("missing value struct field {:?}", field.name),
                    ));
                };
                table.insert(field.name.clone(), encode_lp_value(&field.ty, value, path)?);
            }
            Ok(Value::Table(table))
        }
        (LpType::List(item), LpValue::Array(values))
        | (LpType::Array(item, _), LpValue::Array(values)) => Ok(Value::Array(
            values
                .iter()
                .map(|value| encode_lp_value(item, value, path))
                .collect::<Result<_, _>>()?,
        )),
        _ => Err(SlotTomlError::new(
            path,
            format!("value {value:?} does not match type {ty:?}"),
        )),
    }
}

fn decode_map_key(
    shape: SlotMapKeyShape,
    text: &str,
    path: &Path,
) -> Result<SlotMapKey, SlotTomlError> {
    match shape {
        SlotMapKeyShape::String => Ok(SlotMapKey::String(text.to_string())),
        SlotMapKeyShape::I32 => text
            .parse()
            .map(SlotMapKey::I32)
            .map_err(|_| SlotTomlError::new(path, format!("expected i32 map key, got {text:?}"))),
        SlotMapKeyShape::U32 => text
            .parse()
            .map(SlotMapKey::U32)
            .map_err(|_| SlotTomlError::new(path, format!("expected u32 map key, got {text:?}"))),
    }
}

fn encode_map_key(key: &SlotMapKey) -> String {
    match key {
        SlotMapKey::String(value) => value.clone(),
        SlotMapKey::I32(value) => value.to_string(),
        SlotMapKey::U32(value) => value.to_string(),
    }
}

fn integer(value: &Value, path: &Path) -> Result<i64, SlotTomlError> {
    value
        .as_integer()
        .ok_or_else(|| SlotTomlError::new(path, "expected integer"))
}

fn float(value: &Value, path: &Path) -> Result<f32, SlotTomlError> {
    match value {
        Value::Float(value) => Ok(*value as f32),
        Value::Integer(value) => Ok(*value as f32),
        _ => Err(SlotTomlError::new(path, "expected float")),
    }
}

fn array<'a>(value: &'a Value, path: &Path) -> Result<&'a [Value], SlotTomlError> {
    value
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| SlotTomlError::new(path, "expected array"))
}

fn fixed_f32_array(value: &Value, len: usize, path: &Path) -> Result<Vec<f32>, SlotTomlError> {
    let values = array(value, path)?;
    if values.len() != len {
        return Err(SlotTomlError::new(
            path,
            format!("expected array length {len}"),
        ));
    }
    values.iter().map(|value| float(value, path)).collect()
}

fn fixed_f32_matrix<const N: usize>(
    value: &Value,
    path: &Path,
) -> Result<[[f32; N]; N], SlotTomlError> {
    let rows = array(value, path)?;
    if rows.len() != N {
        return Err(SlotTomlError::new(
            path,
            format!("expected matrix row count {N}"),
        ));
    }

    let mut matrix = [[0.0; N]; N];
    for (row_index, row_value) in rows.iter().enumerate() {
        let row = fixed_f32_array(row_value, N, path)?;
        matrix[row_index].copy_from_slice(&row);
    }
    Ok(matrix)
}

fn encode_f32_matrix<const N: usize>(matrix: &[[f32; N]; N]) -> Value {
    Value::Array(
        matrix
            .iter()
            .map(|row| {
                Value::Array(
                    row.iter()
                        .map(|value| Value::Float(f64::from(*value)))
                        .collect(),
                )
            })
            .collect(),
    )
}

#[derive(Default)]
struct Path {
    segments: Vec<String>,
}

impl Path {
    fn push_field(&mut self, field: &str) {
        self.segments.push(field.to_string());
    }

    fn push_key(&mut self, key: &str) {
        self.segments.push(format!("[{key}]"));
    }

    fn pop(&mut self) {
        self.segments.pop();
    }
}

impl fmt::Display for Path {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, segment) in self.segments.iter().enumerate() {
            if segment.starts_with('[') {
                f.write_str(segment)?;
            } else {
                if index > 0 {
                    f.write_str(".")?;
                }
                f.write_str(segment)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn mat3x3_lp_value_round_trips_toml_arrays() {
        let value = LpValue::Mat3x3([[1.0, 0.25, 12.0], [-0.5, 2.0, -8.0], [0.0, 0.0, 1.0]]);

        let encoded =
            encode_lp_value(&LpType::Mat3x3, &value, &Path::default()).expect("encode matrix");
        assert_eq!(
            encoded,
            Value::Array(vec![
                Value::Array(vec![
                    Value::Float(1.0),
                    Value::Float(0.25),
                    Value::Float(12.0)
                ]),
                Value::Array(vec![
                    Value::Float(-0.5),
                    Value::Float(2.0),
                    Value::Float(-8.0)
                ]),
                Value::Array(vec![
                    Value::Float(0.0),
                    Value::Float(0.0),
                    Value::Float(1.0)
                ]),
            ])
        );
        assert_eq!(
            decode_lp_value(&LpType::Mat3x3, &encoded, &Path::default()).expect("decode matrix"),
            value
        );
    }
}
