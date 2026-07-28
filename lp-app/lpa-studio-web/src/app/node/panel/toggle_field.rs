//! Pill toggle field for boolean panel controls.
//!
//! Good-green is worn ONLY by the on/valid state (Studio convention: green
//! means good/valid — never selection, never binding). A bound toggle keeps
//! its violet ring on the pill regardless of on/off. Click dispatches
//! `SlotEditOp::SetValue` with the flipped value.

use dioxus::prelude::*;
use lpa_studio_core::{LpValue, ProjectSlotAddress, UiAction, UiSlotFieldState};

use crate::app::node::slot_edit_actions::slot_set_value_action;
use crate::app::node::slot_fields::field_wiring;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ToggleField(
    value: bool,
    /// Live bus reading (display-only; P6 item 1): the pill renders this
    /// state — with its violet bound ring — while `value` (the authored
    /// default) stays what a click flips.
    #[props(default = None)]
    live_value: Option<bool>,
    state: UiSlotFieldState,
    /// Violet bound treatment on the pill ring.
    #[props(default = false)]
    bound: bool,
    #[props(default = None)] address: Option<ProjectSlotAddress>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let wired = field_wiring(&state, &address, on_action);
    let disabled = wired.is_none();
    let shown = live_value.unwrap_or(value);
    let pill_class = toggle_pill_class(shown, bound, disabled);
    let thumb_class = toggle_thumb_class(shown);
    let invalid_title = state.invalid.clone().unwrap_or_default();

    rsx! {
        button {
            class: "{pill_class}",
            r#type: "button",
            role: "switch",
            aria_checked: "{shown}",
            disabled,
            title: "{invalid_title}",
            onclick: move |event| {
                event.stop_propagation();
                if let Some((address, handler)) = wired.clone() {
                    handler.call(slot_set_value_action(address, LpValue::Bool(!value)));
                }
            },
            span { class: "{thumb_class}" }
        }
    }
}

/// Pill chrome: good-green surface strictly for the ON state; bound rings
/// violet in both states; disabled pills drop the pointer cursor.
fn toggle_pill_class(on: bool, bound: bool, disabled: bool) -> String {
    let surface = if on {
        "tw:border-status-good-border tw:bg-status-good-bg"
    } else {
        "tw:border-border-strong tw:bg-page"
    };
    let ring = if bound {
        " tw:ring-1 tw:ring-status-bound-border"
    } else {
        ""
    };
    let cursor = if disabled { "" } else { " tw:cursor-pointer" };
    format!(
        "tw:relative tw:inline-flex tw:h-[22px] tw:w-10 tw:flex-none tw:appearance-none \
         tw:items-center tw:rounded-full tw:border tw:p-0 {surface}{ring}{cursor}"
    )
}

/// Thumb dot: slides right and takes the good-green foreground when ON.
fn toggle_thumb_class(on: bool) -> &'static str {
    if on {
        "tw:absolute tw:left-[19px] tw:h-4 tw:w-4 tw:rounded-full tw:bg-status-good-foreground tw:transition-[left] tw:duration-100 tw:motion-reduce:transition-none"
    } else {
        "tw:absolute tw:left-[2px] tw:h-4 tw:w-4 tw:rounded-full tw:bg-subtle-foreground tw:transition-[left] tw:duration-100 tw:motion-reduce:transition-none"
    }
}

#[cfg(test)]
mod tests {
    use super::{toggle_pill_class, toggle_thumb_class};

    #[test]
    fn green_is_reserved_for_the_on_state() {
        assert!(toggle_pill_class(true, false, false).contains("status-good"));
        assert!(!toggle_pill_class(false, false, false).contains("status-good"));
        assert!(toggle_thumb_class(true).contains("status-good"));
        assert!(!toggle_thumb_class(false).contains("status-good"));
    }

    #[test]
    fn bound_ring_is_violet_in_both_states() {
        assert!(toggle_pill_class(true, true, false).contains("status-bound"));
        assert!(toggle_pill_class(false, true, false).contains("status-bound"));
        assert!(!toggle_pill_class(false, false, false).contains("status-bound"));
    }
}
