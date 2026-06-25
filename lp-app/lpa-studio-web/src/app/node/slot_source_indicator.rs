//! Source indicator menu for config slot values.

use dioxus::prelude::*;
use lpa_studio_core::UiSlotSourceState;

use crate::base::{IconMenuButton, IconMenuTone, PopoverPlacement, StudioIconName};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn SlotSourceIndicator(label: String, source: UiSlotSourceState) -> Element {
    let menu_label = format!("{label} source");
    let tone = match source {
        UiSlotSourceState::Bound(_) => IconMenuTone::Accent,
        UiSlotSourceState::Direct => IconMenuTone::Neutral,
        UiSlotSourceState::Unset => IconMenuTone::Warning,
    };
    let active = !matches!(source, UiSlotSourceState::Unset);

    rsx! {
        span { class: "tw:inline-flex tw:w-6 tw:justify-end",
            IconMenuButton {
                icon: source_icon(&source),
                icon_size: 13,
                label: menu_label.clone(),
                title: menu_label,
                tone,
                placement: PopoverPlacement::BottomStart,
                active,
                div { class: "tw:grid tw:gap-1",
                    span { class: "tw:text-[0.68rem] tw:font-bold tw:uppercase tw:text-heading", "slot source" }
                    strong { class: "tw:text-sm tw:text-strong-foreground", "{label}" }
                }
                match source {
                    UiSlotSourceState::Direct => rsx! {
                        p { class: "tw:m-0 tw:text-xs tw:text-muted-foreground", "Value is authored directly on this slot." }
                    },
                    UiSlotSourceState::Bound(endpoint) => rsx! {
                        div { class: "tw:grid tw:gap-1",
                            span { class: "tw:text-[0.68rem] tw:font-bold tw:uppercase tw:text-subtle-foreground", "binding" }
                            code { class: "tw:rounded-xs tw:border tw:border-border-subtle tw:bg-page tw:p-2 tw:font-mono tw:text-xs tw:text-muted-foreground tw:break-words", "{endpoint.label}" }
                            if let Some(detail) = endpoint.detail.as_ref() {
                                small { class: "tw:text-xs tw:text-subtle-foreground tw:break-words", "{detail}" }
                            }
                        }
                    },
                    UiSlotSourceState::Unset => rsx! {
                        p { class: "tw:m-0 tw:text-xs tw:text-status-warning-foreground", "No direct value or binding is set." }
                    },
                }
            }
        }
    }
}

fn source_icon(source: &UiSlotSourceState) -> StudioIconName {
    match source {
        UiSlotSourceState::Direct => StudioIconName::AssignedValue,
        UiSlotSourceState::Bound(_) => StudioIconName::BoundValue,
        UiSlotSourceState::Unset => StudioIconName::StatusIdle,
    }
}
