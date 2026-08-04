//! Fixture brightness level (0–255) — the fixture card's front-panel fader.
//!
//! A `u32` newtype whose slot value shape carries the presentation bucket
//! the generic `u32` leaf cannot: `SlotMeta::panel` marks the slot for the
//! node face's control panel (node-card Q3) and the `Slider` editor hint
//! supplies the fader's range. Wire form and def JSON stay a plain unsigned
//! integer (`#[serde(transparent)]`, `LpValue::U32`).

use serde::{Deserialize, Serialize};

use crate::{
    FromLpValue, LpType, LpValue, OrderedF32, SlotMeta, SlotShapeId, SlotValue, SlotValueShape,
    StaticLpType, StaticSlotMeta, StaticSlotValueShape, StaticValueEditorHint, ToLpValue,
    ValueEditorHint, ValueRootError,
};

/// Fixture brightness level (0–255).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(transparent)]
pub struct Brightness(pub u32);

impl Brightness {
    /// Default brightness level when none is authored.
    pub const DEFAULT: Self = Self(64);

    /// Clamped 8-bit brightness value.
    pub fn as_u8(self) -> u8 {
        u8::try_from(self.0).unwrap_or(u8::MAX)
    }
}

impl Default for Brightness {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl ToLpValue for Brightness {
    fn to_lp_value(&self) -> LpValue {
        LpValue::U32(self.0)
    }
}

impl FromLpValue for Brightness {
    fn from_lp_value(value: &LpValue) -> Result<Self, ValueRootError> {
        u32::from_lp_value(value).map(Self)
    }
}

impl SlotValue for Brightness {
    const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name("lp::fixture::Brightness");
    const STATIC_VALUE_SHAPE_DESCRIPTOR: Option<StaticSlotValueShape> =
        Some(StaticSlotValueShape {
            id: Self::SHAPE_ID,
            ty: StaticLpType::U32,
            meta: StaticSlotMeta {
                label: Some("Brightness"),
                description: Some("Fixture output brightness (0-255)."),
                unit: None,
            },
            editor: StaticValueEditorHint::Slider {
                min: OrderedF32(0.0),
                max: OrderedF32(255.0),
                step: Some(OrderedF32(1.0)),
            },
        });

    fn value_shape() -> SlotValueShape {
        SlotValueShape {
            id: Self::SHAPE_ID,
            ty: LpType::U32,
            meta: SlotMeta {
                label: Some("Brightness".into()),
                description: Some("Fixture output brightness (0-255).".into()),
                unit: None,
            },
            editor: ValueEditorHint::Slider {
                min: OrderedF32(0.0),
                max: OrderedF32(255.0),
                step: Some(OrderedF32(1.0)),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brightness_round_trips_as_plain_u32() {
        let decoded: Brightness = serde_json::from_str("200").expect("plain integer decodes");
        assert_eq!(decoded, Brightness(200));
        assert_eq!(
            serde_json::to_string(&decoded).expect("encodes"),
            "200",
            "wire form stays a bare unsigned integer"
        );
        assert_eq!(decoded.to_lp_value(), LpValue::U32(200));
        assert_eq!(
            Brightness::from_lp_value(&LpValue::U32(31)).expect("u32 converts"),
            Brightness(31)
        );
    }

    /// Q13 (binding-is-publicity): brightness carries no panel FLAG — the
    /// fixture face's fader is that face's own affordance, derived from
    /// this slider editor hint. The hint is therefore load-bearing.
    #[test]
    fn brightness_shape_carries_the_slider_editor_hint() {
        let shape = Brightness::value_shape();
        assert!(matches!(
            shape.editor,
            ValueEditorHint::Slider { min, max, .. }
                if min == OrderedF32(0.0) && max == OrderedF32(255.0)
        ));

        let static_shape =
            Brightness::STATIC_VALUE_SHAPE_DESCRIPTOR.expect("static descriptor exists");
        assert_eq!(
            static_shape.to_owned_value_shape(),
            shape,
            "static and owned shapes agree"
        );
    }

    #[test]
    fn clamps_to_u8_for_the_led_path() {
        assert_eq!(Brightness(64).as_u8(), 64);
        assert_eq!(Brightness(4096).as_u8(), 255);
    }
}
