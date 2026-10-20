//! Horizontal fader field v2 — the fixture face's dominant brightness
//! control (P2b item 4; build A settled at the P2c re-gate).
//!
//! Anatomy: one tick row above a thin recessed slot (7px, inset-shadowed)
//! with the value fill riding inside it, and a pronounced squared block
//! grip (20 × 24, vertical gradient + center hairline) — a native range
//! input stretched invisibly over the slot supplies the gesture surface
//! while `.ux-hfader` in `style.css` draws the thumb. Accent fill normally,
//! the violet bound family when the backing slot is bound, amber when a
//! panel writer holds the channel. Dispatches `SlotEditOp::SetValue` with
//! `oninput` semantics (the actor coalesces the drag flood per address).
//!
//! **As a panel strip** (the connected board card's, `module/panel_strip.rs`)
//! the fader takes a `body` — the strip's label row — and the whole strip
//! becomes its gesture surface: a press on the track still jumps there (the
//! native input), and a drag that starts anywhere else moves the value
//! sideways from where it was, the track's width the whole range
//! ([`strip_drag_value`]). A press there that never leaves the tap slop
//! writes nothing and is a tap (`on_tap`). The strip lets a vertical swipe
//! scroll the page (`touch-action: pan-y`).
//!
//! The thumb and fill follow the hand, not the snapshot, through a
//! [`GestureHold`](crate::app::node::gesture_hold::GestureHold): the native input is
//! controlled, so a snapshot still echoing an earlier write of the drag used
//! to be written back into it — the thumb jumped back, then "took".

use std::sync::atomic::{AtomicUsize, Ordering};

use dioxus::prelude::*;
use lpa_studio_core::{ProjectSlotAddress, UiAction, UiPanelTarget, UiSlotFieldState};

use crate::app::node::face::{RATE_DETENTS, adjacent_detent, apply_detents, frac_rate, rate_frac};
use crate::app::node::slot_edit_actions::panel_or_slot_action;
use crate::app::node::slot_fields::field_wiring;

use super::PanelEmit;
use super::knob_field::{KNOB_DISPATCH_INTERVAL_MS, knob_fraction, knob_snap};
use super::panel_tap::{PanelRelease, panel_release, past_tap_slop};
use crate::app::node::gesture_hold::use_gesture_hold;
use crate::app::node::slot_fields::capture_field_pointer;

