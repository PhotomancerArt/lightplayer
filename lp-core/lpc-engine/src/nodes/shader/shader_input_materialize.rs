//! Materialize resolved model slot data into shader ABI input values.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use lp_collection::VecMap;

use lpc_model::{
    LpType, LpValue, ShaderMapKeyDef, ShaderSlotDef, ShaderSlotKind, ShaderSlotMappingKind,
    ShaderValueShapeRef, SlotData, SlotMapKey, SlotShapeId, SlotShapeLookup, SlotShapeRegistry,
};
use lps_shared::LpsValueF32;

use crate::dataflow::resolver::resolver::model_value_to_lps_value_f32;

/// Convert one resolved/default shader input into the runtime shader ABI shape.
pub fn materialize_shader_input(
    slot_name: &str,
    slot: &ShaderSlotDef,
    data: Option<&SlotData>,
    registry: &SlotShapeRegistry,
) -> Result<LpsValueF32, ShaderInputMaterializeError> {
    match slot.kind.value() {
        ShaderSlotKind::Value => materialize_value_input(slot_name, slot, data),
        ShaderSlotKind::Map => materialize_map_input(slot_name, slot, data, registry),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShaderInputMaterializeError {
    ExpectedValue(String),
    ExpectedMap(String),
    MissingMapping(String),
    MissingKey(String),
    UnknownNativeShape(String),
    NativeShapeIsNotValue(String),
    UnsupportedType(String),
    UnsupportedKey(String),
    MissingKeyField {
        slot: String,
        key: String,
    },
    MismatchedKey {
        slot: String,
        key: String,
    },
    TooManyEntries {
        slot: String,
        len: u32,
        count: usize,
    },
    Value(String),
}

impl core::fmt::Display for ShaderInputMaterializeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::ExpectedValue(slot) => write!(f, "shader input {slot:?} expected value data"),
            Self::ExpectedMap(slot) => write!(f, "shader input {slot:?} expected map data"),
            Self::MissingMapping(slot) => write!(f, "shader map input {slot:?} missing mapping"),
            Self::MissingKey(slot) => write!(f, "shader map input {slot:?} missing key"),
            Self::UnknownNativeShape(name) => write!(f, "unknown native shader shape {name:?}"),
            Self::NativeShapeIsNotValue(name) => {
                write!(f, "native shader shape {name:?} is not a value shape")
            }
            Self::UnsupportedType(ty) => write!(f, "unsupported shader input type {ty}"),
            Self::UnsupportedKey(slot) => {
                write!(f, "shader map input {slot:?} has unsupported key")
            }
            Self::MissingKeyField { slot, key } => {
                write!(
                    f,
                    "shader map input {slot:?} item missing key field {key:?}"
                )
            }
            Self::MismatchedKey { slot, key } => {
                write!(
                    f,
                    "shader map input {slot:?} item key field {key:?} does not match map key"
                )
            }
            Self::TooManyEntries { slot, len, count } => write!(
                f,
                "shader map input {slot:?} has {count} entries but mapping length is {len}"
            ),
            Self::Value(message) => f.write_str(message),
        }
    }
}

impl core::error::Error for ShaderInputMaterializeError {}

fn materialize_value_input(
    slot_name: &str,
    slot: &ShaderSlotDef,
    data: Option<&SlotData>,
) -> Result<LpsValueF32, ShaderInputMaterializeError> {
    let value = match data {
        Some(SlotData::Value(value)) => value.value().clone(),
        Some(_) => {
            return Err(ShaderInputMaterializeError::ExpectedValue(String::from(
                slot_name,
            )));
        }
        None => slot.default_value(),
    };
    model_value_to_lps_value_f32(&value)
        .map_err(|e| ShaderInputMaterializeError::Value(format!("shader input {slot_name:?}: {e}")))
}

