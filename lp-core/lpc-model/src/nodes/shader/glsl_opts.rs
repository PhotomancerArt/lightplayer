//! GLSL compilation options (per-shader-node)

use crate::{
    FromLpValue, LpType, LpValue, SlotEnumOption, SlotMeta, SlotRecord, SlotShapeId, SlotValue,
    SlotValueShape, ToLpValue, ValueEditorHint, ValueRootError, ValueSlot,
};
use alloc::string::ToString;
use serde::{Deserialize, Serialize};

/// Mode for Q32 add/sub: wrapping (inline iadd/isub) or saturating (debug/reference)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AddSubMode {
    /// __lp_q32_add/sub: saturates on overflow
    Saturating,
    /// Inline iadd/isub: wraps on overflow, faster
    #[default]
    Wrapping,
}

impl AddSubMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Saturating => "saturating",
            Self::Wrapping => "wrapping",
        }
    }

    pub fn parse(value: &str) -> Result<Self, ValueRootError> {
        match value {
            "saturating" => Ok(Self::Saturating),
            "wrapping" => Ok(Self::Wrapping),
            other => Err(ValueRootError::new(alloc::format!(
                "unknown add/sub mode {other:?}"
            ))),
        }
    }
}

/// Mode for Q32 mul: wrapping (inline imul+smulhi) or saturating (debug/reference)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MulMode {
    /// __lp_q32_mul: saturates on overflow
    Saturating,
    /// Inline imul+smulhi: wraps on overflow, faster
    #[default]
    Wrapping,
}

impl MulMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Saturating => "saturating",
            Self::Wrapping => "wrapping",
        }
    }

    pub fn parse(value: &str) -> Result<Self, ValueRootError> {
        match value {
            "saturating" => Ok(Self::Saturating),
            "wrapping" => Ok(Self::Wrapping),
            other => Err(ValueRootError::new(alloc::format!(
                "unknown mul mode {other:?}"
            ))),
        }
    }
}

/// Mode for Q32 div: reciprocal approximation or saturating reference divide
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DivMode {
    /// __lp_q32_div: exact, saturates on div-by-zero
    Saturating,
    /// Reciprocal multiplication: ~0.01% typical error, faster
    #[default]
    Reciprocal,
}

impl DivMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Saturating => "saturating",
            Self::Reciprocal => "reciprocal",
        }
    }

    pub fn parse(value: &str) -> Result<Self, ValueRootError> {
        match value {
            "saturating" => Ok(Self::Saturating),
            "reciprocal" => Ok(Self::Reciprocal),
            other => Err(ValueRootError::new(alloc::format!(
                "unknown div mode {other:?}"
            ))),
        }
    }
}

/// GLSL compilation options (per-shader-node)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, SlotRecord)]
pub struct GlslOpts {
    #[serde(default)]
    pub add_sub: ValueSlot<AddSubMode>,
    #[serde(default)]
    pub mul: ValueSlot<MulMode>,
    #[serde(default)]
    pub div: ValueSlot<DivMode>,
}

impl Default for GlslOpts {
    fn default() -> Self {
        Self {
            add_sub: ValueSlot::new(AddSubMode::Wrapping),
            mul: ValueSlot::new(MulMode::Wrapping),
            div: ValueSlot::new(DivMode::Reciprocal),
        }
    }
}

impl ToLpValue for AddSubMode {
    fn to_lp_value(&self) -> LpValue {
        LpValue::String(self.as_str().to_string())
    }
}

impl FromLpValue for AddSubMode {
    fn from_lp_value(value: &LpValue) -> Result<Self, ValueRootError> {
        string_lp_value(value).and_then(|value| Self::parse(&value))
    }
}

impl SlotValue for AddSubMode {
    const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name("slot.leaf.glsl_add_sub_mode");

    fn value_shape() -> SlotValueShape {
        mode_shape(
            Self::SHAPE_ID,
            &[("saturating", "Saturating"), ("wrapping", "Wrapping")],
        )
    }
}

impl ToLpValue for MulMode {
    fn to_lp_value(&self) -> LpValue {
        LpValue::String(self.as_str().to_string())
    }
}

impl FromLpValue for MulMode {
    fn from_lp_value(value: &LpValue) -> Result<Self, ValueRootError> {
        string_lp_value(value).and_then(|value| Self::parse(&value))
    }
}

impl SlotValue for MulMode {
    const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name("slot.leaf.glsl_mul_mode");

    fn value_shape() -> SlotValueShape {
        mode_shape(
            Self::SHAPE_ID,
            &[("saturating", "Saturating"), ("wrapping", "Wrapping")],
        )
    }
}

impl ToLpValue for DivMode {
    fn to_lp_value(&self) -> LpValue {
        LpValue::String(self.as_str().to_string())
    }
}

impl FromLpValue for DivMode {
    fn from_lp_value(value: &LpValue) -> Result<Self, ValueRootError> {
        string_lp_value(value).and_then(|value| Self::parse(&value))
    }
}

impl SlotValue for DivMode {
    const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name("slot.leaf.glsl_div_mode");

    fn value_shape() -> SlotValueShape {
        mode_shape(
            Self::SHAPE_ID,
            &[("saturating", "Saturating"), ("reciprocal", "Reciprocal")],
        )
    }
}

fn string_lp_value(value: &LpValue) -> Result<&str, ValueRootError> {
    match value {
        LpValue::String(value) => Ok(value.as_str()),
        other => Err(ValueRootError::new(alloc::format!(
            "expected String, got {other:?}"
        ))),
    }
}

fn mode_shape(id: SlotShapeId, options: &[(&str, &str)]) -> SlotValueShape {
    SlotValueShape {
        id,
        ty: LpType::String,
        meta: SlotMeta::empty(),
        editor: ValueEditorHint::Dropdown {
            options: options
                .iter()
                .map(|(value, label)| SlotEnumOption::new(value, label))
                .collect(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glsl_opts_default() {
        let opts = GlslOpts::default();
        assert_eq!(*opts.add_sub.value(), AddSubMode::Wrapping);
        assert_eq!(*opts.mul.value(), MulMode::Wrapping);
        assert_eq!(*opts.div.value(), DivMode::Reciprocal);
    }
}
