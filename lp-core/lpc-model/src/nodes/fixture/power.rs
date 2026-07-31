//! A fixture's power supply: what it drives, and how much current it may draw.
//!
//! # Why a struct and not a bare milliamp figure
//!
//! A fixture-wide budget is the simple case of a richer model: groups of LEDs
//! assigned to their own supplies, owned by the fixture mapper. That richer
//! model is future work, but it arrives as an added field here rather than as a
//! replacement for a scalar, which keeps it a non-breaking change.
//!
//! # Absent means unlimited
//!
//! The `power` slot on a fixture is optional, and its absence means no limiting
//! at all. There is no default budget because a wrong guess is worse than no
//! guess in both directions: too low throttles a working installation, too high
//! gives false confidence. Stating a budget is the user's job.

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

use super::lamp_type::LampType;
use crate::{
    FromLpValue, LpType, LpValue, ModelStructMember, SlotMeta, SlotShapeId, SlotValue,
    SlotValueShape, StaticLpType, StaticModelStructMember, StaticSlotMeta, StaticSlotValueShape,
    StaticValueEditorHint, ToLpValue, ValueEditorHint, ValueRootError,
};

/// The supply a fixture's lamps run from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct FixturePower {
    /// The lamp part this fixture drives. Selects the power model.
    pub lamp_type: LampType,
    /// What the supply can deliver, in milliamps.
    pub budget_ma: u32,
}

impl FixturePower {
    /// Budget used when the slot is added without one being stated.
    ///
    /// Sized for a common small 5V supply. Erring low is deliberate: an
    /// under-stated budget dims a fixture until someone corrects it, while an
    /// over-stated one lets the original failure through.
    pub const DEFAULT_BUDGET_MA: u32 = 2000;

    const FIELD_LAMP_TYPE: &'static str = "lamp_type";
    const FIELD_BUDGET_MA: &'static str = "budget_ma";
}

impl Default for FixturePower {
    fn default() -> Self {
        Self {
            lamp_type: LampType::default(),
            budget_ma: Self::DEFAULT_BUDGET_MA,
        }
    }
}

impl ToLpValue for FixturePower {
    fn to_lp_value(&self) -> LpValue {
        LpValue::Struct {
            name: Some(String::from("FixturePower")),
            fields: Vec::from([
                (
                    String::from(Self::FIELD_LAMP_TYPE),
                    self.lamp_type.to_lp_value(),
                ),
                (
                    String::from(Self::FIELD_BUDGET_MA),
                    LpValue::U32(self.budget_ma),
                ),
            ]),
        }
    }
}

impl FromLpValue for FixturePower {
    fn from_lp_value(value: &LpValue) -> Result<Self, ValueRootError> {
        let LpValue::Struct { name, fields } = value else {
            return Err(ValueRootError::new("expected FixturePower struct"));
        };
        if name.as_deref() != Some("FixturePower") || fields.len() != 2 {
            return Err(ValueRootError::new("expected FixturePower struct"));
        }
        let lamp_type = match fields.first() {
            Some((name, value)) if name == Self::FIELD_LAMP_TYPE => LampType::from_lp_value(value)?,
            _ => return Err(ValueRootError::new("expected FixturePower.lamp_type")),
        };
        let budget_ma = match fields.get(1) {
            Some((name, value)) if name == Self::FIELD_BUDGET_MA => u32::from_lp_value(value)?,
            _ => return Err(ValueRootError::new("expected FixturePower.budget_ma")),
        };
        Ok(Self {
            lamp_type,
            budget_ma,
        })
    }
}

impl SlotValue for FixturePower {
    const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name("lp::fixture::FixturePower");
    const STATIC_VALUE_SHAPE_DESCRIPTOR: Option<StaticSlotValueShape> =
        Some(StaticSlotValueShape {
            id: Self::SHAPE_ID,
            ty: StaticLpType::Struct {
                name: Some("FixturePower"),
                fields: &[
                    StaticModelStructMember {
                        name: FixturePower::FIELD_LAMP_TYPE,
                        ty: StaticLpType::String,
                    },
                    StaticModelStructMember {
                        name: FixturePower::FIELD_BUDGET_MA,
                        ty: StaticLpType::U32,
                    },
                ],
            },
            meta: StaticSlotMeta {
                label: Some("Power"),
                description: Some("Lamp type and supply budget used to limit output current."),
                panel: false,
                unit: None,
            },
            editor: StaticValueEditorHint::Plain,
        });

    fn value_shape() -> SlotValueShape {
        SlotValueShape {
            id: Self::SHAPE_ID,
            ty: LpType::Struct {
                name: Some(String::from("FixturePower")),
                fields: Vec::from([
                    ModelStructMember {
                        name: String::from(Self::FIELD_LAMP_TYPE),
                        ty: LpType::String,
                    },
                    ModelStructMember {
                        name: String::from(Self::FIELD_BUDGET_MA),
                        ty: LpType::U32,
                    },
                ]),
            },
            meta: SlotMeta {
                label: Some("Power".to_string()),
                description: Some(
                    "Lamp type and supply budget used to limit output current.".to_string(),
                ),
                panel: false,
                unit: None,
            },
            editor: ValueEditorHint::Plain,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_lp_value() {
        let power = FixturePower {
            lamp_type: LampType::Ws281512v,
            budget_ma: 4000,
        };
        let value = power.to_lp_value();
        assert_eq!(FixturePower::from_lp_value(&value).expect("decodes"), power);
    }

    #[test]
    fn json_form_is_a_plain_object() {
        let power = FixturePower {
            lamp_type: LampType::Ws2812b5v,
            budget_ma: 2500,
        };
        let json = serde_json::to_string(&power).expect("encodes");
        assert_eq!(json, r#"{"lamp_type":"ws2812b_5v","budget_ma":2500}"#);
        assert_eq!(
            serde_json::from_str::<FixturePower>(&json).expect("decodes"),
            power
        );
    }

    #[test]
    fn wrong_shape_is_rejected() {
        assert!(FixturePower::from_lp_value(&LpValue::U32(5)).is_err());
        assert!(
            FixturePower::from_lp_value(&LpValue::Struct {
                name: Some(String::from("SomethingElse")),
                fields: Vec::new(),
            })
            .is_err()
        );
    }

    #[test]
    fn static_and_owned_shapes_agree() {
        let shape = FixturePower::value_shape();
        let static_shape =
            FixturePower::STATIC_VALUE_SHAPE_DESCRIPTOR.expect("static descriptor exists");
        assert_eq!(static_shape.to_owned_value_shape(), shape);
    }
}