fn materialize_map_input(
    slot_name: &str,
    slot: &ShaderSlotDef,
    data: Option<&SlotData>,
    registry: &SlotShapeRegistry,
) -> Result<LpsValueF32, ShaderInputMaterializeError> {
    let mapping = slot
        .mapping
        .data
        .as_ref()
        .ok_or_else(|| ShaderInputMaterializeError::MissingMapping(String::from(slot_name)))?;
    match mapping.kind.value() {
        ShaderSlotMappingKind::Sentinel => {}
    }
    let key_def = slot
        .key
        .data
        .as_ref()
        .ok_or_else(|| ShaderInputMaterializeError::MissingKey(String::from(slot_name)))?;
    let ShaderMapKeyDef::U32 = key_def.value();

    let len = *mapping.len.value();
    let key_field = mapping.key.value().as_str();
    let empty_key = *mapping.empty_key.value();
    let element_ty = lp_type_for_ref(slot.value.value(), registry)?;
    let default_element = default_value_for_type(&element_ty)?;
    let mut items = Vec::new();
    let len_usize = usize::try_from(len).map_err(|_| {
        ShaderInputMaterializeError::Value(format!("shader map input {slot_name:?}: len overflow"))
    })?;
    items.resize(len_usize, default_element.clone());
    for item in &mut items {
        set_u32_field(slot_name, item, key_field, empty_key)?;
    }

    let entries: VecMap<SlotMapKey, SlotData> = match data {
        Some(SlotData::Map(map)) => map.entries.clone(),
        Some(_) => {
            return Err(ShaderInputMaterializeError::ExpectedMap(String::from(
                slot_name,
            )));
        }
        None => VecMap::new(),
    };
    if entries.len() > len_usize {
        return Err(ShaderInputMaterializeError::TooManyEntries {
            slot: String::from(slot_name),
            len,
            count: entries.len(),
        });
    }

    for (index, (key, data)) in entries.into_iter().enumerate() {
        let SlotMapKey::U32(key) = key else {
            return Err(ShaderInputMaterializeError::UnsupportedKey(String::from(
                slot_name,
            )));
        };
        let SlotData::Value(value) = data else {
            return Err(ShaderInputMaterializeError::ExpectedValue(String::from(
                slot_name,
            )));
        };
        validate_u32_field(slot_name, value.value(), key_field, key)?;
        items[index] = value.value().clone();
    }

    let mut out = Vec::with_capacity(items.len());
    for value in &items {
        out.push(model_value_to_lps_value_f32(value).map_err(|e| {
            ShaderInputMaterializeError::Value(format!("shader input {slot_name:?}: {e}"))
        })?);
    }
    Ok(LpsValueF32::Array(out.into_boxed_slice()))
}

fn lp_type_for_ref(
    value_ref: &ShaderValueShapeRef,
    registry: &SlotShapeRegistry,
) -> Result<LpType, ShaderInputMaterializeError> {
    if let Some(ty) = value_ref.as_lp_type() {
        return Ok(ty);
    }

    let id = SlotShapeId::from_static_name(value_ref.as_str());
    let shape = registry.get_shape(id).ok_or_else(|| {
        ShaderInputMaterializeError::UnknownNativeShape(value_ref.as_str().into())
    })?;
    shape.value_shape().map(|shape| shape.ty_owned()).ok_or(
        ShaderInputMaterializeError::NativeShapeIsNotValue(value_ref.as_str().into()),
    )
}

fn default_value_for_type(ty: &LpType) -> Result<LpValue, ShaderInputMaterializeError> {
    Ok(match ty {
        LpType::I32 => LpValue::I32(0),
        LpType::U32 => LpValue::U32(0),
        LpType::F32 => LpValue::F32(0.0),
        LpType::Bool => LpValue::Bool(false),
        LpType::Vec2 => LpValue::Vec2([0.0, 0.0]),
        LpType::Vec3 => LpValue::Vec3([0.0, 0.0, 0.0]),
        LpType::Vec4 => LpValue::Vec4([0.0, 0.0, 0.0, 0.0]),
        LpType::IVec2 => LpValue::IVec2([0, 0]),
        LpType::IVec3 => LpValue::IVec3([0, 0, 0]),
        LpType::IVec4 => LpValue::IVec4([0, 0, 0, 0]),
        LpType::UVec2 => LpValue::UVec2([0, 0]),
        LpType::UVec3 => LpValue::UVec3([0, 0, 0]),
        LpType::UVec4 => LpValue::UVec4([0, 0, 0, 0]),
        LpType::BVec2 => LpValue::BVec2([false, false]),
        LpType::BVec3 => LpValue::BVec3([false, false, false]),
        LpType::BVec4 => LpValue::BVec4([false, false, false, false]),
        LpType::Mat2x2 => LpValue::Mat2x2([[0.0, 0.0], [0.0, 0.0]]),
        LpType::Mat3x3 => LpValue::Mat3x3([[0.0, 0.0, 0.0]; 3]),
        LpType::Mat4x4 => LpValue::Mat4x4([[0.0, 0.0, 0.0, 0.0]; 4]),
        LpType::Array(element, len) => {
            let item = default_value_for_type(element)?;
            LpValue::Array(alloc::vec![item; *len])
        }
        LpType::Struct { name, fields } => {
            let mut out = Vec::with_capacity(fields.len());
            for field in fields {
                out.push((field.name.clone(), default_value_for_type(&field.ty)?));
            }
            LpValue::Struct {
                name: name.clone(),
                fields: out,
            }
        }
        other => {
            return Err(ShaderInputMaterializeError::UnsupportedType(format!(
                "{other:?}"
            )));
        }
    })
}

