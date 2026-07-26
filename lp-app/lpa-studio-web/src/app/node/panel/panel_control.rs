//! Shared chrome for one node-face panel control.
//!
//! Renders a [`lpa_studio_core::UiPanelControl`]: the widget (knob, fader,
//! or toggle) with its name and value readout, the edit-state dot (warning
//! for unsaved, live-blue for transient, red for a failed write), and the
//! hover-revealed corner ⓘ opening the SAME [`SlotDetailButton`] popover as
//! the backing slot row — identical aspect sections, no forked content.
//!
//! The control IS the popover trigger (P2c item 3): the ⓘ is only the
//! click affordance hint, and opening merges the CONTROL's outline with the
//! aspect card below through the contiguous-outline system
//! (`base/outline.rs` + `base/popover.rs` anchored mode) — one contiguous
//! shape, "diving into the control". While open, the top layer paints a
//! live copy of the same [`PanelControlBody`] over the control, so nothing
//! is duplicated inside the panel.

use std::sync::atomic::{AtomicUsize, Ordering};

use dioxus::prelude::*;
use lpa_studio_core::{
    UiAction, UiNodeDirtyState, UiPanelControl as UiPanelControlData, UiPanelWidget,
    UiSlotValueKind,
};

use crate::app::node::{SlotDetailButton, SlotUnitSuffix};
use crate::base::PopoverPlacement;

use super::{HFaderField, KnobField, PanelEmit, ToggleField};

static NEXT_PANEL_CONTROL_ID: AtomicUsize = AtomicUsize::new(1);

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn PanelControl(
    control: UiPanelControlData,
    /// Open the corner ⓘ popover on first render and keep the trigger
    /// visible without hover (stories).
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
        UiPanelWidget::Fader { .. } => "tw:group tw:relative tw:grid tw:min-w-0 tw:gap-1.5",
        UiPanelWidget::Knob { .. } | UiPanelWidget::Toggle => {
            "tw:group tw:relative tw:flex tw:min-w-[52px] tw:flex-none tw:flex-col tw:items-center tw:gap-1"
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
    let detail_class = if detail_initially_open {
        "tw:visible tw:absolute tw:-right-2 tw:-top-2 tw:z-10"
    } else {
        "tw:invisible tw:absolute tw:-right-2 tw:-top-2 tw:z-10 tw:group-hover:visible tw:focus-within:visible"
    };
    let anchor_control = control.clone();

    rsx! {
        div { id: "{anchor_id}", class: outer_class,
            span { class: detail_class,
                SlotDetailButton {
                    label: control.label.clone(),
                    aspects: control.aspects.clone(),
                    initially_open: detail_initially_open,
                    placement: PopoverPlacement::BottomMiddle,
                    anchor_id: Some(anchor_id.clone()),
                    anchor_visual: rsx! {
                        div { class: anchor_visual_class,
                            PanelControlBody { control: anchor_control, on_action }
                        }
                    },
                    on_action,
                }
            }
            PanelControlBody { control, on_action }
        }
    }
}

/// The control itself — widget, name, and readout — without the corner ⓘ
/// chrome. Rendered on the card face AND as the popover's top-layer anchor
/// visual (P2c item 3); the fragment adopts the caller's column/grid
/// layout.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PanelControlBody(
    control: UiPanelControlData,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let bound = control.bound();
    let name_class = if bound {
        "tw:text-[0.66rem] tw:font-bold tw:uppercase tw:tracking-[0.08em] tw:text-status-bound-foreground"
    } else {
        "tw:text-[0.66rem] tw:font-bold tw:uppercase tw:tracking-[0.08em] tw:text-subtle-foreground"
    };
    let readout = rsx! {
        span { class: "tw:inline-flex tw:items-baseline tw:gap-1 tw:font-mono tw:text-[0.7rem] tw:text-muted-foreground",
            span { "{control.value.display}" }
            SlotUnitSuffix { unit: control.unit.clone(), reserve: false }
            if let Some((dot_class, dot_title)) = edit_state_dot(&control) {
                span {
                    class: "{dot_class} tw:h-1.5 tw:w-1.5 tw:self-center tw:rounded-full",
                    title: "{dot_title}",
                }
            }
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
                    min,
                    max,
                    step,
                    state: control.state.clone(),
                    bound,
                    address: control.address.clone(),
                    emit,
                    on_action,
                }
                span { class: name_class, "{control.label}" }
                {readout}
            }
        }
        UiPanelWidget::Fader { min, max, step } => {
            let Some((value, emit)) = numeric_value(&control) else {
                return mismatch_fallback(&control);
            };
            rsx! {
                div { class: "tw:flex tw:items-baseline tw:justify-between tw:gap-2",
                    span { class: name_class, "{control.label}" }
                    {readout}
                }
                HFaderField {
                    value,
                    min,
                    max,
                    step,
                    state: control.state.clone(),
                    bound,
                    address: control.address.clone(),
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
                        state: control.state.clone(),
                        bound,
                        address: control.address.clone(),
                        on_action,
                    }
                }
                span { class: name_class, "{control.label}" }
                {readout}
            }
        }
    }
}

