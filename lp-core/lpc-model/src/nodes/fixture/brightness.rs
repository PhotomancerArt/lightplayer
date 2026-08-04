//! Fixture brightness amplitude (0–1) — the fixture card's front-panel fader.
//!
//! An `f32` newtype whose slot value shape carries the presentation the
//! generic `f32` leaf cannot: the `Slider` editor hint supplies the fader's
//! range. Wire form and def JSON are a plain number (`#[serde(transparent)]`,
//! `LpValue::F32`) in `[0, 1]` — the bus `brightness` channel's amplitude
//! convention, so the same value works authored, on the channel, and from a
//! panel writer. The pre-amplitude 0–255 authored form still reads: any
//! number above 1 is normalized by 1/255 (see [`Brightness::from_f32`]).

use serde::{Deserialize, Serialize};

use crate::{
    FromLpValue, LpType, LpValue, OrderedF32, SlotMeta, SlotShapeId, SlotValue, SlotValueShape,
    StaticLpType, StaticSlotMeta, StaticSlotValueShape, StaticValueEditorHint, ToLpValue,
    ValueEditorHint, ValueRootError,
};

/// Fixture brightness amplitude (0–1).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(transparent)]
pub struct Brightness(pub f32);

impl Brightness {
    /// Default brightness when none is authored (a quarter of the photons —
    /// the old 64/255, kept as the round number it always meant).
    pub const DEFAULT: Self = Self(0.25);

    /// Normalize a raw number: `[0, 1]` passes through; anything above 1 is
    /// the legacy 0–255 authored scale and divides by 255. A legacy `1`
    /// (1/255, effectively black) reads as full brightness under this rule —
    /// the only ambiguous point, resolved toward the amplitude convention.
    pub fn from_f32(value: f32) -> Self {
        if value > 1.0 {
            Self((value / 255.0).clamp(0.0, 1.0))
        } else {
            Self(value.clamp(0.0, 1.0))
        }
    }

    /// Clamped 8-bit brightness value for the LED path.
    pub fn as_u8(self) -> u8 {
        (self.0.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
    }
}

impl Default for Brightness {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl ToLpValue for Brightness {
    fn to_lp_value(&self) -> LpValue {
        LpValue::F32(self.0)
    }
}

impl FromLpValue for Brightness {
    fn from_lp_value(value: &LpValue) -> Result<Self, ValueRootError> {
        match value {
            LpValue::F32(value) => Ok(Self::from_f32(*value)),
            // Pre-amplitude wire forms carried the 0–255 integer directly.
            LpValue::U32(value) => Ok(Self::from_f32(*value as f32)),
            LpValue::I32(value) => Ok(Self::from_f32(*value as f32)),
            other => Err(ValueRootError::new(alloc::format!(
                "expected F32, got {other:?}"
            ))),
        }
    }
}

impl SlotValue for Brightness {
    const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name("lp::fixture::Brightness");
    const STATIC_VALUE_SHAPE_DESCRIPTOR: Option<StaticSlotValueShape> =
        Some(StaticSlotValueShape {
            id: Self::SHAPE_ID,
            ty: StaticLpType::F32,
            meta: StaticSlotMeta {
                label: Some("Brightness"),
                description: Some("Fixture output brightness (0-1)."),
                unit: None,
            },
            editor: StaticValueEditorHint::Slider {
                min: OrderedF32(0.0),
                max: OrderedF32(1.0),
                step: None,
            },
        });

    fn value_shape() -> SlotValueShape {
        SlotValueShape {
            id: Self::SHAPE_ID,
            ty: LpType::F32,
            meta: SlotMeta {
                label: Some("Brightness".into()),
                description: Some("Fixture output brightness (0-1).".into()),
                unit: None,
            },
            editor: ValueEditorHint::Slider {
                min: OrderedF32(0.0),
                max: OrderedF32(1.0),
                step: None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brightness_round_trips_as_plain_number() {
        let decoded: Brightness = serde_json::from_str("0.5").expect("plain number decodes");
        assert_eq!(decoded, Brightness(0.5));
        assert_eq!(
            serde_json::to_string(&decoded).expect("encodes"),
            "0.5",
            "wire form stays a bare number"
        );
        assert_eq!(decoded.to_lp_value(), LpValue::F32(0.5));
        assert_eq!(
            Brightness::from_lp_value(&LpValue::F32(0.25)).expect("f32 converts"),
            Brightness(0.25)
        );
    }

    /// The pre-amplitude authored form (`"brightness": 255`) keeps reading:
    /// values above 1 normalize by 1/255, from both the F32 the new slot
    /// type decodes and the U32 the old one carried.
    #[test]
    fn legacy_0_255_values_normalize() {
        assert_eq!(
            Brightness::from_lp_value(&LpValue::F32(255.0)).expect("legacy converts"),
            Brightness(1.0)
        );
        let legacy = Brightness::from_lp_value(&LpValue::U32(64)).expect("legacy u32 converts");
        assert!((legacy.0 - 64.0 / 255.0).abs() < 1e-6);
        assert_eq!(legacy.as_u8(), 64);
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
                if min == OrderedF32(0.0) && max == OrderedF32(1.0)
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
        assert_eq!(Brightness(0.25).as_u8(), 64);
        assert_eq!(Brightness(1.0).as_u8(), 255);
        assert_eq!(Brightness(4096.0).as_u8(), 255);
        assert_eq!(Brightness(0.0).as_u8(), 0);
    }
}
