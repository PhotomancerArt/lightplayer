//! [`RowCta`]: a pick row's one primary verb — the press a project pick or
//! a board pick ends in (`device_pick_popover.rs`) — and [`RowCtaDisabled`],
//! the same verb waiting on a pick that has not resolved.
//!
//! Moved out of the retired device card, unchanged: the pickers are its
//! only users.

use dioxus::prelude::*;
use lpa_studio_core::UiAction;

use crate::core::{ActionButton, ActionButtonVariant};

/// The verb row's Primary: the standing spectrum ring every surface's one
/// primary verb wears, sized to the fixed 30px row. It reads its label and
/// its explanation off the action's own meta, so the action model stays the
/// single source for both.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn RowCta(action: UiAction, on_action: EventHandler<UiAction>) -> Element {
    // A Lasting verb (a flash over firmware, a push over a project the
    // library has no copy of) wears its level's look and arms on its own
    // button, as every Lasting verb does (D7): the quiet tinted chip.
    if action.meta().consequence.arms() {
        return rsx! {
            ActionButton { action, running: false, variant: ActionButtonVariant::Quiet, on_action }
        };
    }
    let meta = action.meta().clone();
    let dispatch = action.clone();

    rsx! {
        button {
            class: ROW_CTA_CLASS,
            r#type: "button",
            title: "{meta.summary}",
            onclick: move |_| on_action.call(dispatch.clone()),
            "{meta.label}"
        }
    }
}

/// The same verb with nothing behind it yet: the pick did not resolve, so
/// the button says what it is waiting for instead of guessing.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn RowCtaDisabled(label: String, hint: String) -> Element {
    rsx! {
        button {
            class: ROW_CTA_DISABLED_CLASS,
            r#type: "button",
            disabled: true,
            title: "{hint}",
            "{label}"
        }
    }
}

/// The verb row's Primary voice — the standing spectrum ring every
/// surface's one primary verb wears, at the row's height.
const ROW_CTA_CLASS: &str = "ux-spectrum-cta ux-focus-ring tw:inline-flex tw:h-[26px] tw:flex-none tw:cursor-pointer tw:items-center tw:rounded-md tw:border tw:border-transparent tw:bg-transparent tw:px-3 tw:text-xs tw:font-bold tw:text-strong-foreground tw:no-underline";

const ROW_CTA_DISABLED_CLASS: &str = "tw:inline-flex tw:h-[26px] tw:flex-none tw:cursor-not-allowed tw:items-center tw:rounded-md tw:border tw:border-border tw:bg-transparent tw:px-3 tw:text-xs tw:font-semibold tw:text-subtle-foreground tw:opacity-60";
