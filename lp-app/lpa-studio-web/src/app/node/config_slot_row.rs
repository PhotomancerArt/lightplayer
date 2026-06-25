//! Table row presentation for a single config slot.

use dioxus::prelude::*;
use lpa_studio_core::{UiConfigSlot, UiConfigSlotBody};

use crate::app::node::{
    DirtyMark, SlotIssueList, SlotRecordEditor, SlotSourceIndicator, SlotValueEditor,
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
    let mut issues = slot.issues.clone();
    if let Some(invalid) = slot.state.invalid.as_ref() {
        issues.push(invalid.clone());
    }
    let row_class = if index % 2 == 0 {
        "tw:grid tw:min-w-0 tw:grid-cols-[minmax(120px,0.4fr)_minmax(0,1fr)_24px] tw:items-center tw:gap-2 tw:border-t tw:border-border-muted tw:bg-card-muted tw:px-2 tw:py-1.5 first:tw:border-t-0"
    } else {
        "tw:grid tw:min-w-0 tw:grid-cols-[minmax(120px,0.4fr)_minmax(0,1fr)_24px] tw:items-center tw:gap-2 tw:border-t tw:border-border-muted tw:bg-card-subtle tw:px-2 tw:py-1.5 first:tw:border-t-0"
    };
    let indent = depth * 14;

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
                div { class: "tw:flex tw:min-w-0 tw:items-center tw:justify-end tw:gap-2 tw:text-sm tw:leading-tight tw:text-muted-foreground",
                    SlotBodyPreview { body: slot.body.clone(), state: slot.state.clone(), expanded: expanded() }
                    DirtyMark { dirty: slot.state.dirty }
                }
                SlotSourceIndicator { label: slot.label.clone(), source: slot.source.clone() }
                if !issues.is_empty() {
                    div { class: "tw:col-span-3 tw:min-w-0",
                        SlotIssueList { issues }
                    }
                }
            }
            if expanded() {
                if let Some(record) = child_record {
                    SlotRecordEditor {
                        record,
                        depth: depth + 1,
                    }
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn SlotBodyPreview(
    body: UiConfigSlotBody,
    state: lpa_studio_core::UiSlotFieldState,
    expanded: bool,
) -> Element {
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