fn set_u32_field(
    slot_name: &str,
    value: &mut LpValue,
    field: &str,
    key: u32,
) -> Result<(), ShaderInputMaterializeError> {
    let LpValue::Struct { fields, .. } = value else {
        return Err(ShaderInputMaterializeError::UnsupportedType(format!(
            "{value:?}"
        )));
    };
    let Some((_, value)) = fields.iter_mut().find(|(name, _)| name == field) else {
        return Err(ShaderInputMaterializeError::MissingKeyField {
            slot: String::from(slot_name),
            key: String::from(field),
        });
    };
    *value = LpValue::U32(key);
    Ok(())
}

fn validate_u32_field(
    slot_name: &str,
    value: &LpValue,
    field: &str,
    expected: u32,
) -> Result<(), ShaderInputMaterializeError> {
    let LpValue::Struct { fields, .. } = value else {
        return Err(ShaderInputMaterializeError::UnsupportedType(format!(
            "{value:?}"
        )));
    };
    let Some((_, value)) = fields.iter().find(|(name, _)| name == field) else {
        return Err(ShaderInputMaterializeError::MissingKeyField {
            slot: String::from(slot_name),
            key: String::from(field),
        });
    };
    if value == &LpValue::U32(expected) {
        Ok(())
    } else {
        Err(ShaderInputMaterializeError::MismatchedKey {
            slot: String::from(slot_name),
            key: String::from(field),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_collection::VecMap;
    use lpc_model::{
        CONTROL_MESSAGE_SHAPE_NAME, ControlMessage, Revision, ShaderSlotMappingDef, SlotMapDyn,
        ToLpValue, WithRevision,
    };

    #[test]
    fn materializes_sentinel_map_to_shader_array() {
        let registry = SlotShapeRegistry::default();
        let slot = ShaderSlotDef::map_u32_native(
            CONTROL_MESSAGE_SHAPE_NAME,
            ShaderSlotMappingDef::sentinel(3, "id", 0),
        );
        let data = SlotData::Map(SlotMapDyn::with_revision(
            Revision::new(4),
            VecMap::from([(
                SlotMapKey::U32(7),
                SlotData::Value(WithRevision::new(
                    Revision::new(4),
                    ControlMessage::new(7, 42).to_lp_value(),
                )),
            )]),
        ));

        let value =
            materialize_shader_input("events", &slot, Some(&data), &registry).expect("input");

        let LpsValueF32::Array(items) = value else {
            panic!("expected array");
        };
        assert_eq!(items.len(), 3);
        assert_message(&items[0], 7, 42);
        assert_message(&items[1], 0, 0);
        assert_message(&items[2], 0, 0);
    }

    #[test]
    fn materializes_missing_map_to_empty_sentinel_array() {
        let registry = SlotShapeRegistry::default();
        let slot = ShaderSlotDef::map_u32_native(
            CONTROL_MESSAGE_SHAPE_NAME,
            ShaderSlotMappingDef::sentinel(2, "id", 0),
        );

        let value = materialize_shader_input("events", &slot, None, &registry).expect("input");

        let LpsValueF32::Array(items) = value else {
            panic!("expected array");
        };
        assert_eq!(items.len(), 2);
        assert_message(&items[0], 0, 0);
        assert_message(&items[1], 0, 0);
    }

    #[test]
    fn rejects_map_key_that_disagrees_with_item_key_field() {
        let registry = SlotShapeRegistry::default();
        let slot = ShaderSlotDef::map_u32_native(
            CONTROL_MESSAGE_SHAPE_NAME,
            ShaderSlotMappingDef::sentinel(3, "id", 0),
        );
        let data = SlotData::Map(SlotMapDyn::with_revision(
            Revision::new(4),
            VecMap::from([(
                SlotMapKey::U32(7),
                SlotData::Value(WithRevision::new(
                    Revision::new(4),
                    ControlMessage::new(8, 42).to_lp_value(),
                )),
            )]),
        ));

        let err = materialize_shader_input("events", &slot, Some(&data), &registry)
            .expect_err("mismatched key");

        assert!(matches!(
            err,
            ShaderInputMaterializeError::MismatchedKey { .. }
        ));
    }

    fn assert_message(value: &LpsValueF32, id: u32, seq: u32) {
        let LpsValueF32::Struct { fields, .. } = value else {
            panic!("expected struct");
        };
        assert!(matches!(
            fields.iter().find(|(name, _)| name == "id").map(|(_, v)| v),
            Some(LpsValueF32::U32(value)) if *value == id
        ));
        assert!(matches!(
            fields.iter().find(|(name, _)| name == "seq").map(|(_, v)| v),
            Some(LpsValueF32::U32(value)) if *value == seq
        ));
    }
}
