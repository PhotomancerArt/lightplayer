//! The clock's Speed knob on a panel ([`lpa_studio_core::UiPanelWidget::Speed`]):
//! the mapping [`super::KnobField`] turns with its `speed` switch on.
//!
//! The same range and feel as the tape's speed fader (`tape_transport.rs`):
//! log ¼×–8×, the octave stops magnetic. Unlike the fader, **1× sits at
//! the top centre** of the knob's sweep, where a knob reads "normal": the
//! left half spans the two slow octaves (¼× to 1×) and the right half the
//! three fast ones (1× to 8×), each half log on its own. The marks are the
//! octave stops, so the knob shows where its detents are.

use dioxus::prelude::Key;

use crate::app::node::face::{RATE_DETENTS, adjacent_detent, apply_detents};

/// The knob's ends: the tape's span.
pub(crate) const SPEED_MIN: f32 = 0.25;
pub(crate) const SPEED_MAX: f32 = 8.0;

/// Octaves below 1× (the left half of the sweep) and above it (the right).
const SLOW_OCTAVES: f32 = 2.0;
const FAST_OCTAVES: f32 = 3.0;

/// Vertical drag distance (CSS px) that sweeps the whole knob — the plain
/// knob's, so every knob on a panel answers the hand alike.
const SPEED_DRAG_RANGE_PX: f64 = 160.0;

/// Where a rate sits on the knob's sweep, 0–1, with 1× at 0.5.
pub(crate) fn speed_knob_fraction(rate: f32) -> f32 {
    let octaves = rate.clamp(SPEED_MIN, SPEED_MAX).log2();
    let frac = match octaves < 0.0 {
        true => 0.5 + 0.5 * octaves / SLOW_OCTAVES,
        false => 0.5 + 0.5 * octaves / FAST_OCTAVES,
    };
    frac.clamp(0.0, 1.0)
}

/// The rate at sweep fraction `frac` — [`speed_knob_fraction`]'s inverse.
pub(crate) fn speed_knob_rate(frac: f32) -> f32 {
    let frac = frac.clamp(0.0, 1.0);
    let octaves = match frac < 0.5 {
        true => (frac - 0.5) * 2.0 * SLOW_OCTAVES,
        false => (frac - 0.5) * 2.0 * FAST_OCTAVES,
    };
    2.0_f32.powf(octaves)
}

/// The rate a vertical drag reaches: the anchor's sweep fraction plus
/// `rise` (up = faster) over the drag range, seated on an octave stop when
/// it comes near one (the tape's magnetic detents).
pub(crate) fn speed_knob_drag_value(anchor_rate: f32, rise: f64) -> f32 {
    let frac = speed_knob_fraction(anchor_rate) + (rise / SPEED_DRAG_RANGE_PX) as f32;
    apply_detents(speed_knob_rate(frac))
}

/// The rate one key press reaches: the arrows step to the next octave stop
/// (as the tape's fader does), Home is ¼×, End is 8×. `None` for any other
/// key.
pub(crate) fn speed_knob_key_value(rate: f32, key: &Key) -> Option<f32> {
    match key {
        Key::ArrowUp | Key::ArrowRight => Some(adjacent_detent(rate, true)),
        Key::ArrowDown | Key::ArrowLeft => Some(adjacent_detent(rate, false)),
        Key::Home => Some(SPEED_MIN),
        Key::End => Some(SPEED_MAX),
        _ => None,
    }
}

/// The sweep fractions of the knob's marks: one per octave stop.
pub(crate) fn speed_knob_ticks() -> [f32; 6] {
    RATE_DETENTS.map(speed_knob_fraction)
}

/// The value as the knob's readout says it: `1×`, `2×`, `½×`, `¼×`; between
/// the stops, `1.5×` above 1× and `0.75×` below it.
pub(crate) fn speed_words(rate: f32) -> String {
    let rate = rate.clamp(SPEED_MIN, SPEED_MAX);
    let fraction = [(0.25, "¼"), (0.5, "½"), (0.75, "¾")]
        .into_iter()
        .find(|(stop, _)| (rate - stop).abs() < 0.005)
        .map(|(_, words)| words.to_string());
    let number = fraction.unwrap_or_else(|| {
        let places = if rate < 1.0 { 2 } else { 1 };
        let text = format!("{rate:.places$}");
        text.trim_end_matches('0').trim_end_matches('.').to_string()
    });
    format!("{number}×")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 1× is the top centre; the ends are ¼× and 8×.
    #[test]
    fn one_times_is_the_top_centre() {
        assert_eq!(speed_knob_fraction(1.0), 0.5);
        assert_eq!(speed_knob_fraction(0.25), 0.0);
        assert_eq!(speed_knob_fraction(8.0), 1.0);
        assert_eq!(speed_knob_fraction(0.5), 0.25, "one slow octave");
        assert!((speed_knob_fraction(2.0) - 0.5 - 1.0 / 6.0).abs() < 1e-6);
        assert_eq!(speed_knob_fraction(100.0), 1.0, "clamped");
    }

    /// The mapping round-trips across both halves.
    #[test]
    fn the_mapping_round_trips() {
        for rate in [0.25, 0.3, 0.5, 0.9, 1.0, 1.5, 2.0, 3.3, 4.0, 8.0] {
            let back = speed_knob_rate(speed_knob_fraction(rate));
            assert!((back - rate).abs() < 1e-4, "{rate} → {back}");
        }
    }

    /// A drag seats on an octave stop near one, and moves freely between.
    #[test]
    fn a_drag_seats_on_the_octave_stops() {
        assert_eq!(speed_knob_drag_value(1.0, 2.0), 1.0, "1× holds hardest");
        // A full half-sweep up from 1× is 8×.
        assert_eq!(speed_knob_drag_value(1.0, 80.0), 8.0);
        // A sixth of the sweep up from 1× is one octave: 2×.
        assert_eq!(speed_knob_drag_value(1.0, 160.0 / 6.0), 2.0);
        let between = speed_knob_drag_value(1.0, 13.0);
        assert!(between > 1.0 && between < 2.0, "{between}");
    }

    /// The arrows step stop to stop; Home and End are the ends.
    #[test]
    fn the_keys_step_stop_to_stop() {
        assert_eq!(speed_knob_key_value(1.0, &Key::ArrowUp), Some(2.0));
        assert_eq!(speed_knob_key_value(1.5, &Key::ArrowDown), Some(1.0));
        assert_eq!(speed_knob_key_value(1.0, &Key::Home), Some(0.25));
        assert_eq!(speed_knob_key_value(1.0, &Key::End), Some(8.0));
        assert_eq!(speed_knob_key_value(1.0, &Key::Enter), None);
    }

    /// One mark per octave stop, 1× at the top.
    #[test]
    fn the_marks_are_the_octave_stops() {
        let ticks = speed_knob_ticks();
        assert_eq!(ticks[0], 0.0);
        assert_eq!(ticks[2], 0.5);
        assert_eq!(ticks[5], 1.0);
    }

    #[test]
    fn the_readout_says_the_speed() {
        assert_eq!(speed_words(1.0), "1×");
        assert_eq!(speed_words(2.0), "2×");
        assert_eq!(speed_words(8.0), "8×");
        assert_eq!(speed_words(0.5), "½×");
        assert_eq!(speed_words(0.25), "¼×");
        assert_eq!(speed_words(0.75), "¾×");
        assert_eq!(speed_words(1.5), "1.5×");
        assert_eq!(speed_words(0.6), "0.6×");
        assert_eq!(speed_words(3.04), "3×");
    }
}
