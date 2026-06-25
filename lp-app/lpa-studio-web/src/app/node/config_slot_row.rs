//! Table row presentation for a single config slot.

use dioxus::prelude::*;
use lpa_studio_core::{UiConfigSlot, UiConfigSlotBody, UiSlotFieldState};

use crate::app::node::{
    SlotAspectMenu, SlotRecordEditor, SlotValueEditor, primary_affordance, slot_row_class,
};
use crate::base::{StudioIcon, StudioIconName};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ConfigSlotRow(slot: UiConfigSlot, depth: usize, index: usize) -> Element {
    let child_record = match &slot.body {
        UiConfigSlotBody::Record(record) if !record.fields.is_empty() => Some(record.clone()),
        _ => None,
    };
    let has_children = child_record.is_some();
    let mut expanded = use_signal(|| depth > 0 || !has_children);
    let aspects = slot.visible_aspects();
    let primary = primary_affordance(&aspects);
    let row_class = slot_row_class(primary, index);
    let indent = depth * 14;

    rsx! {
        div { class: "tw:grid tw:min-w-0",
            div { class: row_class,
                div { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-1.5", style: "padding-left: {indent}px;",
                    if has_children {
                        button {
                            class: "tw:inline-flex tw:h-6 tw:w-6 tw:flex-none tw:appearance-none tw:items-center tw:justify-center tw:rounded-xs tw:border-0 tw:bg-transparent tw:p-0 tw:text-muted-foreground tw:transition-colors tw:hover:text-strong-foreground tw:focus-visible:outline tw:focus-visible:outline-1 tw:focus-visible:outline-border-strong",
                            style: "appearance: none; -webkit-appearance: none; border: 0; background: transparent; cursor: pointer;",
                            r#type: "button",
                            aria_label: if expanded() { "Collapse slot" } else { "Expand slot" },
                            title: if expanded() { "Collapse slot" } else { "Expand slot" },
                            onclick: move |_| expanded.set(!expanded()),
                            span { class: expand_chevron_class(expanded()),
                                style: "stroke-width: 3;",
                                StudioIcon {
                                    name: StudioIconName::Collapsed,
                                    size: 16,
                                }
                            }
                        }
                    } else {
                        span { class: "tw:h-6 tw:w-6 tw:flex-none" }
                    }
                    div { class: "tw:min-w-0",
                        strong { class: "tw:block tw:min-w-0 tw:text-sm tw:leading-tight tw:text-strong-foreground tw:break-words", "{slot.label}" }
                        if let Some(detail) = slot.detail.as_ref() {
                            small { class: "tw:block tw:text-xs tw:text-subtle-foreground tw:break-words", "{detail}" }
                        }
                    }
                }
                div { class: "tw:flex tw:min-w-0 tw:items-center tw:justify-end tw:gap-2 tw:text-sm tw:leading-tight tw:text-muted-foreground",
                    SlotBodyPreview { body: slot.body.clone(), state: slot.state.clone(), expanded: expanded() }
                }
                SlotAspectMenu {
                    label: slot.label.clone(),
                    aspects,
                }
            }
            if expanded() {
                if let Some(record) = child_record {
                    SlotRecordEditor {
                        record,
                        depth: depth + 1,
                        separated: true,
                    }
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn SlotBodyPreview(body: UiConfigSlotBody, state: UiSlotFieldState, expanded: bool) -> Element {
    match body {
        UiConfigSlotBody::Empty => rsx! {
            span { class: "tw:text-subtle-foreground", "unset" }
        },
        UiConfigSlotBody::Value(value) => rsx! {
            SlotValueEditor { value, state }
        },
        UiConfigSlotBody::Record(record) => {
            let label = if record.fields.len() == 1 {
                "1 field".to_string()
            } else {
                format!("{} fields", record.fields.len())
            };
            rsx! {
                span { class: record_summary_class(expanded), "{label}" }
            }
        }
    }
}

fn record_summary_class(expanded: bool) -> &'static str {
    if expanded {
        "tw:text-xs tw:font-bold tw:uppercase tw:text-subtle-foreground"
    } else {
        "tw:text-xs tw:font-bold tw:uppercase tw:text-muted-foreground"
    }
}

fn expand_chevron_class(expanded: bool) -> &'static str {
    if expanded {
        "tw:inline-flex tw:rotate-90 tw:transition-transform"
    } else {
        "tw:inline-flex tw:transition-transform"
    }
}
