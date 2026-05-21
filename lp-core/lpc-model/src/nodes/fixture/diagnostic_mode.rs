//! Fixture hardware diagnostic output modes.

use alloc::string::ToString;
use serde::{Deserialize, Serialize};

use crate::{
    FromLpValue, LpType, LpValue, SlotEnumOption, SlotMeta, SlotShapeId, SlotValue, SlotValueShape,
    ToLpValue, ValueEditorHint, ValueRootError,
};

/// Fixture-level hardware diagnostic pattern.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FixtureDiagnosticMode {
    /// Render the fixture's normal visual input.
    #[default]
    Off,
    /// High-contrast per-LED identity colors with 5/10 markers.
    LedIndex,
    /// Color LEDs in countable groups of ten.
    Groups10,
    /// Animate a single bright index marker through the fixture.
    Chase,
}

impl FixtureDiagnosticMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::LedIndex => "led_index",
            Self::Groups10 => "groups_10",
            Self::Chase => "chase",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "off" => Some(Self::Off),
            "led_index" => Some(Self::LedIndex),
            "groups_10" => Some(Self::Groups10),
            "chase" => Some(Self::Chase),
            _ => None,
        }
    }
}

impl ToLpValue for FixtureDiagnosticMode {
    fn to_lp_value(&self) -> LpValue {
        LpValue::String(self.as_str().to_string())
    }
}

impl FromLpValue for FixtureDiagnosticMode {
    fn from_lp_value(value: &LpValue) -> Result<Self, ValueRootError> {
        match value {
            LpValue::String(value) => Self::parse(value)
                .ok_or_else(|| ValueRootError::new("expected fixture diagnostic mode")),
            other => Err(ValueRootError::new(alloc::format!(
                "expected String, got {other:?}"
            ))),
        }
    }
}

impl SlotValue for FixtureDiagnosticMode {
    const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name("FixtureDiagnosticMode");

    fn value_shape() -> SlotValueShape {
        SlotValueShape {
            id: Self::SHAPE_ID,
            ty: LpType::String,
            meta: SlotMeta::empty(),
            editor: ValueEditorHint::Dropdown {
                options: alloc::vec![
                    SlotEnumOption::new("off", "Off"),
                    SlotEnumOption::new("led_index", "LED index"),
                    SlotEnumOption::new("groups_10", "Groups of 10"),
                    SlotEnumOption::new("chase", "Chase"),
                ],
            },
        }
    }
}
