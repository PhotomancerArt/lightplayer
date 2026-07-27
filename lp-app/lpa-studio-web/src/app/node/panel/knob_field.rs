//! Rotary knob field for bounded numeric panel controls (knob v2).
//!
//! SVG anatomy from the node-card spike: a 270° track arc with the filled
//! value arc riding it, min/mid/max (plus quarter) tick marks, a restrained
//! radial-gradient body, and a pointer line. The value arc and pointer wear
//! the accent color, or the violet bound family when the backing slot is
//! bound (never green — green is valid-only).
//!
//! Interaction: vertical drag (up = increase) dispatching
//! `SlotEditOp::SetValue` with `oninput` semantics — a continuous flood the
//! actor coalesces per address, exactly like slider drags. The knob is also
//! keyboard-operable (P6 item 3): arrows step (the authored hint step, else
//! 1% of the range), Shift multiplies by 10, Home/End jump to min/max — all
//! dispatching the same `slot_set_value` path.
//!
//! A bound knob with a live bus reading renders the arc and pointer AT the
//! live value (already violet); drags and keys still edit the authored
//! default (P6 item 1).

use dioxus::prelude::*;
use lpa_studio_core::{ProjectSlotAddress, UiAction, UiSlotFieldState};

use crate::app::node::slot_edit_actions::slot_set_value_action;
use crate::app::node::slot_fields::{capture_field_pointer, field_wiring};

use super::PanelEmit;

/// Vertical drag distance (CSS px) that sweeps the knob across its whole
/// range — small enough for one comfortable wrist motion, large enough for
/// fine control.
const KNOB_DRAG_RANGE_PX: f64 = 160.0;

/// The knob's sweep in degrees (gap at the bottom), from -135° at `min` to
/// +135° at `max`.
const KNOB_SWEEP_DEG: f32 = 270.0;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn KnobField(
    value: f32,
    /// Live bus reading (display-only): the arc and pointer render AT this
    /// value while `value` (the authored default) stays the edit target.
    #[props(default = None)]
    live_value: Option<f32>,
    min: f32,
    max: f32,
    #[props(default = None)] step: Option<f32>,
    state: UiSlotFieldState,
    /// Violet bound treatment on the arc, pointer, and body ring.
    #[props(default = false)]
    bound: bool,
    #[props(default = None)] address: Option<ProjectSlotAddress>,
    /// Value family the drag dispatches (`F32` default; integer slots
    /// round).
    #[props(default)]
    emit: PanelEmit,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let wired = field_wiring(&state, &address, on_action);
    let editable = wired.is_some();
    let frac = knob_fraction(live_value.unwrap_or(value), min, max);
    let arc_len = frac * 100.0;
    let pointer_deg = knob_pointer_deg(frac);
    let stroke = knob_value_stroke(&state, bound, editable);
    let body_stroke = if bound {
        "var(--studio-status-bound-border)"
    } else {
        "var(--studio-color-border-strong)"
    };
    let invalid_title = state.invalid.clone().unwrap_or_default();

    let down_wiring = wired.clone();
    let move_wiring = wired.clone();
    let key_wiring = wired;
    // Drag anchor: pointer y and value at pointerdown; None while idle.
    let mut drag = use_signal(|| None::<(f64, f32)>);

    rsx! {
        span {
            class: if editable { "tw:inline-flex tw:flex-none tw:cursor-ns-resize tw:touch-none tw:rounded-full tw:outline-none tw:focus-visible:outline tw:focus-visible:outline-1 tw:focus-visible:outline-border-strong" } else { "tw:inline-flex tw:flex-none" },
            role: "slider",
            tabindex: if editable { "0" } else { "-1" },
            aria_valuemin: "{min}",
            aria_valuemax: "{max}",
            aria_valuenow: "{value}",
            title: "{invalid_title}",
            onkeydown: move |event| {
                let Some((address, handler)) = key_wiring.clone() else {
                    return;
                };
                let multiplier = if event.modifiers().shift() { 10.0 } else { 1.0 };
                let Some(next) = knob_key_value(value, &event.key(), multiplier, min, max, step)
                else {
                    return;
                };
                event.prevent_default();
                handler.call(slot_set_value_action(address, emit.lp_value(next)));
            },
            onpointerdown: move |event| {
                if down_wiring.is_none() {
                    return;
                }
                capture_field_pointer(&event);
                drag.set(Some((event.data().client_coordinates().y, value)));
            },
            onpointermove: move |event| {
                let Some((anchor_y, anchor_value)) = drag() else {
                    return;
                };
                if event.data().held_buttons().is_empty() {
                    // Missed release (no pointer capture): stop the drag.
                    drag.set(None);
                    return;
                }
                let Some((address, handler)) = move_wiring.clone() else {
                    return;
                };
                let next = knob_drag_value(
                    anchor_value,
                    anchor_y - event.data().client_coordinates().y,
                    min,
                    max,
                    step,
                );
                handler.call(slot_set_value_action(address, emit.lp_value(next)));
            },
            onpointerup: move |_| drag.set(None),
            onpointercancel: move |_| drag.set(None),
            svg {
                class: "tw:block",
                width: "46",
                height: "46",
                view_box: "0 0 48 48",
                defs {
                // Shared per-knob gradient: every knob carries an identical
                // def under the same id, so `url(#…)` always resolves to an
                // identical gradient regardless of render order.
                radialGradient {
                    id: "lp-knob-body-gradient",
                    cx: "35%",
                    cy: "30%",
                    r: "80%",
                    stop {
                        offset: "0%",
                        stop_color: "var(--studio-color-surface-raised-strong)",
                    }
                    stop {
                        offset: "100%",
                        stop_color: "var(--studio-color-surface-raised)",
                    }
                }
            }
            // Track: the full 270° sweep.
            path {
                d: "{KNOB_ARC_PATH}",
                path_length: "100",
                fill: "none",
                stroke: "var(--studio-color-border-strong)",
                stroke_width: "2.5",
                stroke_linecap: "round",
            }
            // Value arc: the filled portion of the sweep.
            path {
                d: "{KNOB_ARC_PATH}",
                path_length: "100",
                fill: "none",
                stroke: "{stroke}",
                stroke_width: "2.5",
                stroke_linecap: "round",
                stroke_dasharray: "{arc_len} 100",
            }
            // Min / quarter / mid / quarter / max tick marks.
            g {
                stroke: "var(--studio-color-text-subtle)",
                stroke_width: "1",
                opacity: "0.5",
                line { x1: "11.63", y1: "36.37", x2: "9.15", y2: "38.85" }
                line { x1: "7.83", y1: "17.3", x2: "4.6", y2: "16.0" }
                line { x1: "24", y1: "6.5", x2: "24", y2: "3" }
                line { x1: "40.2", y1: "17.3", x2: "43.4", y2: "16.0" }
                line { x1: "36.37", y1: "36.37", x2: "38.85", y2: "38.85" }
            }
            // Body: restrained radial-gradient face.
            circle {
                cx: "24",
                cy: "24",
                r: "12.5",
                fill: "url(#lp-knob-body-gradient)",
                stroke: "{body_stroke}",
            }
            // Pointer.
            line {
                x1: "24",
                y1: "19",
                x2: "24",
                y2: "12.5",
                stroke: "{stroke}",
                stroke_width: "2",
                stroke_linecap: "round",
                transform: "rotate({pointer_deg} 24 24)",
            }
            }
        }
    }
}