static NEXT_FADER_TRACK_ID: AtomicUsize = AtomicUsize::new(1);

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn HFaderField(
    value: f32,
    /// Live bus reading (display-only; P6 item 1): the fill renders AT this
    /// value — already violet when bound — while `value` (the authored
    /// default) stays the edit target under the gesture surface.
    #[props(default = None)]
    live_value: Option<f32>,
    min: f32,
    max: f32,
    #[props(default = None)] step: Option<f32>,
    state: UiSlotFieldState,
    /// Violet bound treatment on the fill, slot border, and grip ring.
    #[props(default = false)]
    bound: bool,
    /// Gold ENGAGED treatment (`docs/design/panel.md` P2/P6): a panel
    /// writer has captured this channel and holds it. Outranks the violet
    /// bound family — bound means "wired", engaged means "captured".
    #[props(default = false)]
    engaged: bool,
    #[props(default = None)] address: Option<ProjectSlotAddress>,
    /// Panel-write target: when present, gestures dispatch `PanelWriteOp`
    /// at this `(scope, channel)` instead of editing the authored default.
    #[props(default = None)]
    panel_target: Option<UiPanelTarget>,
    /// Value family the drag dispatches (`F32` default; integer slots
    /// round).
    #[props(default)]
    emit: PanelEmit,
    /// The clock transport's speed mode (G1: "we keep re-inventing faders
    /// — use the normal skeuomorphic one"): the input rides the 0–1
    /// fraction of the log ׼–×8 span, values map through the magnetic
    /// octave detents (×1 pulls hardest), arrows step detent-to-detent,
    /// and double-click seats ×1. The tick row becomes the detent stops.
    /// `min`/`max`/`step` are ignored in this mode.
    #[props(default = false)]
    rate_log_detents: bool,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
    /// What a panel strip draws with the fader (its label row), above the
    /// track; the whole strip is then the gesture surface. `None` = the
    /// fader alone, as every other surface draws it.
    #[props(default)]
    body: Option<Element>,
    /// The gesture surface's own layout when it carries a `body` (the
    /// strip's padding and rows).
    #[props(default)]
    surface_class: &'static str,
    /// A press off the track that never moves past the tap slop: writes
    /// nothing and calls this (the strip opens the control's details).
    #[props(default)]
    on_tap: Option<EventHandler<()>>,
) -> Element {
    let wired = field_wiring(&state, &address, on_action);
    let disabled = wired.is_none();
    // The gesture surface tracks the CURRENT reading — grabbing the thumb
    // of a live control must start from what the fill shows, not snap back
    // to the authored default underneath (GV2 bug, same as the knob).
    let reported = live_value.unwrap_or(value);
    // …and while the hand's own writes are still on their way back, the
    // value under the hand (see `gesture_hold`).
    let mut hold = use_gesture_hold(reported);
    let base = hold.shown(reported);
    // The fill rides the step grid the native input's thumb already snaps
    // to, so a stepped fader never shows fill and thumb in different places.
    let frac = if rate_log_detents {
        rate_frac(base)
    } else {
        knob_fraction(knob_snap(base, min, step), min, max)
    };
    let input_class = fader_input_class(bound, engaged);
    let fill_style = fader_fill_style(frac);
    let fill_class = fader_fill_class(&state, bound, engaged);
    let slot_class = fader_slot_class(&state, bound, engaged);
    // Rate mode drives the native input over the log-frac domain; the
    // oninput mapping owns the value space.
    let (input_min, input_max, input_value) = if rate_log_detents {
        ("0".to_string(), "1".to_string(), frac.to_string())
    } else {
        (min.to_string(), max.to_string(), base.to_string())
    };
    let step_hint = step;
    let step = if rate_log_detents {
        "any".to_string()
    } else {
        step.map_or_else(|| "any".to_string(), |step| step.to_string())
    };
    let invalid_title = state.invalid.clone().unwrap_or_default();
    let key_wired = wired.clone();
    let dbl_wired = wired.clone();
    let move_wired = wired.clone();
    let up_wired = wired.clone();
    let key_target = panel_target.clone();
    let dbl_target = panel_target.clone();
    let move_target = panel_target.clone();
    let up_target = panel_target.clone();
    let has_body = body.is_some();
    let editable = wired.is_some();
    let track_id = use_hook(|| {
        let id = NEXT_FADER_TRACK_ID.fetch_add(1, Ordering::Relaxed);
        format!("ux-fader-track-{id}")
    });
    let track_id_for_press = track_id.clone();
    // A strip's sideways drag off the track: where the press landed, the
    // track's width and the value's fraction then; whether it has gone past
    // the tap slop, and whether it has written.
    let mut strip_drag = use_signal(|| None::<StripDragAnchor>);
    let mut past_slop = use_signal(|| false);
    let mut wrote = use_signal(|| false);
    let last_sent = use_signal(|| 0.0_f64);
    let to_value = move |fraction: f32| {
        if rate_log_detents {
            apply_detents(frac_rate(fraction))
        } else {
            knob_snap(min + fraction * (max - min), min, step_hint).clamp(min, max)
        }
    };

    let track = rsx! {
        div { class: "tw:grid tw:min-w-0 tw:gap-0.5",
            if rate_log_detents {
                RateDetentTicks {}
            } else {
                FaderTicks {}
            }
            div { id: "{track_id}", class: "tw:relative tw:h-7 tw:min-w-0",
                // The recessed slot + value fill; the input above is the
                // gesture surface. Colors ride STATIC classes and the only
                // dynamic style is the fill width: a compound inline style
                // (width + color-mix background) got mangled by attribute
                // diffing on the engaged re-render — background and
                // border-color vanished on alternate updates, the GF-gate
                // "track toggles / goes white" glitch.
                div {
                    class: "tw:pointer-events-none tw:absolute tw:inset-x-0 tw:top-1/2 tw:h-[7px] tw:-translate-y-1/2 tw:overflow-hidden tw:rounded-full tw:border tw:bg-page tw:shadow-[inset_0_1px_2px_rgb(0_0_0/0.35)] {slot_class}",
                    div {
                        class: "tw:absolute tw:inset-y-0 tw:left-0 tw:rounded-full {fill_class}",
                        style: "{fill_style}",
                    }
                }
                input {
                    class: "{input_class}",
                    r#type: "range",
                    min: "{input_min}",
                    max: "{input_max}",
                    step: "{step}",
                    value: "{input_value}",
                    disabled,
                    title: "{invalid_title}",
                    onpointerdown: move |event| {
                        // A press on the track is the native input's (it
                        // jumps there), not the strip's sideways drag.
                        if has_body {
                            event.stop_propagation();
                        }
                        hold.press();
                    },
                    onpointerup: move |_| hold.release(reported),
                    onpointercancel: move |_| hold.release(reported),
                    oninput: move |event| {
                        let Ok(raw) = event.value().parse::<f32>() else {
                            return;
                        };
                        let next = if rate_log_detents {
                            apply_detents(frac_rate(raw))
                        } else {
                            raw
                        };
                        if let Some((address, handler)) = wired.clone() {
                            hold.write(emit.sent_value(next));
                            handler
                                .call(panel_or_slot_action(&panel_target, address, emit.lp_value(next)));
                        }
                    },
                    onkeydown: move |event| {
                        if !rate_log_detents {
                            return;
                        }
                        let Some((address, handler)) = key_wired.clone() else {
                            return;
                        };
                        // The detent stops ARE the keyboard grid (native
                        // range stepping would crawl the raw fraction).
                        let next = match event.key() {
                            Key::ArrowUp | Key::ArrowRight => adjacent_detent(base, true),
                            Key::ArrowDown | Key::ArrowLeft => adjacent_detent(base, false),
                            _ => return,
                        };
                        event.prevent_default();
                        hold.write(emit.sent_value(next));
                        handler
                            .call(panel_or_slot_action(&key_target, address, emit.lp_value(next)));
                    },
                    ondoubleclick: move |_| {
                        if !rate_log_detents {
                            return;
                        }
                        if let Some((address, handler)) = dbl_wired.clone() {
                            hold.write(emit.sent_value(1.0));
                            handler
                                .call(panel_or_slot_action(&dbl_target, address, emit.lp_value(1.0)));
                        }
                    },
                }
            }
        }
    };

    let Some(body) = body else {
        return track;
    };
    rsx! {
        div {
            class: "ux-panel-surface {surface_class} tw:cursor-ew-resize tw:touch-pan-y",
            "data-panel-surface": "",
            onpointerdown: move |event| {
                if !editable {
                    return;
                }
                capture_field_pointer(&event);
                hold.press();
                let at = event.data().client_coordinates();
                let width = element_width(&track_id_for_press).unwrap_or(0.0);
                strip_drag.set(Some(StripDragAnchor {
                    press: (at.x, at.y),
                    width,
                    fraction: frac,
                }));
                past_slop.set(on_tap.is_none());
                wrote.set(false);
            },
            onpointermove: move |event| {
                let mut last_sent = last_sent;
                let Some(anchor) = strip_drag() else {
                    return;
                };
                if event.data().held_buttons().is_empty() {
                    // Missed release (no pointer capture): stop the drag.
                    strip_drag.set(None);
                    hold.clear();
                    return;
                }
                let at = event.data().client_coordinates();
                // Under the slop it may still be a tap; past it the drag
                // counts from the press, so the value follows the whole of
                // the hand's travel however coarse the moves come.
                if !past_slop() {
                    if !past_tap_slop(anchor.press, (at.x, at.y)) {
                        return;
                    }
                    past_slop.set(true);
                }
                let Some((address, handler)) = move_wired.clone() else {
                    return;
                };
                let fraction =
                    strip_drag_fraction(anchor.fraction, at.x - anchor.press.0, anchor.width);
                let next = to_value(fraction);
                hold.write(emit.sent_value(next));
                wrote.set(true);
                // Throttled, as the knob's drag is; the release flushes.
                let now = js_sys::Date::now();
                if now - last_sent() < KNOB_DISPATCH_INTERVAL_MS {
                    return;
                }
                last_sent.set(now);
                handler.call(panel_or_slot_action(&move_target, address, emit.lp_value(next)));
            },
            onpointerup: move |_| {
                match panel_release(strip_drag().is_some(), past_slop(), wrote()) {
                    PanelRelease::Flush => {
                        if let (Some(next), Some((address, handler))) =
                            (hold.held(), up_wired.clone())
                        {
                            handler
                                .call(panel_or_slot_action(&up_target, address, emit.lp_value(next)));
                        }
                    }
                    PanelRelease::Tap => {
                        if let Some(on_tap) = on_tap {
                            on_tap.call(());
                        }
                    }
                    PanelRelease::Nothing => {}
                }
                strip_drag.set(None);
                hold.release(reported);
            },
            onpointercancel: move |_| {
                strip_drag.set(None);
                hold.clear();
            },
            {body}
            {track}
        }
    }
}

