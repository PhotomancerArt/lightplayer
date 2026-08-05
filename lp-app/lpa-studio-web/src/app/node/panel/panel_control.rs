//! Shared chrome for one node-face panel control.
//!
//! Renders a [`lpa_studio_core::UiPanelControl`]: the widget (knob, fader,
//! or toggle) with its name and value readout. The NAME is the detail
//! trigger — a label button (name + small info glyph) wearing the slot's
//! state color (violet bound, warning unsaved, blue live-transient, red
//! failed; green stays valid-only) that opens the SAME [`SlotDetailButton`]
//! popover as the backing slot row — identical aspect sections, no forked
//! content. The label replaces both the old hover-corner ⓘ (which covered
//! the knob) and the edit-state dot (the label color now carries that
//! signal; the widget itself still wears the violet bound family, so a
//! bound control with an unsaved edit shows both at once).
//!
//! The readout carries exactly ONE value (GV fix 3): the live bus reading
//! when the channel has one, else the authored value. It sits in a
//! width-reserved, tabular-numeral slot ([`READOUT_CLASS`]) so a drag never
//! reflows the control.
//!
//! The control IS the popover visual (P2c item 3): opening merges the
//! CONTROL's outline with the aspect card below through the
//! contiguous-outline system (`base/outline.rs` + `base/popover.rs`
//! anchored mode) — one contiguous shape, "diving into the control". While
//! open, the top layer paints a live copy of the same [`PanelControlBody`]
//! over the control, so nothing is duplicated inside the panel.

use std::sync::atomic::{AtomicUsize, Ordering};

use dioxus::prelude::*;
use lpa_studio_core::{
    UiAction, UiPanelControl as UiPanelControlData, UiPanelEmit, UiPanelWidget, UiSlotAffordance,
    UiSlotValueKind,
};

use crate::app::node::slot_detail_button::primary_affordance;
use crate::app::node::slot_edit_actions::panel_clear_action;
use crate::app::node::{SlotDetailButton, SlotUnitSuffix};
use crate::base::{PopoverPlacement, StudioIcon, StudioIconName};

use super::{HFaderField, KnobField, PanelEmit, ToggleField};

static NEXT_PANEL_CONTROL_ID: AtomicUsize = AtomicUsize::new(1);

/// Button chrome for the label trigger: bare text button; the state color
/// rides the inner visual, the hover affordance is a dotted underline (the
/// persistent info glyph is the standing hint).
const PANEL_LABEL_TRIGGER_CLASS: &str = "tw:inline-flex tw:min-w-0 tw:cursor-pointer tw:appearance-none tw:items-center tw:border-0 tw:bg-transparent tw:p-0 tw:leading-none tw:decoration-dotted tw:underline-offset-2 tw:hover:underline";

/// The value slot: tabular numerals in a reserved box, so 0.6 / 0.62 / 200
/// all occupy the same width and the control does not reflow mid-drag (GV
/// fix 3 — the same treatment the module panel's readout already had).
const READOUT_CLASS: &str = "tw:inline-flex tw:min-w-[4.5ch] tw:items-baseline tw:justify-center tw:gap-1 tw:font-mono tw:text-[0.7rem] tw:tabular-nums";

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn PanelControl(
    control: UiPanelControlData,
    /// Open the label trigger's detail popover on first render (stories).
    #[props(default = false)]
    detail_initially_open: bool,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    if !value_matches_widget(&control) {
        return mismatch_fallback(&control);
    }

    // The anchored-outline id: the whole control element is the popover's
    // outline anchor (P2c item 3).
    let anchor_id = use_hook(|| {
        let id = NEXT_PANEL_CONTROL_ID.fetch_add(1, Ordering::Relaxed);
        format!("ux-panel-control-{id}")
    });
    let outer_class = match control.widget {
        UiPanelWidget::Fader { .. } => "tw:grid tw:min-w-0 tw:gap-1.5",
        UiPanelWidget::Knob { .. } | UiPanelWidget::Toggle => {
            "tw:flex tw:min-w-[52px] tw:flex-none tw:flex-col tw:items-center tw:gap-1"
        }
    };
    // The top-layer copy of the control painted over the anchor while the
    // popover is open — same component, same data, sized to the anchor rect.
    let anchor_visual_class = match control.widget {
        UiPanelWidget::Fader { .. } => {
            "tw:grid tw:h-full tw:w-full tw:min-w-0 tw:content-start tw:gap-1.5"
        }
        UiPanelWidget::Knob { .. } | UiPanelWidget::Toggle => {
            "tw:flex tw:h-full tw:w-full tw:flex-col tw:items-center tw:gap-1"
        }
    };
    let label_class = panel_label_class(&control);
    let anchor_control = control.clone();
    // The label rendered inside the popover's top-layer copy: the same
    // visual as the trigger, non-interactive (the backdrop owns dismissal).
    let anchor_label = panel_label_visual(&control.label, label_class);
    // The in-flow label IS the popover trigger: a real button, so keyboard
    // focus + Enter/Space open the same aspects the slot row shows.
    let label_trigger = rsx! {
        SlotDetailButton {
            label: control.label.clone(),
            // The popup keeps the authored value the face may be
            // displacing with a live reading (GV fix 3).
            aspects: control.detail_aspects(),
            // The popover titles the control by its authored label (P6
            // item 2) instead of the backing field row's name ("Default").
            name_override: Some(control.label.clone()),
            initially_open: detail_initially_open,
            placement: PopoverPlacement::BottomMiddle,
            anchor_id: Some(anchor_id.clone()),
            anchor_visual: rsx! {
                div { class: anchor_visual_class,
                    PanelControlBody { control: anchor_control, label: anchor_label, on_action }
                }
            },
            trigger: Some(panel_label_visual(&control.label, label_class)),
            trigger_class: PANEL_LABEL_TRIGGER_CLASS.to_string(),
            trigger_open_class: PANEL_LABEL_TRIGGER_CLASS.to_string(),
            on_action,
        }
    };

    rsx! {
        div { id: "{anchor_id}", class: outer_class,
            PanelControlBody { control, label: label_trigger, on_action }
        }
    }
}

