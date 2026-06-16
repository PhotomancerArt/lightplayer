//! Materialize compute shader ABI outputs into slot data.

use alloc::format;
use alloc::string::String;
use lp_collection::VecMap;

use lpc_model::{
    Revision, ShaderMapKeyDef, ShaderSlotDef, ShaderSlotKind, ShaderSlotMappingKind, SlotData,
    SlotMapDyn, SlotMapKey, WithRevision,
};
use lps_shared::LpsValueF32;

use crate::gfx::lps_value_f32_to_model_value;

/// Convert one produced shader ABI output into the semantic slot data shape.
pub fn materialize_produced_slot(
    slot_name: &str,
    slot: &ShaderSlotDef,
    value: &LpsValueF32,
    revision: Revision,
) -> Result<SlotData, ComputeMaterializeError> {
    match slot.kind.value() {
        ShaderSlotKind::Value => {
            let value = lps_value_f32_to_model_value(value).map_err(|e| {
                ComputeMaterializeError::Unsupported(format!(
                    "produced slot {slot_name:?} cannot convert value: {e}"
                ))
            })?;
            Ok(SlotData::Value(WithRevision::new(revision, value)))
        }
        ShaderSlotKind::Map => materialize_map_slot(slot_name, slot, value, revision),
    }
}

/// Failure materializing compute shader output into slot data.
#[derive(Debug)]
pub enum ComputeMaterializeError {
    MissingMapping(String),
    Unsupported(String),
    ExpectedArray(String),
    ExpectedStruct(String),
    MissingKeyField { slot: String, key: String },
    InvalidKey { slot: String, key: String },
}

impl core::fmt::Display for ComputeMaterializeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::MissingMapping(slot) => write!(f, "produced map slot {slot:?} missing mapping"),
            Self::Unsupported(message) => f.write_str(message),
            Self::ExpectedArray(slot) => write!(f, "produced map slot {slot:?} was not an array"),
            Self::ExpectedStruct(slot) => {
                write!(f, "produced map slot {slot:?} item was not a struct")
            }
            Self::MissingKeyField { slot, key } => {
                write!(
                    f,
                    "produced map slot {slot:?} item missing key field {key:?}"
                )
            }
            Self::InvalidKey { slot, key } => {
                write!(
                    f,
                    "produced map slot {slot:?} key field {key:?} is incompatible"
                )
            }
        }
    }
}

impl core::error::Error for ComputeMaterializeError {}

fn materialize_map_slot(
    slot_name: &str,
    slot: &ShaderSlotDef,
    value: &LpsValueF32,
    revision: Revision,
) -> Result<SlotData, ComputeMaterializeError> {
    let mapping = slot
        .mapping
        .data
        .as_ref()
        .ok_or_else(|| ComputeMaterializeError::MissingMapping(String::from(slot_name)))?;
    match mapping.kind.value() {
        ShaderSlotMappingKind::Sentinel => {}
    }
    let key_def = slot.key.data.as_ref().ok_or_else(|| {
        ComputeMaterializeError::Unsupported(format!("produced map slot {slot_name:?} missing key"))
    })?;
    let key_field = mapping.key.value().as_str();
    let empty_key = *mapping.empty_key.value();
    let LpsValueF32::Array(items) = value else {
        return Err(ComputeMaterializeError::ExpectedArray(String::from(
            slot_name,
        )));
    };

    let mut entries = VecMap::new();
    for item in items.iter() {
        let key = extract_key(slot_name, key_field, key_def.value(), item)?;
        if key == SlotMapKey::U32(empty_key) {
            continue;
        }
        let model = lps_value_f32_to_model_value(item).map_err(|e| {
            ComputeMaterializeError::Unsupported(format!(
                "produced map slot {slot_name:?} item cannot convert value: {e}"
            ))
        })?;
        entries.insert(key, SlotData::Value(WithRevision::new(revision, model)));
    }

    Ok(SlotData::Map(SlotMapDyn::with_revision(revision, entries)))
}

fn extract_key(
    slot_name: &str,
    key_field: &str,
    key_def: &ShaderMapKeyDef,
    value: &LpsValueF32,
) -> Result<SlotMapKey, ComputeMaterializeError> {
    let LpsValueF32::Struct { fields, .. } = value else {
        return Err(ComputeMaterializeError::ExpectedStruct(String::from(
            slot_name,
        )));
    };
    let key = fields
        .iter()
        .find_map(|(name, value)| (name == key_field).then_some(value))
        .ok_or_else(|| ComputeMaterializeError::MissingKeyField {
            slot: String::from(slot_name),
            key: String::from(key_field),
        })?;
    match (key_def, key) {
        (ShaderMapKeyDef::U32, LpsValueF32::U32(value)) => Ok(SlotMapKey::U32(*value)),
        _ => Err(ComputeMaterializeError::InvalidKey {
            slot: String::from(slot_name),
            key: String::from(key_field),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::boxed::Box;
    use alloc::string::String;
    use alloc::vec;
    use lpc_model::{LpValue, ShaderSlotMappingDef, SlotDataAccess};

    #[test]
    fn sentinel_array_materializes_to_slot_map() {
        let slot = ShaderSlotDef::map_u32_native(
            "lp::fluid::Emitter",
            ShaderSlotMappingDef::sentinel(2, "id", 0),
        );
        let value = LpsValueF32::Array(Box::new([emitter(0, 0.0), emitter(7, 0.25)]));

        let data =
            materialize_produced_slot("emitters", &slot, &value, Revision::new(4)).expect("map");

        let SlotData::Map(map) = data else {
            panic!("map");
        };
        assert_eq!(map.keys_revision, Revision::new(4));
        assert_eq!(map.entries.len(), 1);
        let SlotDataAccess::Value(value) = map.entries[&SlotMapKey::U32(7)].access() else {
            panic!("value");
        };
        assert_eq!(value.changed_at(), Revision::new(4));
        assert!(matches!(
            value.value(),
            LpValue::Struct { fields, .. } if fields.iter().any(|(name, value)| name == "id" && value == &LpValue::U32(7))
        ));
    }

    fn emitter(id: u32, x: f32) -> LpsValueF32 {
        LpsValueF32::Struct {
            name: Some(String::from("FluidEmitter")),
            fields: vec![
                (String::from("id"), LpsValueF32::U32(id)),
                (String::from("pos"), LpsValueF32::Vec2([x, 0.5])),
                (String::from("dir"), LpsValueF32::Vec2([1.0, 0.0])),
                (String::from("radius"), LpsValueF32::F32(0.1)),
                (String::from("color"), LpsValueF32::Vec3([1.0, 0.5, 0.25])),
                (String::from("velocity"), LpsValueF32::F32(0.2)),
                (String::from("intensity"), LpsValueF32::F32(0.8)),
            ],
        }
    }
}
