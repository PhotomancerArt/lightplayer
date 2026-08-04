//! Horizontal fader field v2 — the fixture face's dominant brightness
//! control (P2b item 4; build A settled at the P2c re-gate).
//!
//! Anatomy: one tick row above a thin recessed slot (7px, inset-shadowed)
//! with the value fill riding inside it, and a pronounced squared block
//! grip (20 × 24, vertical gradient + center hairline) — a native range
//! input stretched invisibly over the slot supplies the gesture surface
//! while `.ux-hfader` in `style.css` draws the thumb. Accent fill normally,
//! the violet bound family when the backing slot is bound; ENGAGED marks
//! only the grip (amber ring) — the track keeps its family. Dispatches
//! `SlotEditOp::SetValue` with `oninput` semantics (the actor coalesces the
//! drag flood per address).

use dioxus::prelude::*;
use lpa_studio_core::{ProjectSlotAddress, UiAction, UiPanelTarget, UiSlotFieldState};

use crate::app::node::slot_edit_actions::panel_or_slot_action;
use crate::app::node::slot_fields::field_wiring;

use super::PanelEmit;
use super::knob_field::{knob_fraction, knob_snap};

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
    /// Amber ENGAGED treatment (`docs/design/panel.md` P2/P6): a panel
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
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let wired = field_wiring(&state, &address, on_action);
    let disabled = wired.is_none();
    // The gesture surface tracks the CURRENT reading — grabbing the thumb
    // of a live control must start from what the fill shows, not snap back
    // to the authored default underneath (GV2 bug, same as the knob).
    let base = live_value.unwrap_or(value);
    // The fill rides the step grid the native input's thumb already snaps
    // to, so a stepped fader never shows fill and thumb in different places.
    let frac = knob_fraction(knob_snap(base, min, step), min, max);
    let input_class = fader_input_class(bound, engaged);
    let fill_style = fader_fill_style(frac);
    let fill_class = fader_fill_class(&state, bound);
    let slot_class = fader_slot_class(&state, bound);
    let step = step.map_or_else(|| "any".to_string(), |step| step.to_string());
    let invalid_title = state.invalid.clone().unwrap_or_default();

    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:gap-0.5",
            FaderTicks {}
            div { class: "tw:relative tw:h-7 tw:min-w-0",
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
                    min: "{min}",
                    max: "{max}",
                    step: "{step}",
                    value: "{base}",
                    disabled,
                    title: "{invalid_title}",
                    oninput: move |event| {
                        if let (Some((address, handler)), Ok(next)) =
                            (wired.clone(), event.value().parse::<f32>())
                        {
                            handler
                                .call(panel_or_slot_action(&panel_target, address, emit.lp_value(next)));
                        }
                    },
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

/// The value fill's color class by status family: violet when bound, error
/// when invalid, accent otherwise (green stays valid-only). Engaged does
/// NOT recolor the fill — the grip's amber ring (`is-engaged`) and the
/// reset glyph are the whole engaged treatment (GF gate: the track stays
/// the same as the other controls; the orange highlights are plenty).
pub(crate) fn fader_fill_class(state: &UiSlotFieldState, bound: bool) -> &'static str {
    if bound {
        "tw:bg-[color-mix(in_oklab,var(--studio-status-bound-text)_45%,var(--studio-color-surface-muted))]"
    } else if state.invalid.is_some() {
        "tw:bg-[color-mix(in_oklab,var(--studio-status-error-text)_45%,var(--studio-color-surface-muted))]"
    } else {
        "tw:bg-[color-mix(in_oklab,var(--studio-color-accent)_45%,var(--studio-color-surface-muted))]"
    }
}

/// The slot's border-color class, same families as the fill.
pub(crate) fn fader_slot_class(state: &UiSlotFieldState, bound: bool) -> &'static str {
    if bound {
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

    use super::{fader_fill_class, fader_fill_style, fader_input_class, fader_slot_class};

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
        let fill = fader_fill_class(&UiSlotFieldState::editable(), true);
        assert!(fill.contains("--studio-status-bound-text"));
        let slot = fader_slot_class(&UiSlotFieldState::editable(), true);
        assert!(slot.contains("--studio-status-bound-border"));
        assert!(fader_input_class(true, false).contains("is-bound"));
        assert!(!fader_input_class(false, false).contains("is-bound"));
    }

    #[test]
    fn engaged_keeps_the_track_family_and_marks_only_the_grip() {
        // GF gate: engagement is the grip's amber ring + the reset glyph;
        // the track keeps its status family (accent, or violet when the
        // fader still follows a binding) instead of flooding pale amber.
        let fill = fader_fill_class(&UiSlotFieldState::editable(), false);
        assert!(fill.contains("--studio-color-accent"));
        assert!(!fill.contains("attention"));
        assert!(fader_input_class(true, true).contains("is-engaged"));
    }

    #[test]
    fn invalid_fill_wears_the_error_family_when_unbound() {
        let state = UiSlotFieldState::editable().with_invalid("too bright");
        assert!(fader_fill_class(&state, false).contains("--studio-status-error-text"));
    }
}