/// The control itself — widget, label slot, and readout — without popover
/// wiring. Rendered on the card face (label slot = the trigger button) AND
/// as the popover's top-layer anchor visual (label slot = the static label
/// copy); the fragment adopts the caller's column/grid layout.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PanelControlBody(
    control: UiPanelControlData,
    /// The label element: the interactive trigger in flow, a static copy in
    /// the top-layer visual.
    label: Element,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let bound = control.bound();
    // A panel-targeted control with an ENGAGED writer gets the clear
    // affordance (panel.md P2): a small ↺ beside the readout releasing the
    // held value back to the authored wiring.
    let clear = control
        .panel_target
        .as_ref()
        .filter(|target| target.engaged)
        .map(|target| {
            let action = panel_clear_action(target);
            rsx! {
                button {
                    class: "tw:absolute tw:left-full tw:top-1/2 tw:ml-0.5 tw:inline-flex tw:flex-none tw:-translate-y-1/2 tw:cursor-pointer tw:appearance-none tw:items-center tw:border-0 tw:bg-transparent tw:p-0 tw:leading-none tw:text-status-bound-foreground tw:opacity-70 tw:hover:opacity-100",
                    r#type: "button",
                    title: "Release the held panel value",
                    onclick: move |event| {
                        event.stop_propagation();
                        if let Some(handler) = on_action {
                            handler.call(action.clone());
                        }
                    },
                    StudioIcon { name: StudioIconName::Revert, size: 10 }
                }
            }
        });
    // ONE value on the face (GV fix 3): the live bus reading when the
    // channel has one — in the bound-violet family — else the authored
    // value. The authored value the live reading displaces is a row in the
    // detail popup ([`UiPanelControlData::detail_aspects`]); printed here
    // in parentheses it read as a second value and, worse, reflowed the
    // control's width on every step of a drag.
    let live = control.live_value.is_some();
    let readout_class = if live {
        "tw:text-status-bound-foreground"
    } else {
        "tw:text-muted-foreground"
    };
    let readout_title = if live {
        "live bus value"
    } else {
        "authored value"
    };
    // A phasor speed knob stores period_seconds but PRESENTS as speed (G2
    // feedback — "phase period" was expert vocabulary): the readout is the
    // reciprocal ("1/100 s") and the drag axis inverts so up = faster.
    // PROVISIONAL display language pending the clock-face UX spike.
    let phasor = matches!(control.emit, UiPanelEmit::PhasorPeriod { .. });
    let shown_value = if phasor {
        phasor_speed_display(control.shown_display())
    } else {
        control.shown_display().to_string()
    };
    let readout = rsx! {
        // `relative` so the engaged clear hangs past the readout without
        // occupying layout — its appearance must not reflow the row (GV2).
        span { class: "tw:relative {READOUT_CLASS} {readout_class}",
            span { title: readout_title, "{shown_value}" }
            SlotUnitSuffix { unit: control.unit.clone(), reserve: false }
            {clear}
        }
    };

    match control.widget.clone() {
        UiPanelWidget::Knob { min, max, step } => {
            let Some((value, emit)) = numeric_value(&control) else {
                return mismatch_fallback(&control);
            };
            rsx! {
                KnobField {
                    value,
                    live_value: control.live_numeric(),
                    min,
                    max,
                    step,
                    state: control.state.clone(),
                    bound,
                    invert: phasor,
                    address: control.address.clone(),
                    panel_target: control.panel_target.clone(),
                    emit,
                    on_action,
                }
                {label}
                {readout}
            }
        }
        UiPanelWidget::Fader { min, max, step } => {
            let Some((value, emit)) = numeric_value(&control) else {
                return mismatch_fallback(&control);
            };
            rsx! {
                div { class: "tw:flex tw:items-baseline tw:justify-between tw:gap-2",
                    {label}
                    {readout}
                }
                HFaderField {
                    value,
                    live_value: control.live_numeric(),
                    min,
                    max,
                    step,
                    state: control.state.clone(),
                    bound,
                    address: control.address.clone(),
                    panel_target: control.panel_target.clone(),
                    emit,
                    on_action,
                }
            }
        }
        UiPanelWidget::Toggle => {
            let Some(value) = bool_value(&control) else {
                return mismatch_fallback(&control);
            };
            rsx! {
                // Center the pill in the knob's vertical band so mixed
                // rows keep one baseline.
                span { class: "tw:flex tw:h-[46px] tw:items-center",
                    ToggleField {
                        value,
                        live_value: control.live_bool(),
                        state: control.state.clone(),
                        bound,
                        address: control.address.clone(),
                        panel_target: control.panel_target.clone(),
                        on_action,
                    }
                }
                {label}
                {readout}
            }
        }
    }
}

