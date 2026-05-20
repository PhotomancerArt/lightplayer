//! Authored shader parameter metadata.

use crate::{
    LpValue, OptionSlot, PositiveF32, PositiveF32Slot, Ratio, RatioSlot, Slotted, ValueSlot,
};
use alloc::string::String;

/// Authored definition for one shader parameter.
#[derive(Debug, Clone, Default, PartialEq, Slotted)]
pub struct ShaderParamDef {
    pub label: ValueSlot<String>,
    pub description: ValueSlot<String>,
    pub value_type: ValueSlot<String>,
    pub default: RatioSlot,
    pub min: OptionSlot<ScalarHint>,
}

/// Simple numeric hint for scalar shader params.
#[derive(Debug, Clone, Default, PartialEq, Slotted)]
pub struct ScalarHint {
    pub value: PositiveF32Slot,
}

impl ShaderParamDef {
    pub fn new(label: &str, description: &str, default: f32, min: Option<f32>) -> Self {
        Self {
            label: ValueSlot::new(String::from(label)),
            description: ValueSlot::new(String::from(description)),
            value_type: ValueSlot::new(String::from("f32")),
            default: RatioSlot::new(Ratio(default)),
            min: min
                .map(ScalarHint::new)
                .map_or_else(OptionSlot::none, OptionSlot::some),
        }
    }

    pub fn default_value(&self) -> LpValue {
        LpValue::F32(self.default.value().0)
    }
}

impl ScalarHint {
    pub fn new(value: f32) -> Self {
        Self {
            value: PositiveF32Slot::new(PositiveF32(value)),
        }
    }
}