/// The 270° knob arc (radius 19, centered at 24,24), from the -135° bottom
///-left start to the +135° bottom-right end. Shared by the track and value
/// arcs; `pathLength=100` makes the dasharray a percentage.
const KNOB_ARC_PATH: &str = "M10.56 37.44 A19 19 0 1 1 37.44 37.44";

/// The value's position in the knob's range, clamped to 0..=1. A degenerate
/// range (max <= min) pins to 0 so the arc and pointer stay deterministic.
pub(crate) fn knob_fraction(value: f32, min: f32, max: f32) -> f32 {
    if max <= min {
        return 0.0;
    }
    ((value - min) / (max - min)).clamp(0.0, 1.0)
}

/// Pointer rotation for a range fraction: -135° at min through +135° at max.
pub(crate) fn knob_pointer_deg(frac: f32) -> f32 {
    -(KNOB_SWEEP_DEG / 2.0) + frac * KNOB_SWEEP_DEG
}

/// Value for a vertical drag: `rise` CSS px above the anchor (up = increase)
/// maps [`KNOB_DRAG_RANGE_PX`] onto the whole range, snaps to `step` when
/// present, and clamps to the domain.
pub(crate) fn knob_drag_value(
    anchor_value: f32,
    rise: f64,
    min: f32,
    max: f32,
    step: Option<f32>,
) -> f32 {
    let span = (max - min).max(f32::EPSILON);
    let raw = anchor_value + (rise / KNOB_DRAG_RANGE_PX) as f32 * span;
    let snapped = match step {
        Some(step) if step > 0.0 => (raw / step).round() * step,
        _ => raw,
    };
    snapped.clamp(min, max)
}

/// Keyboard step for one arrow press: the authored hint step when present,
/// else 1% of the range (P6 item 3).
pub(crate) fn knob_key_step(min: f32, max: f32, step: Option<f32>) -> f32 {
    match step {
        Some(step) if step > 0.0 => step,
        _ => (max - min).max(f32::EPSILON) / 100.0,
    }
}

/// Value for one keyboard gesture on the knob: arrows step by
/// [`knob_key_step`] × `multiplier` (Shift = 10), Home/End jump to the
/// domain edges; anything else is `None` (not a knob key). Stepped results
/// snap to the authored step and clamp, exactly like drags.
pub(crate) fn knob_key_value(
    value: f32,
    key: &Key,
    multiplier: f32,
    min: f32,
    max: f32,
    step: Option<f32>,
) -> Option<f32> {
    let delta = knob_key_step(min, max, step) * multiplier;
    let raw = match key {
        Key::ArrowUp | Key::ArrowRight => value + delta,
        Key::ArrowDown | Key::ArrowLeft => value - delta,
        Key::Home => return Some(min),
        Key::End => return Some(max),
        _ => return None,
    };
    let snapped = match step {
        Some(step) if step > 0.0 => (raw / step).round() * step,
        _ => raw,
    };
    Some(snapped.clamp(min, max))
}