/// A period reading presented as an auto-denominated rate: `"0.5"` → `2/s`,
/// `"20"` → `3/min`, `"240"` → `15/hr` (G2 convergence — pick the smallest
/// time unit that keeps the number ≥ 1, so the reading is always a natural
/// count; the unit is part of the string, so these controls carry no
/// separate unit suffix). A frozen phasor (period 0) never cycles: `0/s`.
/// A reading that does not parse passes through untouched.
pub(crate) fn phasor_speed_display(shown: &str) -> String {
    let Ok(period) = shown.trim().parse::<f32>() else {
        return shown.to_string();
    };
    if period <= 0.0 {
        return "0/s".to_string();
    }
    // Smallest unit whose count reaches 1; /hr is the floor either way.
    let (count, unit) = [(1.0, "s"), (60.0, "min"), (3600.0, "hr")]
        .into_iter()
        .map(|(seconds, unit)| (seconds / period, unit))
        .find(|(count, unit)| *count >= 1.0 || *unit == "hr")
        .expect("the ladder always yields");
    let number = if count >= 9.95 {
        format!("{}", count.round() as i64)
    } else {
        let rounded = (count * 10.0).round() / 10.0;
        if rounded.fract() == 0.0 {
            format!("{}", rounded as i64)
        } else {
            format!("{rounded:.1}")
        }
    };
    format!("{number}/{unit}")
}

/// The label visual shared by the trigger button and its top-layer copy:
/// the control name in the panel's label typography plus a small info glyph
/// (the standing "this opens details" hint), both in the state color.
fn panel_label_visual(label: &str, color_class: &'static str) -> Element {
    rsx! {
        span { class: "tw:inline-flex tw:min-w-0 tw:items-center tw:gap-1 {color_class}",
            span { class: "tw:truncate tw:text-[0.66rem] tw:font-bold tw:uppercase tw:leading-none tw:tracking-[0.08em]",
                "{label}"
            }
            span { class: "tw:inline-flex tw:flex-none tw:opacity-70",
                StudioIcon { name: StudioIconName::InfoBare, size: 10 }
            }
        }
    }
}