/// Where a strip's sideways drag started: the press point (the tap slop's
/// centre, and where the drag counts from), the track's width, and the
/// value's fraction of the track then.
#[derive(Clone, Copy, Debug, PartialEq)]
struct StripDragAnchor {
    press: (f64, f64),
    width: f64,
    fraction: f32,
}

/// The fraction of the track a strip's sideways drag lands on: `dx` CSS px
/// from where it started, the track's `width` the whole range, clamped to
/// the track. A track with no width moves nothing.
pub(crate) fn strip_drag_fraction(anchor_fraction: f32, dx: f64, width: f64) -> f32 {
    if width <= 0.0 {
        return anchor_fraction.clamp(0.0, 1.0);
    }
    (anchor_fraction + (dx / width) as f32).clamp(0.0, 1.0)
}

/// An element's rendered width in CSS px, by id.
fn element_width(id: &str) -> Option<f64> {
    let element = web_sys::window()?.document()?.get_element_by_id(id)?;
    Some(element.get_bounding_client_rect().width())
}

/// The rate fader's tick row: one stop per octave detent at its log-frac
/// position, ×1 taller — the stops ARE the scale (spike round 2).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn RateDetentTicks() -> Element {
    rsx! {
        div { class: "tw:pointer-events-none tw:relative tw:h-1.5 tw:px-2",
            for detent in RATE_DETENTS {
                span {
                    key: "{detent}",
                    class: if detent == 1.0 { "tw:absolute tw:bottom-0 tw:h-1.5 tw:w-px tw:bg-[var(--studio-color-text-subtle)] tw:opacity-80" } else { "tw:absolute tw:bottom-0 tw:h-1 tw:w-px tw:bg-[var(--studio-color-text-subtle)] tw:opacity-50" },
                    style: format!(
                        "left: calc(8px + (100% - 16px) * {});",
                        rate_frac(detent),
                    ),
                }
            }
        }
    }
}