/// Stroke for the value arc and pointer: violet when bound, error when
/// invalid, subtle when read-only, accent otherwise (green stays valid-only).
fn knob_value_stroke(state: &UiSlotFieldState, bound: bool, editable: bool) -> &'static str {
    if bound {
        "var(--studio-status-bound-text)"
    } else if state.invalid.is_some() {
        "var(--studio-status-error-text)"
    } else if editable {
        "var(--studio-color-accent)"
    } else {
        "var(--studio-color-text-subtle)"
    }
}

#[cfg(test)]
mod tests {
    use dioxus::prelude::Key;
    use lpa_studio_core::UiSlotFieldState;

    use super::{
        knob_drag_value, knob_fraction, knob_key_step, knob_key_value, knob_pointer_deg,
        knob_value_stroke,
    };

    #[test]
    fn fraction_clamps_and_survives_degenerate_ranges() {
        assert_eq!(knob_fraction(1.0, 0.0, 4.0), 0.25);
        assert_eq!(knob_fraction(-3.0, 0.0, 4.0), 0.0);
        assert_eq!(knob_fraction(9.0, 0.0, 4.0), 1.0);
        assert_eq!(knob_fraction(1.0, 2.0, 2.0), 0.0);
    }

    #[test]
    fn pointer_sweeps_from_minus_135_to_plus_135() {
        assert_eq!(knob_pointer_deg(0.0), -135.0);
        assert_eq!(knob_pointer_deg(0.5), 0.0);
        assert_eq!(knob_pointer_deg(1.0), 135.0);
    }

    #[test]
    fn drag_maps_rise_onto_the_range_and_clamps() {
        // A full-range rise sweeps min → max.
        assert_eq!(knob_drag_value(0.0, 160.0, 0.0, 4.0, None), 4.0);
        // Half the range, downward drag decreases.
        assert_eq!(knob_drag_value(2.0, -80.0, 0.0, 4.0, None), 0.0);
        // Overshoot pins to the domain edge.
        assert_eq!(knob_drag_value(3.5, 400.0, 0.0, 4.0, None), 4.0);
    }

    #[test]
    fn drag_snaps_to_step_when_present() {
        let value = knob_drag_value(0.0, 43.0, 0.0, 4.0, Some(0.5));
        assert_eq!(value, 1.0);
    }

    #[test]
    fn keyboard_steps_use_the_hint_step_or_one_percent_of_the_range() {
        assert_eq!(knob_key_step(0.0, 4.0, Some(0.5)), 0.5);
        assert_eq!(knob_key_step(0.0, 4.0, None), 0.04);
        // A non-positive authored step falls back to the range fraction.
        assert_eq!(knob_key_step(0.0, 4.0, Some(0.0)), 0.04);
    }

    #[test]
    fn arrow_keys_step_shift_multiplies_and_home_end_jump() {
        // 1% of the 0..100 range = an exact 1.0 step.
        assert_eq!(
            knob_key_value(50.0, &Key::ArrowUp, 1.0, 0.0, 100.0, None),
            Some(51.0)
        );
        assert_eq!(
            knob_key_value(50.0, &Key::ArrowLeft, 1.0, 0.0, 100.0, None),
            Some(49.0)
        );
        // Shift multiplies the step by 10.
        assert_eq!(
            knob_key_value(50.0, &Key::ArrowUp, 10.0, 0.0, 100.0, None),
            Some(60.0)
        );
        // Home/End jump to the domain edges regardless of step.
        assert_eq!(
            knob_key_value(2.0, &Key::Home, 1.0, 0.0, 4.0, Some(0.5)),
            Some(0.0)
        );
        assert_eq!(
            knob_key_value(2.0, &Key::End, 1.0, 0.0, 4.0, Some(0.5)),
            Some(4.0)
        );
        // Stepped knobs snap like drags and clamp at the edges.
        assert_eq!(
            knob_key_value(3.9, &Key::ArrowUp, 10.0, 0.0, 4.0, Some(0.5)),
            Some(4.0)
        );
        // Non-knob keys pass through untouched (no dispatch).
        assert_eq!(knob_key_value(2.0, &Key::Tab, 1.0, 0.0, 4.0, None), None);
    }

    #[test]
    fn bound_wins_the_stroke_even_over_invalid() {
        let invalid = UiSlotFieldState::editable().with_invalid("out of range");
        assert_eq!(
            knob_value_stroke(&invalid, true, true),
            "var(--studio-status-bound-text)"
        );
        assert_eq!(
            knob_value_stroke(&invalid, false, true),
            "var(--studio-status-error-text)"
        );
        assert_eq!(
            knob_value_stroke(&UiSlotFieldState::editable(), false, true),
            "var(--studio-color-accent)"
        );
        assert_eq!(
            knob_value_stroke(&UiSlotFieldState::readonly(), false, false),
            "var(--studio-color-text-subtle)"
        );
    }
}
