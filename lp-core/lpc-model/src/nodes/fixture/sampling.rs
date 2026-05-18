//! Fixture visual sampling strategy.

use alloc::string::ToString;
use serde::{Deserialize, Serialize};

use crate::{
    FromLpValue, LpType, LpValue, SlotMeta, SlotShapeId, SlotValue, SlotValueShape, ToLpValue,
    ValueEditorHint, ValueRootError,
};

/// How a fixture evaluates its input visual product before writing control samples.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FixtureSamplingConfig {
    /// Sample the shader directly once per fixture lamp.
    Direct,
    /// Render the visual product to a texture, then area-sample the texture.
    #[default]
    TextureArea,
}

impl FixtureSamplingConfig {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::TextureArea => "texture_area",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "direct" => Some(Self::Direct),
            "texture_area" => Some(Self::TextureArea),
            _ => None,
        }
    }
}

impl ToLpValue for FixtureSamplingConfig {
    fn to_lp_value(&self) -> LpValue {
        LpValue::String(self.as_str().to_string())
    }
}

impl FromLpValue for FixtureSamplingConfig {
    fn from_lp_value(value: &LpValue) -> Result<Self, ValueRootError> {
        match value {
            LpValue::String(value) => {
                Self::parse(value).ok_or_else(|| ValueRootError::new("expected fixture sampling"))
            }
            other => Err(ValueRootError::new(alloc::format!(
                "expected String, got {other:?}"
            ))),
        }
    }
}

impl SlotValue for FixtureSamplingConfig {
    const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name("FixtureSamplingConfig");

    fn value_shape() -> SlotValueShape {
        SlotValueShape {
            id: Self::SHAPE_ID,
            ty: LpType::String,
            meta: SlotMeta::empty(),
            editor: ValueEditorHint::Plain,
        }
    }
}
