//! Value family a numeric panel widget writes back (node-card P3).
//!
//! Knobs and faders gesture in `f32`, but what the backing slot (or the
//! channel behind it) accepts may not be an `f32` at all. The emit family
//! types the dispatched `LpValue` so `SlotEditOp::SetValue` and
//! `PanelWriteOp` always carry the shape the other end expects:
//!
//! - integer families round to the nearest whole value (the fixture
//!   brightness fader edits a `u32` slot);
//! - a **phasor period** knob re-wraps its number into a whole
//!   `PhasorConfig`, because the period is one field of a record — see
//!   [`lpa_studio_core::UiPanelEmit`], which is where the shaping the wrap
//!   preserves comes from.

use lpa_studio_core::{
    LpValue, PhasorConfig, ToLpValue, UiPanelControl, UiPanelEmit, UiSlotValueKind, Waveform,
};

/// The `LpValue` family a numeric panel gesture dispatches.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum PanelEmit {
    /// Dispatch `LpValue::F32` (the default).
    #[default]
    F32,
    /// Dispatch `LpValue::U32`, rounding (drags below zero clamp to 0).
    U32,
    /// Dispatch `LpValue::I32`, rounding.
    I32,
    /// Dispatch a whole `PhasorConfig` whose `period_seconds` is the
    /// gesture value and whose shaping is this slot's own, untouched.
    ///
    /// Both dispatch paths need it: the slot-edit path writes the record at
    /// `consumed[<name>].phasor.some`, and the panel-write path puts the
    /// same record on the config channel, where every reader sharing that
    /// channel picks the period out of it (parent D3).
    PhasorPeriod {
        waveform: Waveform,
        phase_offset: f32,
    },
}

impl PanelEmit {
    /// The gesture value `(as f32, emit family)` for a numeric panel
    /// control; `None` for non-numeric families.
    ///
    /// Reads the control's declared [`UiPanelEmit`] first: a period knob's
    /// displayed value is a plain `f32` (the seconds), so the value kind
    /// alone cannot tell it apart from an ordinary knob — only the
    /// projection knows the number has to be re-wrapped on the way out.
    pub fn for_control(control: &UiPanelControl) -> Option<(f32, Self)> {
        let (value, family) = Self::for_value(&control.value.kind)?;
        match control.emit {
            UiPanelEmit::Value => Some((value, family)),
            UiPanelEmit::PhasorPeriod {
                waveform,
                phase_offset,
            } => Some((
                value,
                Self::PhasorPeriod {
                    waveform,
                    phase_offset,
                },
            )),
            // A palette does not gesture in `f32` at all: the chooser hands
            // back a whole `GradientConfig`, so there is no number for the
            // numeric ladder to type. The write goes out through
            // `palette_write_action` instead (M4 P3).
            UiPanelEmit::Gradient => None,
        }
    }

    /// The gesture value `(as f32, emit family)` for a bare slot value.
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
            Self::PhasorPeriod {
                waveform,
                phase_offset,
            } => PhasorConfig {
                period_seconds: value,
                waveform,
                phase_offset,
            }
            .to_lp_value(),
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

    /// A period gesture rides out as a WHOLE config: the number is the
    /// period, and the slot's shaping comes along untouched — a panel never
    /// gets to change a waveform (settled D11 v1), but it must not silently
    /// reset one either.
    #[test]
    fn a_period_gesture_rewraps_into_the_slots_own_shaping() {
        let emit = PanelEmit::PhasorPeriod {
            waveform: Waveform::Triangle,
            phase_offset: 0.25,
        };

        assert_eq!(
            emit.lp_value(8.0),
            PhasorConfig {
                period_seconds: 8.0,
                waveform: Waveform::Triangle,
                phase_offset: 0.25,
            }
            .to_lp_value()
        );
    }
}
