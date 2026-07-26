//! Value family a numeric panel widget writes back (node-card P3).
//!
//! Knobs and faders gesture in `f32`, but the backing slot may be an
//! integer family (the fixture brightness fader edits a `u32` slot). The
//! emit family types the dispatched `LpValue` so `SlotEditOp::SetValue`
//! always matches the slot's shape; integer families round to the nearest
//! whole value.

use lpa_studio_core::{LpValue, UiSlotValueKind};

/// The `LpValue` family a numeric panel gesture dispatches.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PanelEmit {
    /// Dispatch `LpValue::F32` (the default).
    #[default]
    F32,
    /// Dispatch `LpValue::U32`, rounding (drags below zero clamp to 0).
    U32,
    /// Dispatch `LpValue::I32`, rounding.
    I32,
}

impl PanelEmit {
    /// The gesture value `(as f32, emit family)` for a numeric panel
    /// control's slot value; `None` for non-numeric families.
    pub fn for_value(kind: &UiSlotValueKind) -> Option<(f32, Self)> {
        match kind {
            UiSlotValueKind::F32(value) => Some((*value, Self::F32)),
            UiSlotValueKind::U32(value) => Some((*value as f32, Self::U32)),
            UiSlotValueKind::I32(value) => Some((*value as f32, Self::I32)),
            _ => None,
        }
    }

    /// Type a gesture's `f32` into the slot's value family.
    pub fn lp_value(self, value: f32) -> LpValue {
        match self {
            Self::F32 => LpValue::F32(value),
            Self::U32 => LpValue::U32(value.round().max(0.0) as u32),
            Self::I32 => LpValue::I32(value.round() as i32),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numeric_families_map_to_gesture_values() {
        assert_eq!(
            PanelEmit::for_value(&UiSlotValueKind::F32(1.5)),
            Some((1.5, PanelEmit::F32))
        );
        assert_eq!(
            PanelEmit::for_value(&UiSlotValueKind::U32(64)),
            Some((64.0, PanelEmit::U32))
        );
        assert_eq!(
            PanelEmit::for_value(&UiSlotValueKind::I32(-3)),
            Some((-3.0, PanelEmit::I32))
        );
        assert_eq!(PanelEmit::for_value(&UiSlotValueKind::Bool(true)), None);
    }

    #[test]
    fn integer_families_round_and_stay_in_domain() {
        assert_eq!(PanelEmit::F32.lp_value(1.5), LpValue::F32(1.5));
        assert_eq!(PanelEmit::U32.lp_value(200.4), LpValue::U32(200));
        assert_eq!(PanelEmit::U32.lp_value(-3.0), LpValue::U32(0));
        assert_eq!(PanelEmit::I32.lp_value(-2.6), LpValue::I32(-3));
    }
}
