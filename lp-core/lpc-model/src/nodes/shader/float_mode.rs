//! Numeric mode a shader node is authored and compiled in.

use crate::{
    FromLpValue, LpType, LpValue, SlotEnumOption, SlotMeta, SlotShapeId, SlotValue, SlotValueShape,
    StaticLpType, StaticSlotEnumOption, StaticSlotMeta, StaticSlotValueShape,
    StaticValueEditorHint, ToLpValue, ValueEditorHint, ValueRootError,
};
use alloc::string::ToString;

/// A **pin** forcing one execution representation for a shader's `float`
/// arithmetic.
///
/// `float` is the authored semantics: a shader is written in floats, and the
/// number the author reasons about is a real number. This type does not choose
/// those semantics — it overrides how they are *executed*, which is otherwise
/// the target's own answer.
///
/// The pin therefore lives in an `OptionSlot` on
/// [`ShaderDef`](crate::ShaderDef) and
/// [`ComputeShaderDef`](crate::ComputeShaderDef), and **absence is the
/// interesting state**: an unpinned shader (no `float_mode` key in its JSON)
/// runs the target's native representation — Q32 on every shipping CPU
/// backend today, `F32Gpu` on the GPU tier. Auto is that absence, never a
/// third variant here: the compiler must always receive a concrete mode, so a
/// mode meaning "decide later" would leak an undecidable value into it.
///
/// This is the only arithmetic knob the model exposes. It replaced
/// `GlslOpts`, which carried three per-operator Q32 mode slots (`add_sub`,
/// `mul`, `div`) whose non-default alternatives existed only as debug probes;
/// the shipped configuration — wrapping add/sub/mul, reciprocal divide — is
/// now hard-coded in the compiler.
///
/// [`FloatMode::Float`] is the authored surface for native `f32`. It reaches
/// the compiler as a per-shader parameter
/// (`docs/adr/2026-08-01-float-mode-as-a-compiler-parameter.md`), and what it
/// becomes depends on the board: hardware FPU instructions on an ESP32-S3,
/// soft-float calls on a target without one, and a **compile error** on a
/// build that linked neither. It never silently falls back to
/// [`FloatMode::Fixed`] — a board quietly given different numerics than the
/// author asked for is the failure this refuses
/// (`docs/adr/2026-07-09-preview-fidelity-tiers.md` §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FloatMode {
    /// Q16.16 fixed point stored in a signed 32-bit integer. Today's native
    /// representation on every CPU backend, so pinning it is a no-op there;
    /// it exists as a pin for the day one is not.
    #[default]
    Fixed,
    /// Native IEEE-754 single precision.
    Float,
}

/// Row label for the pin in the advanced drawer.
const FLOAT_MODE_LABEL: &str = "Float mode";

/// Detail line the drawer shows beside the pin. It has to carry what the
/// dropdown cannot: that leaving the row unset is the normal, recommended
/// state, and what unset actually does.
const FLOAT_MODE_DESCRIPTION: &str = concat!(
    "Unset = Auto (target default): the target's native representation. ",
    "Set this only to force one representation regardless of target."
);

/// Dropdown label for [`FloatMode::Fixed`]. Names the representation, not a
/// preference — the author is picking how the floats are executed.
const FIXED_LABEL: &str = "Fixed (Q32)";

/// Dropdown label for [`FloatMode::Float`].
const FLOAT_LABEL: &str = "Float (f32)";

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
            meta: StaticSlotMeta {
                label: Some(FLOAT_MODE_LABEL),
                description: Some(FLOAT_MODE_DESCRIPTION),
                unit: None,
            },
            editor: StaticValueEditorHint::Dropdown {
                options: &[
                    StaticSlotEnumOption {
                        value: "float",
                        label: FLOAT_LABEL,
                    },
                    StaticSlotEnumOption {
                        value: "fixed",
                        label: FIXED_LABEL,
                    },
                ],
            },
        });

    fn value_shape() -> SlotValueShape {
        SlotValueShape {
            id: Self::SHAPE_ID,
            ty: LpType::String,
            meta: SlotMeta {
                label: Some(FLOAT_MODE_LABEL.to_string()),
                description: Some(FLOAT_MODE_DESCRIPTION.to_string()),
                unit: None,
            },
            editor: ValueEditorHint::Dropdown {
                options: alloc::vec![
                    SlotEnumOption::new("float", FLOAT_LABEL),
                    SlotEnumOption::new("fixed", FIXED_LABEL),
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