/// One row of five tick marks (min / ¼ / mid / ¾ / max), inset by roughly
/// half a grip so the end ticks sit near the grip-center extremes.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn FaderTicks() -> Element {
    rsx! {
        div { class: "tw:pointer-events-none tw:flex tw:justify-between tw:px-2",
            for index in 0..5 {
                span {
                    key: "{index}",
                    class: "tw:h-1 tw:w-px tw:bg-[var(--studio-color-text-subtle)] tw:opacity-50",
                }
            }
        }
    }
}

/// Input classes: the transparent gesture surface (with the styled grip)
/// plus the bound / engaged grip ring.
pub(crate) fn fader_input_class(bound: bool, engaged: bool) -> &'static str {
    if engaged {
        "ux-hfader is-engaged"
    } else if bound {
        "ux-hfader is-bound"
    } else {
        "ux-hfader"
    }
}

/// The value fill's color class by status family: amber when engaged (a
/// panel writer holds the channel — the rail wears the capture), violet
/// when bound, error when invalid, bright neutral otherwise (accent
/// reckoning D4 — an authored value is chrome; green stays valid-only).
/// STATIC classes on purpose: the colors used to ride a
/// compound dynamic inline style and attribute diffing mangled it (the
/// GF-gate track glitch); only the numeric width stays dynamic.
pub(crate) fn fader_fill_class(
    state: &UiSlotFieldState,
    bound: bool,
    engaged: bool,
) -> &'static str {
    if engaged {
        "tw:bg-[color-mix(in_oklab,var(--studio-status-engaged-text)_45%,var(--studio-color-surface-muted))]"
    } else if bound {
        "tw:bg-[color-mix(in_oklab,var(--studio-status-bound-text)_45%,var(--studio-color-surface-muted))]"
    } else if state.invalid.is_some() {
        "tw:bg-[color-mix(in_oklab,var(--studio-status-error-text)_45%,var(--studio-color-surface-muted))]"
    } else {
        "tw:bg-[color-mix(in_oklab,var(--studio-color-text-strong)_45%,var(--studio-color-surface-muted))]"
    }
}

