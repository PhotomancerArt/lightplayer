use dioxus::prelude::*;
use lpa_studio_core::{UiConsumedSlot, UiSlotSource};

use crate::app::node::DirtyMark;
use crate::base::{StudioIcon, StudioIconName};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ConsumedSlots(slots: Vec<UiConsumedSlot>) -> Element {
    rsx! {
        section { class: "tw:grid tw:min-w-0 tw:gap-2",
            h4 { class: "tw:m-0 tw:text-xs tw:font-bold tw:uppercase tw:leading-none tw:text-heading", "Consumed values" }
            div { class: "tw:grid tw:min-w-0 tw:gap-1.5",
                for slot in slots {
                    ConsumedSlotRow { slot, depth: 0 }
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ConsumedSlotRow(slot: UiConsumedSlot, depth: usize) -> Element {
    let source_label = source_label(&slot.source);
    let icon = source_icon(&slot.source);
    let indent = depth * 12;

    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:gap-1",
            div {
                class: "tw:grid tw:min-w-0 tw:grid-cols-[auto_minmax(86px,0.42fr)_minmax(0,1fr)_auto] tw:items-start tw:gap-2 tw:rounded-sm tw:border tw:border-border-subtle tw:bg-card-muted tw:p-2",
                style: "margin-left: {indent}px;",
                span { class: "tw:mt-0.5 tw:inline-flex tw:h-5 tw:w-5 tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-border-subtle tw:bg-page tw:text-muted-foreground",
                    StudioIcon { name: icon, size: 13 }
                }
                div { class: "tw:min-w-0",
                    div { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-1.5",
                        strong { class: "tw:min-w-0 tw:text-sm tw:leading-tight tw:text-strong-foreground tw:break-words", "{slot.label}" }
                        DirtyMark { dirty: slot.dirty }
                    }
                    if let Some(detail) = slot.detail.as_ref() {
                        small { class: "tw:block tw:text-xs tw:text-subtle-foreground tw:break-words", "{detail}" }
                    }
                }
                div { class: "tw:min-w-0 tw:text-sm tw:leading-tight tw:text-muted-foreground tw:break-words",
                    match &slot.source {
                        UiSlotSource::Bound(endpoint) => rsx! {
                            code { class: "tw:font-mono tw:text-xs tw:text-accent", "{endpoint.label}" }
                        },
                        UiSlotSource::Child(child) => rsx! {
                            code { class: "tw:font-mono tw:text-xs tw:text-muted-foreground", "{child}" }
                        },
                        UiSlotSource::Direct | UiSlotSource::Unset => rsx! {
                            if let Some(value) = slot.value.as_ref() {
                                "{value}"
                            } else {
                                span { class: "tw:text-subtle-foreground", "unset" }
                            }
                        },
                    }
                }
                span { class: "tw:rounded-xs tw:border tw:border-border-subtle tw:bg-page tw:px-1.5 tw:py-0.5 tw:text-[0.68rem] tw:font-bold tw:uppercase tw:text-subtle-foreground", "{source_label}" }
                if !slot.issues.is_empty() {
                    ul { class: "tw:col-span-4 tw:m-0 tw:grid tw:list-none tw:gap-1 tw:p-0",
                        for issue in slot.issues.clone() {
                            li { class: "tw:text-xs tw:text-status-error-foreground", "{issue}" }
                        }
                    }
                }
            }
            if !slot.children.is_empty() {
                for child in slot.children {
                    ConsumedSlotRow { slot: child, depth: depth + 1 }
                }
            }
        }
    }
}

fn source_icon(source: &UiSlotSource) -> StudioIconName {
    match source {
        UiSlotSource::Direct => StudioIconName::AssignedValue,
        UiSlotSource::Bound(_) => StudioIconName::BoundValue,
        UiSlotSource::Child(_) => StudioIconName::ChildValue,
        UiSlotSource::Unset => StudioIconName::StatusIdle,
    }
}

fn source_label(source: &UiSlotSource) -> &'static str {
    match source {
        UiSlotSource::Direct => "direct",
        UiSlotSource::Bound(_) => "bound",
        UiSlotSource::Child(_) => "child",
        UiSlotSource::Unset => "unset",
    }
}
