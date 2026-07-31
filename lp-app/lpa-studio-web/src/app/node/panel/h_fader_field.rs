//! Horizontal fader field v2 — the fixture face's dominant brightness
//! control (P2b item 4; build A settled at the P2c re-gate).
//!
//! Anatomy: one tick row above a thin recessed slot (7px, inset-shadowed)
//! with the value fill riding inside it, and a pronounced squared block
//! grip (20 × 24, vertical gradient + center hairline) — a native range
//! input stretched invisibly over the slot supplies the gesture surface
//! while `.ux-hfader` in `style.css` draws the thumb. Accent fill normally,
//! the violet bound family when the backing slot is bound. Dispatches
//! `SlotEditOp::SetValue` with `oninput` semantics (the actor coalesces the
//! drag flood per address).

use dioxus::prelude::*;
use lpa_studio_core::{ProjectSlotAddress, UiAction, UiSlotFieldState};

use crate::app::node::slot_edit_actions::slot_set_value_action;
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
    /// Value family the drag dispatches (`F32` default; integer slots
    /// round).
    #[props(default)]
    emit: PanelEmit,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let wired = field_wiring(&state, &address, on_action);
    let disabled = wired.is_none();
    // The fill rides the step grid the native input's thumb already snaps
    // to, so a stepped fader never shows fill and thumb in different places.
    let frac = knob_fraction(knob_snap(live_value.unwrap_or(value), min, step), min, max);
    let input_class = fader_input_class(bound, engaged);
    let fill_style = fader_fill_style(frac, &state, bound, engaged);
    let slot_style = fader_slot_style(&state, bound, engaged);
    let step = step.map_or_else(|| "any".to_string(), |step| step.to_string());
    let invalid_title = state.invalid.clone().unwrap_or_default();

    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:gap-0.5",
            FaderTicks {}
            div { class: "tw:relative tw:h-7 tw:min-w-0",
                // The recessed slot + value fill; the input above is the
                // gesture surface.
                div {
                    class: "tw:pointer-events-none tw:absolute tw:inset-x-0 tw:top-1/2 tw:h-[7px] tw:-translate-y-1/2 tw:overflow-hidden tw:rounded-full tw:border tw:bg-page tw:shadow-[inset_0_1px_2px_rgb(0_0_0/0.35)]",
                    style: "{slot_style}",
                    div {
                        class: "tw:absolute tw:inset-y-0 tw:left-0 tw:rounded-full",
                        style: "{fill_style}",
                    }
                }
                input {
                    class: "{input_class}",
                    r#type: "range",
                    min: "{min}",
                    max: "{max}",
                    step: "{step}",
                    value: "{value}",
                    disabled,
                    title: "{invalid_title}",
                    oninput: move |event| {
                        if let (Some((address, handler)), Ok(next)) =
                            (wired.clone(), event.value().parse::<f32>())
                        {
                            handler.call(slot_set_value_action(address, emit.lp_value(next)));
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

/// Fill and border colors by status family: amber when engaged, violet when
/// bound, error when invalid, accent otherwise (green stays valid-only).
fn fader_fill_colors(
    state: &UiSlotFieldState,
    bound: bool,
    engaged: bool,
) -> (&'static str, &'static str) {
    if engaged {
        (
            "color-mix(in oklab, var(--studio-status-attention-text) 45%, var(--studio-color-surface-muted))",
            "var(--studio-status-attention-border)",
        )
    } else if bound {
        (
            "color-mix(in oklab, var(--studio-status-bound-text) 45%, var(--studio-color-surface-muted))",
            "var(--studio-status-bound-border)",
        )
    } else if state.invalid.is_some() {
        (
            "color-mix(in oklab, var(--studio-status-error-text) 45%, var(--studio-color-surface-muted))",
            "var(--studio-status-error-border)",
        )
    } else {
        (
            "color-mix(in oklab, var(--studio-color-accent) 45%, var(--studio-color-surface-muted))",
            "var(--studio-color-border-strong)",
        )
    }
}

/// Inline style for the value fill inside the slot: width = the value
/// fraction, background = the status family's fill mix.
pub(crate) fn fader_fill_style(
    frac: f32,
    state: &UiSlotFieldState,
    bound: bool,
    engaged: bool,
) -> String {
    let (fill, _) = fader_fill_colors(state, bound, engaged);
    format!("width: {:.1}%; background: {fill};", frac * 100.0)
}

/// Inline style for the slot border so bound faders read violet at a
/// glance.
pub(crate) fn fader_slot_style(state: &UiSlotFieldState, bound: bool, engaged: bool) -> String {
    let (_, border) = fader_fill_colors(state, bound, engaged);
    format!("border-color: {border};")
}

#[cfg(test)]
mod tests {
    use lpa_studio_core::UiSlotFieldState;

    use super::{fader_fill_style, fader_input_class, fader_slot_style};

    #[test]
    fn fill_sizes_to_the_value_fraction() {
        let style = fader_fill_style(0.72, &UiSlotFieldState::editable(), false, false);
        assert!(style.contains("width: 72.0%"));
        assert!(style.contains("--studio-color-accent"));
    }

    #[test]
    fn bound_fader_wears_the_violet_family() {
        let fill = fader_fill_style(0.5, &UiSlotFieldState::editable(), true, false);
        assert!(fill.contains("--studio-status-bound-text"));
        let slot = fader_slot_style(&UiSlotFieldState::editable(), true, false);
        assert!(slot.contains("--studio-status-bound-border"));
        assert!(fader_input_class(true, false).contains("is-bound"));
        assert!(!fader_input_class(false, false).contains("is-bound"));
    }

    #[test]
    fn engaged_fader_wears_amber_over_the_bound_violet() {
        // Same rule as the knob: a captured fader has stopped following its
        // binding, so the engaged family wins (panel.md P-Q2).
        let fill = fader_fill_style(0.5, &UiSlotFieldState::editable(), true, true);
        assert!(fill.contains("--studio-status-attention-text"));
        assert!(!fill.contains("bound"));
        assert!(fader_input_class(true, true).contains("is-engaged"));
    }

    #[test]
    fn invalid_fill_wears_the_error_family_when_unbound() {
        let state = UiSlotFieldState::editable().with_invalid("too bright");
        assert!(fader_fill_style(0.5, &state, false, false).contains("--studio-status-error-text"));
    }
}
