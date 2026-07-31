//! Numeric mode a shader node is authored and compiled in.

use crate::{
    FromLpValue, LpType, LpValue, SlotEnumOption, SlotMeta, SlotShapeId, SlotValue, SlotValueShape,
    StaticLpType, StaticSlotEnumOption, StaticSlotMeta, StaticSlotValueShape,
    StaticValueEditorHint, ToLpValue, ValueEditorHint, ValueRootError,
};
use alloc::string::ToString;

/// How a shader's `float` arithmetic is represented.
///
/// This is the authored numeric contract for one shader node, and the only
/// arithmetic knob the model exposes. It replaced `GlslOpts`, which carried
/// three per-operator Q32 mode slots (`add_sub`, `mul`, `div`) whose
/// non-default alternatives existed only as debug probes; the shipped
/// configuration — wrapping add/sub/mul, reciprocal divide — is now hard-coded
/// in the compiler.
///
/// [`FloatMode::Float`] is the authored surface for native `f32`; codegen for
/// it is not implemented yet (see the f32 roadmap, M7/M9), so compiling a
/// shader in that mode currently fails rather than silently falling back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FloatMode {
    /// Q16.16 fixed point stored in a signed 32-bit integer. The shipped mode.
    #[default]
    Fixed,
    /// Native IEEE-754 single precision.
    Float,
}

impl FloatMode {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fixed => "fixed",
            Self::Float => "float",
        }
    }

    pub fn parse(value: &str) -> Result<Self, ValueRootError> {
        match value {
            "fixed" => Ok(Self::Fixed),
            "float" => Ok(Self::Float),
            other => Err(ValueRootError::new(alloc::format!(
                "unknown float mode {other:?}"
            ))),
        }
    }
}

impl ToLpValue for FloatMode {
    fn to_lp_value(&self) -> LpValue {
        LpValue::String(self.as_str().to_string())
    }
}

impl FromLpValue for FloatMode {
    fn from_lp_value(value: &LpValue) -> Result<Self, ValueRootError> {
        match value {
            LpValue::String(value) => Self::parse(value.as_str()),
            other => Err(ValueRootError::new(alloc::format!(
                "expected String, got {other:?}"
            ))),
        }
    }
}

impl SlotValue for FloatMode {
    const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name("FloatMode");
    const STATIC_VALUE_SHAPE_DESCRIPTOR: Option<StaticSlotValueShape> =
        Some(StaticSlotValueShape {
            id: Self::SHAPE_ID,
            ty: StaticLpType::String,
            meta: StaticSlotMeta::EMPTY,
            editor: StaticValueEditorHint::Dropdown {
                options: &[
                    StaticSlotEnumOption {
                        value: "float",
                        label: "Float",
                    },
                    StaticSlotEnumOption {
                        value: "fixed",
                        label: "Fixed",
                    },
                ],
            },
        });

    fn value_shape() -> SlotValueShape {
        SlotValueShape {
            id: Self::SHAPE_ID,
            ty: LpType::String,
            meta: SlotMeta::empty(),
            editor: ValueEditorHint::Dropdown {
                options: alloc::vec![
                    SlotEnumOption::new("float", "Float"),
                    SlotEnumOption::new("fixed", "Fixed"),
                ],
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_fixed() {
        assert_eq!(FloatMode::default(), FloatMode::Fixed);
    }

    #[test]
    fn round_trips_through_lp_value() {
        for mode in [FloatMode::Fixed, FloatMode::Float] {
            let value = mode.to_lp_value();
            assert_eq!(FloatMode::from_lp_value(&value).unwrap(), mode);
        }
    }

    #[test]
    fn parse_rejects_unknown() {
        assert!(FloatMode::parse("q32").is_err());
    }
}
