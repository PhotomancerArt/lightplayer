//! Authored shader slot definitions.
//!
//! A shader slot definition describes the semantic data a shader consumes or
//! produces. It is separate from shader source text and can describe native
//! LightPlayer shapes such as `lp::fluid::Emitter` without copying their field
//! structure into every shader artifact.

use crate::{
    FromLpValue, LpType, LpValue, OptionSlot, SlotMeta, SlotShapeId, SlotValue, SlotValueShape,
    Slotted, StaticLpType, StaticSlotValueShape, ToLpValue, ValueEditorHint, ValueRootError,
    ValueSlot,
};
use alloc::string::{String, ToString};
use serde::{Deserialize, Serialize};

use super::ShaderSlotMappingDef;

/// Authored definition for one shader consumed or produced slot.
#[derive(Debug, Clone, PartialEq, Slotted)]
pub struct ShaderSlotDef {
    pub kind: ValueSlot<ShaderSlotKind>,
    pub value: ValueSlot<ShaderValueShapeRef>,
    pub key: OptionSlot<ValueSlot<ShaderMapKeyDef>>,
    pub default: OptionSlot<ValueSlot<f32>>,
    pub min: OptionSlot<ValueSlot<f32>>,
    pub mapping: OptionSlot<ShaderSlotMappingDef>,
    pub label: ValueSlot<String>,
    pub description: ValueSlot<String>,
}

impl ShaderSlotDef {
    pub fn value_f32(label: &str, description: &str, default: f32, min: Option<f32>) -> Self {
        Self {
            kind: ValueSlot::new(ShaderSlotKind::Value),
            value: ValueSlot::new(ShaderValueShapeRef::builtin("f32")),
            key: OptionSlot::none(),
            default: OptionSlot::some(ValueSlot::new(default)),
            min: min
                .map(ValueSlot::new)
                .map_or_else(OptionSlot::none, OptionSlot::some),
            mapping: OptionSlot::none(),
            label: ValueSlot::new(String::from(label)),
            description: ValueSlot::new(String::from(description)),
        }
    }

    pub fn map_u32_native(value: &str, mapping: ShaderSlotMappingDef) -> Self {
        Self {
            kind: ValueSlot::new(ShaderSlotKind::Map),
            value: ValueSlot::new(ShaderValueShapeRef::native(value)),
            key: OptionSlot::some(ValueSlot::new(ShaderMapKeyDef::U32)),
            default: OptionSlot::none(),
            min: OptionSlot::none(),
            mapping: OptionSlot::some(mapping),
            label: ValueSlot::default(),
            description: ValueSlot::default(),
        }
    }

    pub fn default_value(&self) -> LpValue {
        self.default
            .data
            .as_ref()
            .map_or(LpValue::F32(0.0), |value| LpValue::F32(*value.value()))
    }

    pub fn value_lp_type(&self) -> Option<LpType> {
        self.value.value().as_lp_type()
    }
}

impl Default for ShaderSlotDef {
    fn default() -> Self {
        Self::value_f32("", "", 0.0, None)
    }
}

/// Top-level shader slot shape kind.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ShaderSlotKind {
    #[default]
    Value,
    Map,
}

impl ShaderSlotKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Value => "value",
            Self::Map => "map",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "value" => Some(Self::Value),
            "map" => Some(Self::Map),
            _ => None,
        }
    }
}

/// Supported map key types for M1 shader slots.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ShaderMapKeyDef {
    #[default]
    U32,
}

impl ShaderMapKeyDef {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::U32 => "u32",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "u32" => Some(Self::U32),
            _ => None,
        }
    }
}

/// Reference to a shader slot value shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShaderValueShapeRef {
    name: String,
}

impl ShaderValueShapeRef {
    pub fn builtin(name: &str) -> Self {
        Self {
            name: String::from(name),
        }
    }

    pub fn native(name: &str) -> Self {
        Self {
            name: String::from(name),
        }
    }

    pub fn as_str(&self) -> &str {
        &self.name
    }

    pub fn is_native(&self) -> bool {
        self.name.starts_with("lp::")
    }

    pub fn as_lp_type(&self) -> Option<LpType> {
        match self.name.as_str() {
            "f32" => Some(LpType::F32),
            "u32" => Some(LpType::U32),
            "i32" => Some(LpType::I32),
            "bool" => Some(LpType::Bool),
            "vec2" => Some(LpType::Vec2),
            "vec3" => Some(LpType::Vec3),
            "vec4" => Some(LpType::Vec4),
            _ => None,
        }
    }
}

impl Default for ShaderValueShapeRef {
    fn default() -> Self {
        Self::builtin("f32")
    }
}

