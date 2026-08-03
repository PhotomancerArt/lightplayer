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
//!
//! A knob carrying a `step` is quantized end to end ([`knob_snap`]): the arc,
//! pointer, and `aria-valuenow` all land on the grid, and so does every value
//! a gesture writes. Integer uniforms (`i32`/`u32`) get `step: 1`, which is
//! what keeps a "how many" knob on 1/2/3/4 instead of 2.37.
//!
//! Stepped knobs also wear their own visual language ([`knob_arc_chunks`]):
//! the sweep is cut into one squared-off block per increment, lighting whole
//! rather than growing smoothly, and the fixed tick marks drop away — the
//! gaps between blocks ARE the scale. Overlaid ticks were tried first and
//! disappeared against the accent arc. A step too fine to block legibly
//! falls back to the continuous rendering.

use dioxus::prelude::*;
use lpa_studio_core::{ProjectSlotAddress, UiAction, UiPanelTarget, UiSlotFieldState};

use crate::app::node::slot_edit_actions::panel_or_slot_action;
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
    /// Amber ENGAGED treatment (`docs/design/panel.md` P2/P6): a panel
    /// writer has captured this channel and holds it. Deliberately NOT the
    /// violet bound family — bound means "wired", engaged means "captured"
    /// — and it outranks violet, because a captured control has stopped
    /// following whatever it is wired to.
    #[props(default = false)]
    engaged: bool,
    #[props(default = None)] address: Option<ProjectSlotAddress>,
    /// Panel-write target: when present, gestures dispatch `PanelWriteOp`
    /// at this `(scope, channel)` (the runtime command channel) instead of
    /// editing the authored default at `address`.
    #[props(default = None)]
    panel_target: Option<UiPanelTarget>,
    /// Value family the drag dispatches (`F32` default; integer slots
    /// round).
    #[props(default)]
    emit: PanelEmit,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let wired = field_wiring(&state, &address, on_action);
    let editable = wired.is_some();
    // A stepped knob renders ON its grid: an integer knob points at 2, never
    // between 2 and 3, whichever off-grid value (a stale authored default, a
    // continuous bus reading) is behind it. Gestures still start from the raw
    // `value` so repeated arrow presses never stall on a rounding boundary.
    let shown = knob_snap(live_value.unwrap_or(value), min, step);
    let frac = knob_fraction(shown, min, max);
    let arc_len = frac * 100.0;
    let pointer_deg = knob_pointer_deg(frac);
    let stroke = knob_value_stroke(&state, bound, engaged, editable);
    let body_stroke = if engaged {
        "var(--studio-status-attention-border)"
    } else if bound {
        "var(--studio-status-bound-border)"
    } else {
        "var(--studio-color-border-strong)"
    };
    let invalid_title = state.invalid.clone().unwrap_or_default();

    let down_wiring = wired.clone();
    let move_wiring = wired.clone();
    let key_wiring = wired;
    let move_target = panel_target.clone();
    let key_target = panel_target;
    // Drag anchor: pointer y and value at pointerdown; None while idle.
    let mut drag = use_signal(|| None::<(f64, f32)>);

    rsx! {
        span {
            class: if editable { "tw:inline-flex tw:flex-none tw:cursor-ns-resize tw:touch-none tw:rounded-full tw:outline-none tw:focus-visible:outline tw:focus-visible:outline-1 tw:focus-visible:outline-border-strong" } else { "tw:inline-flex tw:flex-none" },
            role: "slider",
            tabindex: if editable { "0" } else { "-1" },
            aria_valuemin: "{min}",
            aria_valuemax: "{max}",
            aria_valuenow: "{knob_snap(value, min, step)}",
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
                handler.call(panel_or_slot_action(&key_target, address, emit.lp_value(next)));
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
                handler.call(panel_or_slot_action(&move_target, address, emit.lp_value(next)));
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
            // A stepped knob's sweep is CHUNKED — one squared-off block per
            // increment, lit up to the current value — so a discrete control
            // never looks like a continuous one at a glance. A continuous
            // knob keeps the smooth rounded arc plus its fixed tick marks.
            if let Some(chunks) = knob_arc_chunks(min, max, step) {
                for (index, chunk) in chunks.into_iter().enumerate() {
                    path {
                        key: "{index}",
                        d: "{KNOB_ARC_PATH}",
                        path_length: "100",
                        fill: "none",
                        stroke: if chunk.filled(frac) { "{stroke}" } else { "var(--studio-color-border-strong)" },
                        stroke_width: "2.5",
                        // Butt caps keep the gaps crisp: round caps would
                        // bleed the blocks back into one another.
                        stroke_linecap: "butt",
                        stroke_dasharray: "{chunk.length:.3} 100",
                        stroke_dashoffset: "{-chunk.start:.3}",
                    }
                }
            } else {
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
                    for (index, tick) in CONTINUOUS_TICKS.into_iter().enumerate() {
                        line {
                            key: "{index}",
                            x1: "{knob_tick_x(tick, TICK_INNER_RADIUS):.2}",
                            y1: "{knob_tick_y(tick, TICK_INNER_RADIUS):.2}",
                            x2: "{knob_tick_x(tick, TICK_OUTER_RADIUS):.2}",
                            y2: "{knob_tick_y(tick, TICK_OUTER_RADIUS):.2}",
                        }
                    }
                }
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

/// Quantize a value onto the knob's step grid, or pass it through when the
/// knob is continuous. Integer knobs carry `step: 1`, which is what makes
/// "how many meteors" land on 1/2/3/4 instead of 2.37.
///
/// The grid is anchored at `min`, matching how a native `<input type=range>`
/// steps — so the fader and the knob agree, and every grid position is one
/// the control can actually rest on (see [`knob_arc_chunks`]).
pub(crate) fn knob_snap(value: f32, min: f32, step: Option<f32>) -> f32 {
    match step {
        Some(step) if step > 0.0 => min + ((value - min) / step).round() * step,
        _ => value,
    }
}

/// Radii (viewBox units, center 24,24) the tick marks span.
const TICK_INNER_RADIUS: f32 = 17.5;
const TICK_OUTER_RADIUS: f32 = 21.0;

/// Sweep fractions of the fixed marks a continuous knob wears: min, the
/// quarters, and max.
const CONTINUOUS_TICKS: [f32; 5] = [0.0, 0.25, 0.5, 0.75, 1.0];

/// Most chunks worth drawing. Past this the blocks and their gaps fall
/// below a pixel on a 46px knob and read as a dashed smear, so a very fine
/// step renders as a plain continuous knob — the pointer carries the
/// position and the readout carries the precision.
const MAX_ARC_CHUNKS: usize = 24;

/// Fraction tolerance for "this boundary is already at the end of the
/// sweep" and for "the value has reached this chunk's end".
const CHUNK_EPSILON: f32 = 1e-4;

/// Gap between two chunks, in `pathLength=100` units, as a share of the
/// chunk itself — clamped so three fat chunks are not split by a canyon and
/// twenty thin ones still separate.
const CHUNK_GAP_SHARE: f32 = 0.22;
const CHUNK_GAP_MIN: f32 = 0.9;
const CHUNK_GAP_MAX: f32 = 2.6;

/// One block of a stepped knob's sweep, in `pathLength=100` units along the
/// arc.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct KnobArcChunk {
    /// Where the drawn block starts (the gap is already taken off the front).
    pub start: f32,
    /// How long the drawn block is (the gap is already taken off the back).
    pub length: f32,
    /// Sweep fraction the block ends at — the value must reach this for the
    /// block to light up.
    pub end_frac: f32,
}

impl KnobArcChunk {
    /// Whether the knob's current sweep fraction has filled this block.
    /// Blocks light whole: reaching a resting position lights the block
    /// leading up to it, and nothing partial is ever drawn.
    pub fn filled(&self, frac: f32) -> bool {
        frac >= self.end_frac - CHUNK_EPSILON
    }
}

/// The blocks a stepped knob's arc is cut into — one per increment — or
/// `None` when the knob should draw the smooth continuous arc.
///
/// Boundaries are the positions the knob can rest on (`min + k*step`, plus
/// `max` itself when the range is not a whole number of steps, since the
/// gesture clamp makes the top reachable off-grid). A partial last step
/// therefore renders as a visibly shorter block, which is honest: it IS a
/// shorter reach.
pub(crate) fn knob_arc_chunks(min: f32, max: f32, step: Option<f32>) -> Option<Vec<KnobArcChunk>> {
    let bounds = knob_step_bounds(min, max, step)?;
    let chunks: Vec<KnobArcChunk> = bounds
        .windows(2)
        .map(|pair| {
            let (start, end) = (pair[0] * 100.0, pair[1] * 100.0);
            let gap = ((end - start) * CHUNK_GAP_SHARE).clamp(CHUNK_GAP_MIN, CHUNK_GAP_MAX);
            KnobArcChunk {
                start: start + gap / 2.0,
                // A gap wider than the block itself would invert the dash;
                // never let a block vanish entirely.
                length: (end - start - gap).max(0.5),
                end_frac: pair[1],
            }
        })
        .collect();
    (!chunks.is_empty()).then_some(chunks)
}

/// Sweep fractions (0..=1) of every position a stepped knob can rest on, or
/// `None` when the knob is continuous or its step is too fine to draw.
fn knob_step_bounds(min: f32, max: f32, step: Option<f32>) -> Option<Vec<f32>> {
    let span = max - min;
    let step = step.filter(|step| *step > 0.0)?;
    if !span.is_finite() || span <= 0.0 {
        return None;
    }
    // Guard the cast: a tiny step over a wide range divides to something no
    // usize should hold, let alone draw.
    let divisions = span / step;
    if !divisions.is_finite() || divisions > MAX_ARC_CHUNKS as f32 {
        return None;
    }
    let mut bounds: Vec<f32> = (0..=divisions.floor() as usize)
        .map(|index| (index as f32 * step) / span)
        .collect();
    if bounds
        .last()
        .is_some_and(|last| *last < 1.0 - CHUNK_EPSILON)
    {
        bounds.push(1.0);
    }
    // One boundary is no scale, and more chunks than the cap is a smear.
    (bounds.len() > 1 && bounds.len() <= MAX_ARC_CHUNKS + 1).then_some(bounds)
}

/// A tick endpoint's x, `radius` out from the knob center along the sweep
/// fraction's bearing (0 = the -135° start, 1 = the +135° end).
pub(crate) fn knob_tick_x(frac: f32, radius: f32) -> f32 {
    24.0 + radius * knob_pointer_deg(frac).to_radians().sin()
}

/// A tick endpoint's y (SVG y grows downward, so the cosine subtracts).
pub(crate) fn knob_tick_y(frac: f32, radius: f32) -> f32 {
    24.0 - radius * knob_pointer_deg(frac).to_radians().cos()
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
    knob_snap(raw, min, step).clamp(min, max)
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
    Some(knob_snap(raw, min, step).clamp(min, max))
}

/// Stroke for the value arc and pointer: amber when a panel writer has it
/// engaged, violet when bound, error when invalid, subtle when read-only,
/// accent otherwise (green stays valid-only).
///
/// Engaged outranks bound: a captured control is no longer following the
/// thing it is wired to, and the panel's whole point is that you can see
/// that at a glance (panel.md P-Q2).
fn knob_value_stroke(
    state: &UiSlotFieldState,
    bound: bool,
    engaged: bool,
    editable: bool,
) -> &'static str {
    if engaged {
        "var(--studio-status-attention-text)"
    } else if bound {
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
        CONTINUOUS_TICKS, TICK_INNER_RADIUS, TICK_OUTER_RADIUS, knob_arc_chunks, knob_drag_value,
        knob_fraction, knob_key_step, knob_key_value, knob_pointer_deg, knob_snap, knob_tick_x,
        knob_tick_y, knob_value_stroke,
    };

    #[test]
    fn integer_knobs_render_and_write_whole_numbers() {
        // A `count` uniform over 1..=4: every gesture and the rendered
        // position land on 1/2/3/4, never 2.37.
        assert_eq!(knob_snap(2.37, 1.0, Some(1.0)), 2.0);
        assert_eq!(knob_snap(2.6, 1.0, Some(1.0)), 3.0);
        // A tiny drag off 2 stays at 2 instead of drifting fractional.
        assert_eq!(knob_drag_value(2.0, 8.0, 1.0, 4.0, Some(1.0)), 2.0);
        // A drag big enough to cross the half-step lands on the next whole.
        assert_eq!(knob_drag_value(2.0, 30.0, 1.0, 4.0, Some(1.0)), 3.0);
        // Arrows walk whole steps and clamp at the domain edge.
        assert_eq!(
            knob_key_value(2.0, &Key::ArrowUp, 1.0, 1.0, 4.0, Some(1.0)),
            Some(3.0)
        );
        assert_eq!(
            knob_key_value(4.0, &Key::ArrowUp, 1.0, 1.0, 4.0, Some(1.0)),
            Some(4.0)
        );
        // A continuous knob is untouched.
        assert_eq!(knob_snap(2.37, 1.0, None), 2.37);
        assert_eq!(knob_snap(2.37, 1.0, Some(0.0)), 2.37);
    }

    #[test]
    fn the_grid_is_anchored_at_min_like_a_native_range_input() {
        // An off-grid min still yields reachable positions: 0.5, 1.5, 2.5…
        // rather than integers the control could only reach by clamping.
        assert_eq!(knob_snap(1.4, 0.5, Some(1.0)), 1.5);
        assert_eq!(knob_snap(0.6, 0.5, Some(1.0)), 0.5);
        // With an on-grid min (every real integer uniform) it is unchanged.
        assert_eq!(knob_snap(2.37, 0.0, Some(1.0)), 2.0);
    }

    #[test]
    fn stepped_knobs_cut_the_arc_into_one_block_per_increment() {
        // count over 1..=4: three increments, so three blocks.
        let chunks = knob_arc_chunks(1.0, 4.0, Some(1.0)).expect("a stepped knob chunks");
        assert_eq!(chunks.len(), 3);
        // Blocks tile the sweep in order, each ending on a resting position.
        let ends: Vec<f32> = chunks.iter().map(|chunk| chunk.end_frac).collect();
        assert_eq!(ends, vec![1.0 / 3.0, 2.0 / 3.0, 1.0]);
        // Every block is separated from the next: the drawn span of one ends
        // before the next one starts.
        for pair in chunks.windows(2) {
            assert!(
                pair[0].start + pair[0].length < pair[1].start,
                "blocks must not touch: {pair:?}"
            );
        }
        // The whole run stays inside the sweep.
        assert!(chunks[0].start > 0.0);
        let last = chunks[chunks.len() - 1];
        assert!(last.start + last.length < 100.0);
    }

    #[test]
    fn blocks_light_whole_up_to_the_current_value() {
        let chunks = knob_arc_chunks(1.0, 4.0, Some(1.0)).expect("a stepped knob chunks");
        let lit = |frac: f32| chunks.iter().filter(|chunk| chunk.filled(frac)).count();
        // count = 1 (the minimum): nothing filled.
        assert_eq!(lit(0.0), 0);
        // count = 2, 3, 4: one more block each time, never a partial one.
        assert_eq!(lit(1.0 / 3.0), 1);
        assert_eq!(lit(2.0 / 3.0), 2);
        assert_eq!(lit(1.0), 3);
    }

    #[test]
    fn knobs_that_should_not_chunk_fall_back_to_the_smooth_arc() {
        // A continuous knob.
        assert_eq!(knob_arc_chunks(0.0, 4.0, None), None);
        // 0..=255 by 1 is far too fine to draw as blocks.
        assert_eq!(knob_arc_chunks(0.0, 255.0, Some(1.0)), None);
        // Degenerate or nonsense domains.
        assert_eq!(knob_arc_chunks(2.0, 2.0, Some(1.0)), None);
        assert_eq!(knob_arc_chunks(0.0, 4.0, Some(0.0)), None);
        // A vanishing step would divide to an uncastable count.
        assert_eq!(knob_arc_chunks(0.0, 4.0, Some(f32::MIN_POSITIVE)), None);
    }

    #[test]
    fn a_step_wider_than_the_range_is_a_single_two_position_block() {
        // min and max are the only reachable values, so the sweep is one
        // block: dark at the bottom, lit at the top.
        let chunks = knob_arc_chunks(0.0, 4.0, Some(9.0)).expect("one block");
        assert_eq!(chunks.len(), 1);
        assert!(!chunks[0].filled(0.0));
        assert!(chunks[0].filled(1.0));
    }

    #[test]
    fn a_partial_last_step_gets_its_own_shorter_block() {
        // 0..=5 by 2 rests on 0, 2, 4 — and on 5, because the gesture clamp
        // pins an overshoot to max. The final reach is half as long.
        let chunks = knob_arc_chunks(0.0, 5.0, Some(2.0)).expect("chunks");
        assert_eq!(chunks.len(), 3);
        let ends: Vec<f32> = chunks.iter().map(|chunk| chunk.end_frac).collect();
        assert_eq!(ends, vec![0.4, 0.8, 1.0]);
        assert!(
            chunks[2].length < chunks[1].length,
            "the partial step reads shorter: {chunks:?}"
        );
    }

    #[test]
    fn the_densest_drawable_knob_still_separates_its_blocks() {
        // 24 increments is the cap; the gap floor must still split them.
        let chunks = knob_arc_chunks(0.0, 24.0, Some(1.0)).expect("chunks at the cap");
        assert_eq!(chunks.len(), 24);
        for pair in chunks.windows(2) {
            assert!(pair[0].start + pair[0].length < pair[1].start);
            assert!(pair[0].length > 0.0);
        }
        // One past the cap falls back.
        assert_eq!(knob_arc_chunks(0.0, 25.0, Some(1.0)), None);
    }

    #[test]
    fn tick_geometry_reproduces_the_hand_placed_marks() {
        // The fixed marks the knob wore before ticks were generated, to 0.01.
        let at = |frac: f32| {
            (
                (knob_tick_x(frac, TICK_INNER_RADIUS) * 100.0).round() / 100.0,
                (knob_tick_y(frac, TICK_INNER_RADIUS) * 100.0).round() / 100.0,
                (knob_tick_x(frac, TICK_OUTER_RADIUS) * 100.0).round() / 100.0,
                (knob_tick_y(frac, TICK_OUTER_RADIUS) * 100.0).round() / 100.0,
            )
        };
        assert_eq!(at(0.0), (11.63, 36.37, 9.15, 38.85));
        assert_eq!(at(0.5), (24.0, 6.5, 24.0, 3.0));
        assert_eq!(at(1.0), (36.37, 36.37, 38.85, 38.85));
    }

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
            knob_value_stroke(&invalid, true, false, true),
            "var(--studio-status-bound-text)"
        );
        assert_eq!(
            knob_value_stroke(&invalid, false, false, true),
            "var(--studio-status-error-text)"
        );
        assert_eq!(
            knob_value_stroke(&UiSlotFieldState::editable(), false, false, true),
            "var(--studio-color-accent)"
        );
        assert_eq!(
            knob_value_stroke(&UiSlotFieldState::readonly(), false, false, false),
            "var(--studio-color-text-subtle)"
        );
    }

    #[test]
    fn engaged_outranks_bound_and_never_reuses_violet() {
        // A captured control has stopped following its binding, so the
        // amber engaged family wins over the violet bound family — the
        // three panel states must stay visibly distinct (panel.md P-Q2).
        let stroke = knob_value_stroke(&UiSlotFieldState::editable(), true, true, true);
        assert_eq!(stroke, "var(--studio-status-attention-text)");
        assert!(!stroke.contains("bound"));
    }
}
