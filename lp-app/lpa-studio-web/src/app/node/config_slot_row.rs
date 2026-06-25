//! Table row presentation for a single config slot.

use dioxus::prelude::*;
use lpa_studio_core::{UiConfigSlot, UiConfigSlotBody, UiNodeDirtyState, UiSlotFieldState};

use crate::app::node::{SlotRecordEditor, SlotSourceIndicator, SlotValueEditor};
use crate::base::{IconMenuButton, IconMenuTone, PopoverPlacement, StudioIcon, StudioIconName};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ConfigSlotRow(slot: UiConfigSlot, depth: usize, index: usize) -> Element {
    let child_record = match &slot.body {
        UiConfigSlotBody::Record(record) if !record.fields.is_empty() => Some(record.clone()),
        _ => None,
    };
    let has_children = child_record.is_some();
    let mut expanded = use_signal(|| depth > 0 || !has_children);
    let row_class = row_class(&slot.state, !slot.issues.is_empty(), index);
    let indent = depth * 14;

    rsx! {
        div { class: "tw:grid tw:min-w-0",
            div { class: row_class,
                div { class: "tw:flex tw:min-w-0 tw:items-center", style: "padding-left: {indent}px;",
                    if has_children {
                        button {
                            class: expand_button_class(&slot.state, !slot.issues.is_empty()),
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
                        SlotStateIndicator {
                            label: slot.label.clone(),
                            state: slot.state.clone(),
                            issues: slot.issues.clone(),
                        }
                    }
                }
                div { class: "tw:min-w-0",
                    strong { class: "tw:block tw:min-w-0 tw:text-sm tw:leading-tight tw:text-strong-foreground tw:break-words", "{slot.label}" }
                    if let Some(detail) = slot.detail.as_ref() {
                        small { class: "tw:block tw:text-xs tw:text-subtle-foreground tw:break-words", "{detail}" }
                    }
                }
                div { class: "tw:flex tw:min-w-0 tw:items-center tw:justify-end tw:gap-2 tw:text-sm tw:leading-tight tw:text-muted-foreground",
                    SlotBodyPreview { body: slot.body.clone(), state: slot.state.clone(), expanded: expanded() }
                }
                SlotSourceIndicator { label: slot.label.clone(), source: slot.source.clone() }
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
fn SlotStateIndicator(label: String, state: UiSlotFieldState, issues: Vec<String>) -> Element {
    if !state.needs_attention() && issues.is_empty() {
        return rsx! {
            span { class: "tw:h-6 tw:w-6 tw:flex-none" }
        };
    }

    let title = format!("{label} state");
    let tone = slot_state_tone(&state, !issues.is_empty());
    let icon = slot_state_icon(&state, !issues.is_empty());

    rsx! {
        span { class: "tw:inline-flex tw:w-6 tw:justify-start",
            IconMenuButton {
                icon,
                icon_size: 13,
                label: title.clone(),
                title,
                tone,
                placement: PopoverPlacement::BottomStart,
                active: true,
                div { class: "tw:grid tw:gap-1",
                    span { class: "tw:text-[0.68rem] tw:font-bold tw:uppercase tw:text-heading", "slot state" }
                    strong { class: "tw:text-sm tw:text-strong-foreground", "{label}" }
                }
                if let Some(invalid) = state.invalid.as_ref() {
                    div { class: "tw:grid tw:gap-1",
                        span { class: "tw:text-[0.68rem] tw:font-bold tw:uppercase tw:text-status-error-foreground", "invalid" }
                        p { class: "tw:m-0 tw:text-xs tw:text-status-error-foreground", "{invalid}" }
                    }
                }
                if !issues.is_empty() {
                    div { class: "tw:grid tw:gap-1",
                        span { class: "tw:text-[0.68rem] tw:font-bold tw:uppercase tw:text-status-error-foreground", "issues" }
                        ul { class: "tw:m-0 tw:grid tw:list-none tw:gap-1 tw:p-0",
                            for issue in issues {
                                li { class: "tw:text-xs tw:text-status-error-foreground", "{issue}" }
                            }
                        }
                    }
                }
                if state.dirty.needs_attention() {
                    div { class: "tw:grid tw:gap-1",
                        span { class: dirty_detail_label_class(state.dirty), "{dirty_detail_label(state.dirty)}" }
                        p { class: "tw:m-0 tw:text-xs tw:text-muted-foreground", "{dirty_detail_text(state.dirty)}" }
                    }
                }
            }
        }
    }
}

fn row_class(state: &UiSlotFieldState, has_issues: bool, index: usize) -> &'static str {
    if has_issues || state.invalid.is_some() || state.dirty == UiNodeDirtyState::Error {
        "tw:grid tw:min-w-0 tw:grid-cols-[24px_minmax(96px,0.38fr)_minmax(0,1fr)_24px] tw:items-center tw:gap-2 tw:bg-status-error-bg tw:px-2 tw:py-1.5"
    } else if state.dirty == UiNodeDirtyState::Dirty {
        "tw:grid tw:min-w-0 tw:grid-cols-[24px_minmax(96px,0.38fr)_minmax(0,1fr)_24px] tw:items-center tw:gap-2 tw:bg-[linear-gradient(90deg,var(--studio-status-warning-bg),transparent_74%)] tw:px-2 tw:py-1.5"
    } else if state.dirty == UiNodeDirtyState::Saving {
        "tw:grid tw:min-w-0 tw:grid-cols-[24px_minmax(96px,0.38fr)_minmax(0,1fr)_24px] tw:items-center tw:gap-2 tw:bg-[linear-gradient(90deg,var(--studio-status-working-bg),transparent_74%)] tw:px-2 tw:py-1.5"
    } else if index % 2 == 0 {
        "tw:grid tw:min-w-0 tw:grid-cols-[24px_minmax(96px,0.38fr)_minmax(0,1fr)_24px] tw:items-center tw:gap-2 tw:bg-card-muted tw:px-2 tw:py-1.5"
    } else {
        "tw:grid tw:min-w-0 tw:grid-cols-[24px_minmax(96px,0.38fr)_minmax(0,1fr)_24px] tw:items-center tw:gap-2 tw:bg-card-subtle tw:px-2 tw:py-1.5"
    }
}

fn expand_button_class(state: &UiSlotFieldState, has_issues: bool) -> &'static str {
    if has_issues || state.invalid.is_some() || state.dirty == UiNodeDirtyState::Error {
        "tw:inline-flex tw:h-6 tw:w-6 tw:flex-none tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-status-error-border tw:bg-status-error-bg tw:p-0 tw:text-status-error-foreground"
    } else if state.dirty == UiNodeDirtyState::Dirty {
        "tw:inline-flex tw:h-6 tw:w-6 tw:flex-none tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-status-warning-border tw:bg-status-warning-bg tw:p-0 tw:text-status-warning-foreground"
    } else if state.dirty == UiNodeDirtyState::Saving {
        "tw:inline-flex tw:h-6 tw:w-6 tw:flex-none tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-status-working-border tw:bg-status-working-bg tw:p-0 tw:text-status-working-foreground"
    } else {
        "tw:inline-flex tw:h-6 tw:w-6 tw:flex-none tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-border-subtle tw:bg-page tw:p-0 tw:text-subtle-foreground tw:hover:border-border-strong"
    }
}

fn slot_state_tone(state: &UiSlotFieldState, has_issues: bool) -> IconMenuTone {
    if has_issues || state.invalid.is_some() || state.dirty == UiNodeDirtyState::Error {
        IconMenuTone::Error
    } else if state.dirty == UiNodeDirtyState::Saving {
        IconMenuTone::Working
    } else {
        IconMenuTone::Warning
    }
}

fn slot_state_icon(state: &UiSlotFieldState, has_issues: bool) -> StudioIconName {
    if has_issues || state.invalid.is_some() {
        StudioIconName::StepAttention
    } else {
        match state.dirty {
            UiNodeDirtyState::Clean => StudioIconName::Edited,
            UiNodeDirtyState::Dirty => StudioIconName::Edited,
            UiNodeDirtyState::Saving => StudioIconName::StatusRunning,
            UiNodeDirtyState::Error => StudioIconName::StatusError,
        }
    }
}

fn dirty_detail_label(dirty: UiNodeDirtyState) -> &'static str {
    match dirty {
        UiNodeDirtyState::Clean => "clean",
        UiNodeDirtyState::Dirty => "edited",
        UiNodeDirtyState::Saving => "saving",
        UiNodeDirtyState::Error => "write failed",
    }
}

fn dirty_detail_label_class(dirty: UiNodeDirtyState) -> &'static str {
    match dirty {
        UiNodeDirtyState::Clean => {
            "tw:text-[0.68rem] tw:font-bold tw:uppercase tw:text-subtle-foreground"
        }
        UiNodeDirtyState::Dirty => {
            "tw:text-[0.68rem] tw:font-bold tw:uppercase tw:text-status-warning-foreground"
        }
        UiNodeDirtyState::Saving => {
            "tw:text-[0.68rem] tw:font-bold tw:uppercase tw:text-status-working-foreground"
        }
        UiNodeDirtyState::Error => {
            "tw:text-[0.68rem] tw:font-bold tw:uppercase tw:text-status-error-foreground"
        }
    }
}

fn dirty_detail_text(dirty: UiNodeDirtyState) -> &'static str {
    match dirty {
        UiNodeDirtyState::Clean => "Value is in sync with the project.",
        UiNodeDirtyState::Dirty => "Value has local edits that are not saved yet.",
        UiNodeDirtyState::Saving => "Value is being written or refreshed.",
        UiNodeDirtyState::Error => "The last write failed and the edited value is still preserved.",
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