macro_rules! impl_string_leaf {
    ($ty:ty, $shape_id:literal, $expected:literal, $parse:expr, $as_str:expr) => {
        impl Serialize for $ty {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                serializer.serialize_str($as_str(self))
            }
        }

        impl<'de> Deserialize<'de> for $ty {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                $parse(&value)
                    .ok_or_else(|| serde::de::Error::custom(alloc::format!("unknown {value:?}")))
            }
        }

        impl ToLpValue for $ty {
            fn to_lp_value(&self) -> LpValue {
                LpValue::String($as_str(self).to_string())
            }
        }

        impl FromLpValue for $ty {
            fn from_lp_value(value: &LpValue) -> Result<Self, ValueRootError> {
                match value {
                    LpValue::String(value) => {
                        $parse(value).ok_or_else(|| ValueRootError::new($expected))
                    }
                    other => Err(ValueRootError::new(alloc::format!(
                        "expected String, got {other:?}"
                    ))),
                }
            }
        }

        impl SlotValue for $ty {
            const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name($shape_id);
            const STATIC_VALUE_SHAPE_DESCRIPTOR: Option<StaticSlotValueShape> = Some(
                StaticSlotValueShape::new(Self::SHAPE_ID, StaticLpType::String),
            );

            fn value_shape() -> SlotValueShape {
                SlotValueShape {
                    id: Self::SHAPE_ID,
                    ty: LpType::String,
                    meta: SlotMeta::empty(),
                    editor: ValueEditorHint::Plain,
                }
            }
        }
    };
}

impl_string_leaf!(
    ShaderSlotKind,
    "slot.leaf.shader_slot_kind",
    "expected shader slot kind",
    ShaderSlotKind::parse,
    |value: &ShaderSlotKind| value.as_str()
);

impl_string_leaf!(
    ShaderMapKeyDef,
    "slot.leaf.shader_map_key",
    "expected shader map key",
    ShaderMapKeyDef::parse,
    |value: &ShaderMapKeyDef| value.as_str()
);

impl Serialize for ShaderValueShapeRef {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ShaderValueShapeRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        if value.is_empty() {
            return Err(serde::de::Error::custom(
                "shader value shape cannot be empty",
            ));
        }
        Ok(Self { name: value })
    }
}

impl ToLpValue for ShaderValueShapeRef {
    fn to_lp_value(&self) -> LpValue {
        LpValue::String(self.name.clone())
    }
}

impl FromLpValue for ShaderValueShapeRef {
    fn from_lp_value(value: &LpValue) -> Result<Self, ValueRootError> {
        match value {
            LpValue::String(value) if !value.is_empty() => Ok(Self {
                name: value.clone(),
            }),
            LpValue::String(_) => Err(ValueRootError::new("shader value shape cannot be empty")),
            other => Err(ValueRootError::new(alloc::format!(
                "expected String, got {other:?}"
            ))),
        }
    }
}

impl SlotValue for ShaderValueShapeRef {
    const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name("slot.leaf.shader_value_shape_ref");
    const STATIC_VALUE_SHAPE_DESCRIPTOR: Option<StaticSlotValueShape> = Some(
        StaticSlotValueShape::new(Self::SHAPE_ID, StaticLpType::String),
    );

    fn value_shape() -> SlotValueShape {
        SlotValueShape {
            id: Self::SHAPE_ID,
            ty: LpType::String,
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
    fn value_shader_slot_parses_old_param_shape() {
        let slot = read_slot_def(
            r#"kind = "value"
value = "f32"
default = 1.0
min = 0.0
label = "Exposure"
description = "Output exposure multiplier"
"#,
        );

        assert_eq!(*slot.kind.value(), ShaderSlotKind::Value);
        assert_eq!(slot.value.value().as_str(), "f32");
        assert_eq!(slot.default_value(), LpValue::F32(1.0));
    }

    #[test]
    fn map_shader_slot_parses_native_value_mapping() {
        let slot = read_slot_def(
            r#"kind = "map"
key = "u32"
value = "lp::fluid::Emitter"
mapping = { kind = "sentinel", len = 4, key = "id", empty_key = 0 }
"#,
        );

        assert_eq!(*slot.kind.value(), ShaderSlotKind::Map);
        assert_eq!(slot.value.value().as_str(), "lp::fluid::Emitter");
        assert!(slot.value.value().is_native());
        assert!(slot.mapping.data.is_some());
    }

    fn read_slot_def(text: &str) -> ShaderSlotDef {
        let registry = SlotShapeRegistry::default();
        let value = toml::from_str::<toml::Value>(text).unwrap();
        registry
            .read_slot_toml(ShaderSlotDef::SHAPE_ID, &value)
            .expect("slot")
            .into_any()
            .downcast::<ShaderSlotDef>()
            .map(|def| *def)
            .expect("shader slot def")
    }
}