/// The label's state color — the same affordance ladder as the backing slot
/// row's detail trigger ([`primary_affordance`]): red for failed/invalid,
/// warning for an unsaved edit (live-blue when the edit is transient),
/// working while saving, violet when bound, quiet otherwise. Green stays
/// valid-only. An edited BOUND slot shows the edit color here while the
/// widget itself keeps the violet family — both states stay visible, which
/// is why the separate edit-state dot is gone.
fn panel_label_class(control: &UiPanelControlData) -> &'static str {
    match primary_affordance(&control.aspects) {
        UiSlotAffordance::Error | UiSlotAffordance::Invalid => "tw:text-status-error-foreground",
        // A Debug control fronted on a panel keeps the panel surface's own
        // live-value blue: preview/play surfaces are out of the debug
        // treatment's scope (D8) — the panels line owns their indication.
        UiSlotAffordance::Edited if control.state.debug => "tw:text-status-live-foreground",
        UiSlotAffordance::Edited => "tw:text-status-warning-foreground",
        UiSlotAffordance::Saving => "tw:text-status-working-foreground",
        UiSlotAffordance::Bound => "tw:text-status-bound-foreground",
        UiSlotAffordance::Info => "tw:text-subtle-foreground",
    }
}

/// Whether the widget family and the value family agree — disagreement
/// falls back to the read-only display with no gesture surface or detail
/// trigger.
fn value_matches_widget(control: &UiPanelControlData) -> bool {
    match control.widget {
        UiPanelWidget::Knob { .. } | UiPanelWidget::Fader { .. } => {
            numeric_value(control).is_some()
        }
        UiPanelWidget::Toggle => bool_value(control).is_some(),
    }
}

/// Numeric payload + emit family for knob/fader widgets (f32 and integer
/// slot families; integer gestures round on dispatch). `None` (a
/// non-numeric slot behind a numeric widget) falls back to the read-only
/// display.
fn numeric_value(control: &UiPanelControlData) -> Option<(f32, PanelEmit)> {
    PanelEmit::for_control(control)
}

/// Boolean payload for toggle widgets.
fn bool_value(control: &UiPanelControlData) -> Option<bool> {
    match control.value.kind {
        UiSlotValueKind::Bool(value) => Some(value),
        _ => None,
    }
}

/// Read-only fallback when the widget family and value family disagree —
/// the plain label plus the slot's own display string, no gesture surface.
fn mismatch_fallback(control: &UiPanelControlData) -> Element {
    rsx! {
        div { class: "tw:flex tw:min-w-[52px] tw:flex-none tw:flex-col tw:items-center tw:gap-1",
            span { class: "tw:text-[0.66rem] tw:font-bold tw:uppercase tw:tracking-[0.08em] tw:text-subtle-foreground",
                "{control.label}"
            }
            span { class: "tw:font-mono tw:text-[0.7rem] tw:text-muted-foreground", "{control.value.display}" }
        }
    }
}

#[cfg(test)]
mod tests {
    use lpa_studio_core::{
        UiBindingEndpoint, UiConfigSlot, UiNodeDirtyState, UiPanelControl, UiPanelEmit,
        UiPanelWidget, UiSlotFieldState, UiSlotSourceState, UiSlotValue,
    };

    use super::{PanelEmit, bool_value, numeric_value, panel_label_class, value_matches_widget};

    fn control_with_source(
        value: UiSlotValue,
        state: UiSlotFieldState,
        source: UiSlotSourceState,
    ) -> UiPanelControl {
        // Aspects derive through a real UiConfigSlot exactly like the face
        // builder and the story fixtures, so the label ladder is exercised
        // against production aspect affordances.
        let aspect_slot = UiConfigSlot::value("speed", "speed", value.clone())
            .with_state(state.clone())
            .with_source(source);
        UiPanelControl {
            emit: UiPanelEmit::Value,
            label: "speed".to_string(),
            address: None,
            widget: UiPanelWidget::Knob {
                min: 0.0,
                max: 4.0,
                step: None,
            },
            value,
            live_value: None,
            panel_target: None,
            unit: None,
            state,
            aspects: aspect_slot.visible_aspects(),
        }
    }

    fn control(value: UiSlotValue, state: UiSlotFieldState) -> UiPanelControl {
        control_with_source(value, state, UiSlotSourceState::Unset)
    }

    fn bound_source() -> UiSlotSourceState {
        UiSlotSourceState::Bound(UiBindingEndpoint::new("bus:master-tempo"))
    }

    #[test]
    fn value_extraction_covers_the_numeric_families() {
        let editable = UiSlotFieldState::editable();
        assert_eq!(
            numeric_value(&control(UiSlotValue::f32(1.6), editable.clone())),
            Some((1.6, PanelEmit::F32))
        );
        // Integer slots gesture as f32 and dispatch in their own family
        // (the fixture brightness fader edits a u32 slot).
        assert_eq!(
            numeric_value(&control(UiSlotValue::u32(7), editable.clone())),
            Some((7.0, PanelEmit::U32))
        );
        assert_eq!(
            numeric_value(&control(UiSlotValue::bool(true), editable.clone())),
            None
        );
        assert_eq!(
            bool_value(&control(UiSlotValue::bool(true), editable.clone())),
            Some(true)
        );
        assert_eq!(bool_value(&control(UiSlotValue::f32(1.0), editable)), None);
    }