/// Whether the widget family and the value family agree — disagreement
/// falls back to the read-only display with no gesture surface or ⓘ.
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
    PanelEmit::for_value(&control.value.kind)
}

/// Boolean payload for toggle widgets.
fn bool_value(control: &UiPanelControlData) -> Option<bool> {
    match control.value.kind {
        UiSlotValueKind::Bool(value) => Some(value),
        _ => None,
    }
}

/// Read-only fallback when the widget family and value family disagree —
/// the label plus the slot's own display string, no gesture surface.
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

/// The edit-state dot beside the readout: live-blue for transient edits,
/// warning for unsaved persisted edits, working while saving, red for a
/// failed write. `None` when clean.
fn edit_state_dot(control: &UiPanelControlData) -> Option<(&'static str, &'static str)> {
    match control.state.dirty {
        UiNodeDirtyState::Clean => None,
        UiNodeDirtyState::Dirty if control.state.live => Some((
            "tw:bg-status-live-foreground",
            "Live edit: applied to the running project, not written by Save",
        )),
        UiNodeDirtyState::Dirty => Some(("tw:bg-status-warning-foreground", "Unsaved edit")),
        UiNodeDirtyState::Saving => Some(("tw:bg-status-working-foreground", "Saving")),
        UiNodeDirtyState::Error => Some(("tw:bg-status-error-foreground", "Write failed")),
    }
}

#[cfg(test)]
mod tests {
    use lpa_studio_core::{
        UiNodeDirtyState, UiPanelControl, UiPanelWidget, UiSlotFieldState, UiSlotValue,
    };

    use super::{PanelEmit, bool_value, edit_state_dot, numeric_value, value_matches_widget};

    fn control(value: UiSlotValue, state: UiSlotFieldState) -> UiPanelControl {
        UiPanelControl {
            label: "speed".to_string(),
            address: None,
            widget: UiPanelWidget::Knob {
                min: 0.0,
                max: 4.0,
                step: None,
            },
            value,
            unit: None,
            state,
            aspects: Vec::new(),
        }
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
    fn edit_state_dot_follows_the_slot_row_families() {
        let clean = control(UiSlotValue::f32(1.0), UiSlotFieldState::editable());
        assert!(edit_state_dot(&clean).is_none());

        let unsaved = control(
            UiSlotValue::f32(1.0),
            UiSlotFieldState::editable().with_dirty(UiNodeDirtyState::Dirty),
        );
        assert!(edit_state_dot(&unsaved).unwrap().0.contains("warning"));

        let live = control(
            UiSlotValue::f32(1.0),
            UiSlotFieldState::editable()
                .with_dirty(UiNodeDirtyState::Dirty)
                .with_live(true),
        );
        assert!(edit_state_dot(&live).unwrap().0.contains("live"));

        let failed = control(
            UiSlotValue::f32(1.0),
            UiSlotFieldState::editable().with_dirty(UiNodeDirtyState::Error),
        );
        assert!(edit_state_dot(&failed).unwrap().0.contains("error"));
    }
}
