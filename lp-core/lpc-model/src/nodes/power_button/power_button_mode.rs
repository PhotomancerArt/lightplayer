//! How a power button's pin is read, and so which level wakes the device.

use alloc::string::ToString;
use serde::{Deserialize, Serialize};

use crate::{
    FromLpValue, LpType, LpValue, SlotEnumOption, SlotMeta, SlotShapeId, SlotValue, SlotValueShape,
    StaticLpType, StaticSlotEnumOption, StaticSlotMeta, StaticSlotValueShape,
    StaticValueEditorHint, ToLpValue, ValueEditorHint, ValueRootError,
};

/// The physical control a power button node reads.
///
/// The mode fixes the pin's electrical reading *and* the deep-sleep wake
/// level, which is why there is no separate wake field: a momentary button
/// wakes on the press that pulls its pin low, a latching switch wakes on the
/// level that means "on".
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PowerButtonMode {
    /// A momentary button to ground (internal pull-up, active low). A short
    /// press emits `click`; holding it for `hold_ms` powers off. Wakes when
    /// the pin goes low again.
    #[default]
    Hold,
    /// A latching switch that drives the pin high when on (internal
    /// pull-down, active high) — e.g. a series resistor from a switched
    /// supply rail. Off powers off; wakes when the pin goes high.
    Switch,
}

impl PowerButtonMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hold => "hold",
            Self::Switch => "switch",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "hold" => Some(Self::Hold),
            "switch" => Some(Self::Switch),
            _ => None,
        }
    }
}

impl ToLpValue for PowerButtonMode {
    fn to_lp_value(&self) -> LpValue {
        LpValue::String(self.as_str().to_string())
    }
}

impl FromLpValue for PowerButtonMode {
    fn from_lp_value(value: &LpValue) -> Result<Self, ValueRootError> {
        match value {
            LpValue::String(value) => {
                Self::parse(value).ok_or_else(|| ValueRootError::new("expected power button mode"))
            }
            other => Err(ValueRootError::new(alloc::format!(
                "expected String, got {other:?}"
            ))),
        }
    }
}

impl SlotValue for PowerButtonMode {
    const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name("PowerButtonMode");
    const STATIC_VALUE_SHAPE_DESCRIPTOR: Option<StaticSlotValueShape> =
        Some(StaticSlotValueShape {
            id: Self::SHAPE_ID,
            ty: StaticLpType::String,
            meta: StaticSlotMeta::EMPTY,
            editor: StaticValueEditorHint::Dropdown {
                options: &[
                    StaticSlotEnumOption {
                        value: "hold",
                        label: "Hold to power off",
                    },
                    StaticSlotEnumOption {
                        value: "switch",
                        label: "On/off switch",
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
                    SlotEnumOption::new("hold", "Hold to power off"),
                    SlotEnumOption::new("switch", "On/off switch"),
                ],
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_round_trips_through_its_wire_string() {
        for mode in [PowerButtonMode::Hold, PowerButtonMode::Switch] {
            let value = mode.to_lp_value();
            assert_eq!(PowerButtonMode::from_lp_value(&value).unwrap(), mode);
        }
        assert!(PowerButtonMode::parse("latch").is_none());
    }
}