    #[test]
    fn widget_value_agreement_gates_the_interactive_control() {
        let editable = UiSlotFieldState::editable();
        assert!(value_matches_widget(&control(
            UiSlotValue::f32(1.6),
            editable.clone()
        )));
        assert!(value_matches_widget(&control(
            UiSlotValue::u32(7),
            editable.clone()
        )));
        assert!(!value_matches_widget(&control(
            UiSlotValue::bool(true),
            editable
        )));
    }

    #[test]
    fn label_color_follows_the_slot_affordance_ladder() {
        let clean = control(UiSlotValue::f32(1.0), UiSlotFieldState::editable());
        assert!(panel_label_class(&clean).contains("subtle-foreground"));

        let unsaved = control(
            UiSlotValue::f32(1.0),
            UiSlotFieldState::editable().with_dirty(UiNodeDirtyState::Dirty),
        );
        assert!(panel_label_class(&unsaved).contains("warning"));

        let debug = control(
            UiSlotValue::f32(1.0),
            UiSlotFieldState::editable()
                .with_dirty(UiNodeDirtyState::Dirty)
                .with_debug(true),
        );
        assert!(panel_label_class(&debug).contains("live"));

        let failed = control(
            UiSlotValue::f32(1.0),
            UiSlotFieldState::editable().with_dirty(UiNodeDirtyState::Error),
        );
        assert!(panel_label_class(&failed).contains("error"));

        let bound = control_with_source(
            UiSlotValue::f32(1.0),
            UiSlotFieldState::editable(),
            bound_source(),
        );
        assert!(panel_label_class(&bound).contains("bound"));
    }

    #[test]
    fn live_readings_parse_back_into_widget_geometry() {
        let mut bound = control_with_source(
            UiSlotValue::f32(1.6),
            UiSlotFieldState::editable(),
            bound_source(),
        );
        bound.live_value = Some("2.72".to_string());
        assert_eq!(bound.live_numeric(), Some(2.72));
        assert_eq!(bound.live_bool(), None, "a number is not a pill state");
        // GV fix 3: the readout is that ONE number — the authored 1.6 it
        // displaces moved into the popup.
        assert_eq!(bound.shown_display(), "2.72");
        assert!(
            bound.detail_aspects().iter().any(|aspect| aspect
                .rows
                .iter()
                .any(|row| row.label == "Authored value" && row.value == "1.6")),
            "the authored value keeps a home behind the label"
        );

        bound.live_value = Some("true".to_string());
        assert_eq!(bound.live_bool(), Some(true));
        assert_eq!(bound.live_numeric(), None);

        bound.live_value = None;
        assert_eq!(bound.live_numeric(), None);
        assert_eq!(bound.shown_display(), "1.6");
    }

    #[test]
    fn phasor_readouts_auto_denominate_the_rate() {
        use super::phasor_speed_display;
        // The G2 examples verbatim: 2/s → 3/min → 15/hr.
        assert_eq!(phasor_speed_display("0.5"), "2/s");
        assert_eq!(phasor_speed_display("20"), "3/min");
        assert_eq!(phasor_speed_display("240"), "15/hr");
        // Plasma's 100 s: under one per minute, so it reads per hour.
        assert_eq!(phasor_speed_display("100"), "36/hr");
        assert_eq!(phasor_speed_display("45"), "1.3/min");
        // Period 0 is the frozen sentinel: it never cycles.
        assert_eq!(phasor_speed_display("0.0"), "0/s");
        // A non-numeric reading (product chip label, etc.) passes through.
        assert_eq!(phasor_speed_display("Time product"), "Time product");
    }

    #[test]
    fn edited_bound_label_shows_the_edit_color_over_violet() {
        // The dot used to carry "unsaved" beside a violet name; now the
        // label wears the edit color while the widget keeps the violet
        // family — no signal lost, one status voice per element.
        let edited_bound = control_with_source(
            UiSlotValue::f32(1.0),
            UiSlotFieldState::editable().with_dirty(UiNodeDirtyState::Dirty),
            bound_source(),
        );
        assert!(panel_label_class(&edited_bound).contains("warning"));
    }
}
