//! Shader-visible mapping for semantic shader slots.
//!
//! Shader slots describe semantic LightPlayer data such as maps. GLSL cannot
//! represent every semantic shape directly, so a slot can define how its data is
//! mapped into shader-visible ABI storage.

use crate::{
    LpValue, SlotMeta, SlotShapeId, SlotValue, SlotValueShape, Slotted, StaticLpType,
    StaticSlotValueShape, ToLpValue, ValueEditorHint, ValueRootError,
};
use alloc::string::{String, ToString};
use serde::{Deserialize, Serialize};

/// Mapping from a semantic shader slot into shader-visible ABI storage.
#[derive(Debug, Clone, PartialEq, Slotted)]
pub struct ShaderSlotMappingDef {
    pub kind: crate::ValueSlot<ShaderSlotMappingKind>,
    pub len: crate::ValueSlot<u32>,
    pub key: crate::ValueSlot<String>,
    pub empty_key: crate::ValueSlot<u32>,
}

impl ShaderSlotMappingDef {
    pub fn sentinel(len: u32, key: &str, empty_key: u32) -> Self {
        Self {
            kind: crate::ValueSlot::new(ShaderSlotMappingKind::Sentinel),
            len: crate::ValueSlot::new(len),
            key: crate::ValueSlot::new(String::from(key)),
            empty_key: crate::ValueSlot::new(empty_key),
        }
    }
}

impl Default for ShaderSlotMappingDef {
    fn default() -> Self {
        Self::sentinel(0, "", 0)
    }
}

/// Supported M1 shader slot mapping strategies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShaderSlotMappingKind {
    Sentinel,
}

impl ShaderSlotMappingKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sentinel => "sentinel",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "sentinel" => Some(Self::Sentinel),
            _ => None,
        }
    }
}

impl Serialize for ShaderSlotMappingKind {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ShaderSlotMappingKind {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).ok_or_else(|| {
            serde::de::Error::custom(alloc::format!("unknown shader slot mapping {value:?}"))
        })
    }
}

impl ToLpValue for ShaderSlotMappingKind {
    fn to_lp_value(&self) -> LpValue {
        LpValue::String(self.as_str().to_string())
    }
}

impl crate::FromLpValue for ShaderSlotMappingKind {
    fn from_lp_value(value: &LpValue) -> Result<Self, ValueRootError> {
        match value {
            LpValue::String(value) => Self::parse(value)
                .ok_or_else(|| ValueRootError::new("expected shader slot mapping kind")),
            other => Err(ValueRootError::new(alloc::format!(
                "expected String, got {other:?}"
            ))),
        }
    }
}

impl SlotValue for ShaderSlotMappingKind {
    const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name("slot.leaf.shader_slot_mapping");
    const STATIC_VALUE_SHAPE_DESCRIPTOR: Option<StaticSlotValueShape> = Some(
        StaticSlotValueShape::new(Self::SHAPE_ID, StaticLpType::String),
    );

    fn value_shape() -> SlotValueShape {
        SlotValueShape {
            id: Self::SHAPE_ID,
            ty: crate::LpType::String,
            meta: SlotMeta::empty(),
            editor: ValueEditorHint::Plain,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SlotShapeRegistry, StaticSlotShape};

    #[test]
    fn sentinel_mapping_round_trips_from_inline_json() {
        let mapping = read_mapping(
            r#"{ "kind": "sentinel", "len": 4, "key": "id", "empty_key": 0 }"#,
        );

        assert_eq!(*mapping.kind.value(), ShaderSlotMappingKind::Sentinel);
        assert_eq!(*mapping.len.value(), 4);
        assert_eq!(mapping.key.value(), "id");
        assert_eq!(*mapping.empty_key.value(), 0);
    }

    fn read_mapping(text: &str) -> ShaderSlotMappingDef {
        let registry = SlotShapeRegistry::default();
        registry
            .read_slot_json(ShaderSlotMappingDef::SHAPE_ID, text)
            .expect("mapping")
            .into_any()
            .downcast::<ShaderSlotMappingDef>()
            .map(|def| *def)
            .expect("shader slot mapping def")
    }
}
