use dioxus::prelude::*;
use lpa_studio_core::{UiConsumedSlot, UiSlotSource};

use crate::app::node::DirtyMark;
use crate::base::{IconMenuButton, IconMenuTone, PopoverPlacement, StudioIcon, StudioIconName};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ConsumedSlots(slots: Vec<UiConsumedSlot>) -> Element {
    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:overflow-hidden",
            for (index, slot) in slots.into_iter().enumerate() {
                ConsumedSlotRow {
                    slot,
                    depth: 0,
                    index,
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ConsumedSlotRow(slot: UiConsumedSlot, depth: usize, index: usize) -> Element {
    let has_children = !slot.children.is_empty();
    let mut expanded = use_signal(|| depth > 0 || !has_children);
    let row_class = if index % 2 == 0 {
        "tw:grid tw:min-w-0 tw:grid-cols-[minmax(120px,0.4fr)_minmax(0,1fr)_24px] tw:items-center tw:gap-2 tw:border-t tw:border-border-muted tw:bg-card-muted tw:px-2 tw:py-1.5 first:tw:border-t-0"
    } else {
        "tw:grid tw:min-w-0 tw:grid-cols-[minmax(120px,0.4fr)_minmax(0,1fr)_24px] tw:items-center tw:gap-2 tw:border-t tw:border-border-muted tw:bg-card-subtle tw:px-2 tw:py-1.5 first:tw:border-t-0"
    };
    let indent = depth * 14;
    let children = slot.children.clone();

    rsx! {
        div { class: "tw:grid tw:min-w-0",
            div { class: row_class,
                div { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-1.5", style: "padding-left: {indent}px;",
                    if has_children {
                        button {
                            class: "tw:inline-flex tw:h-5 tw:w-5 tw:flex-none tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-border-subtle tw:bg-page tw:p-0 tw:text-subtle-foreground tw:hover:border-border-strong",
                            r#type: "button",
                            aria_label: if expanded() { "Collapse slot" } else { "Expand slot" },
                            title: if expanded() { "Collapse slot" } else { "Expand slot" },
                            onclick: move |_| expanded.set(!expanded()),
                            StudioIcon {
                                name: if expanded() { StudioIconName::Expanded } else { StudioIconName::Collapsed },
                                size: 12,
                            }
                        }
                    } else {
                        span { class: "tw:h-5 tw:w-5 tw:flex-none" }
                    }
                    div { class: "tw:min-w-0",
                        strong { class: "tw:block tw:min-w-0 tw:text-sm tw:leading-tight tw:text-strong-foreground tw:break-words", "{slot.label}" }
                        if let Some(detail) = slot.detail.as_ref() {
                            small { class: "tw:block tw:text-xs tw:text-subtle-foreground tw:break-words", "{detail}" }
                        }
                    }
                }
                div { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-2 tw:text-sm tw:leading-tight tw:text-muted-foreground",
                    span { class: "tw:min-w-0 tw:break-words",
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
                    DirtyMark { dirty: slot.dirty }
                }
                SlotSourceMenu { slot: slot.clone() }
                if !slot.issues.is_empty() {
                    ul { class: "tw:col-span-3 tw:m-0 tw:grid tw:list-none tw:gap-1 tw:p-0",
                        for issue in slot.issues.clone() {
                            li { class: "tw:text-xs tw:text-status-error-foreground", "{issue}" }
                        }
                    }
                }
            }
            if expanded() {
                for (child_index, child) in children.into_iter().enumerate() {
                    ConsumedSlotRow {
                        slot: child,
                        depth: depth + 1,
                        index: child_index,
                    }
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn SlotSourceMenu(slot: UiConsumedSlot) -> Element {
    let source = slot.source.clone();
    let label = format!("{} source", slot.label);
    let tone = match source {
        UiSlotSource::Bound(_) => IconMenuTone::Accent,
        UiSlotSource::Child(_) => IconMenuTone::Neutral,
        UiSlotSource::Direct => IconMenuTone::Neutral,
        UiSlotSource::Unset => IconMenuTone::Warning,
    };
    let active = !matches!(source, UiSlotSource::Unset);

    rsx! {
        div { class: "tw:mt-0.5 tw:w-6",
            IconMenuButton {
                icon: source_icon(&source),
                icon_size: 13,
                label: label.clone(),
                title: label,
                tone,
                placement: PopoverPlacement::BottomStart,
                active,
                div { class: "tw:grid tw:gap-1",
                    span { class: "tw:text-[0.68rem] tw:font-bold tw:uppercase tw:text-heading", "consumed slot" }
                    strong { class: "tw:text-sm tw:text-strong-foreground", "{slot.label}" }
                    if let Some(detail) = slot.detail.as_ref() {
                        small { class: "tw:text-xs tw:text-subtle-foreground tw:break-words", "{detail}" }
                    }
                }
                match source {
                    UiSlotSource::Direct => rsx! {
                        p { class: "tw:m-0 tw:text-xs tw:text-muted-foreground tw:break-words",
                            "assigned value "
                            if let Some(value) = slot.value.as_ref() {
                                code { class: "tw:font-mono", "{value}" }
                            } else {
                                code { class: "tw:font-mono", "unit" }
                            }
                        }
                    },
                    UiSlotSource::Bound(endpoint) => rsx! {
                        div { class: "tw:grid tw:gap-1",
                            span { class: "tw:text-[0.68rem] tw:font-bold tw:uppercase tw:text-subtle-foreground", "source binding" }
                            code { class: "tw:rounded-xs tw:border tw:border-border-subtle tw:bg-page tw:p-2 tw:font-mono tw:text-xs tw:text-muted-foreground tw:break-words", "{endpoint.label}" }
                            if let Some(detail) = endpoint.detail.as_ref() {
                                small { class: "tw:text-xs tw:text-subtle-foreground tw:break-words", "{detail}" }
                            }
                        }
                    },
                    UiSlotSource::Child(child) => rsx! {
                        div { class: "tw:grid tw:gap-1",
                            span { class: "tw:text-[0.68rem] tw:font-bold tw:uppercase tw:text-subtle-foreground", "child node" }
                            code { class: "tw:rounded-xs tw:border tw:border-border-subtle tw:bg-page tw:p-2 tw:font-mono tw:text-xs tw:text-muted-foreground tw:break-words", "{child}" }
                        }
                    },
                    UiSlotSource::Unset => rsx! {
                        p { class: "tw:m-0 tw:text-xs tw:text-status-warning-foreground", "No assigned value or source binding." }
                    },
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