/// The slot's border-color class, same families as the fill.
pub(crate) fn fader_slot_class(
    state: &UiSlotFieldState,
    bound: bool,
    engaged: bool,
) -> &'static str {
    if engaged {
        "tw:border-[var(--studio-status-engaged-border)]"
    } else if bound {
        "tw:border-[var(--studio-status-bound-border)]"
    } else if state.invalid.is_some() {
        "tw:border-[var(--studio-status-error-border)]"
    } else {
        "tw:border-[var(--studio-color-border-strong)]"
    }
}

/// Inline style for the value fill inside the slot: width only — color is
/// class-driven so the one dynamic style stays a single numeric property.
pub(crate) fn fader_fill_style(frac: f32) -> String {
    format!("width: {:.1}%;", frac * 100.0)
}

#[cfg(test)]
mod tests {
    use lpa_studio_core::UiSlotFieldState;

    use super::{
        fader_fill_class, fader_fill_style, fader_input_class, fader_slot_class,
        strip_drag_fraction,
    };

    /// A strip's sideways drag: the track's width is the whole range, from
    /// wherever the value was, clamped to the track's ends.
    #[test]
    fn the_strips_drag_maps_the_track_width_to_the_range_and_clamps() {
        // Half the track's width from a quarter: three quarters.
        assert_eq!(strip_drag_fraction(0.25, 100.0, 200.0), 0.75);
        // Leftward moves it down, from where it was (no jump to the press).
        assert_eq!(strip_drag_fraction(0.5, -50.0, 200.0), 0.25);
        // Past either end: the end.
        assert_eq!(strip_drag_fraction(0.9, 400.0, 200.0), 1.0);
        assert_eq!(strip_drag_fraction(0.1, -400.0, 200.0), 0.0);
        // A track not laid out yet moves nothing.
        assert_eq!(strip_drag_fraction(0.4, 120.0, 0.0), 0.4);
    }

    #[test]
    fn fill_style_is_width_only() {
        // The one dynamic style stays a single numeric property — a
        // compound width+background inline style got mangled by attribute
        // diffing on the engaged re-render (GF-gate track glitch).
        let style = fader_fill_style(0.72);
        assert_eq!(style, "width: 72.0%;");
    }

    #[test]
    fn bound_fader_wears_the_violet_family() {
        let fill = fader_fill_class(&UiSlotFieldState::editable(), true, false);
        assert!(fill.contains("--studio-status-bound-text"));
        let slot = fader_slot_class(&UiSlotFieldState::editable(), true, false);
        assert!(slot.contains("--studio-status-bound-border"));
        assert!(fader_input_class(true, false).contains("is-bound"));
        assert!(!fader_input_class(false, false).contains("is-bound"));
    }

    #[test]
    fn engaged_fader_wears_amber_over_the_bound_violet() {
        // Same rule as the knob: a captured fader has stopped following its
        // binding, so the engaged family wins (panel.md P-Q2) — and the
        // amber rides a STATIC class, never a compound inline style.
        let fill = fader_fill_class(&UiSlotFieldState::editable(), true, true);
        assert!(fill.contains("--studio-status-engaged-text"));
        assert!(!fill.contains("bound"));
        assert!(fader_input_class(true, true).contains("is-engaged"));
    }

    #[test]
    fn invalid_fill_wears_the_error_family_when_unbound() {
        let state = UiSlotFieldState::editable().with_invalid("too bright");
        assert!(fader_fill_class(&state, false, false).contains("--studio-status-error-text"));
    }
}
