//! Table row presentation for a single config slot.

use dioxus::prelude::*;
use lpa_studio_core::{
    ProjectSlotAddress, UiAction, UiConfigSlot, UiConfigSlotBody, UiNodeDirtyState,
    UiSlotFieldState, UiSlotOptionality,
};

use crate::app::node::{
    SlotDetailButton, SlotDetailRevert, SlotRecordEditor, SlotValueEditor, primary_affordance,
    slot_row_class,
};
use crate::base::{StudioIcon, StudioIconName};

/// Edit chrome for a touched slot row: persisted edits show as "unsaved"
/// (amber badge + warning tint, counts toward Save), transient edits as
/// "live" (blue tint only, applied to the running project and never written
/// by Save). The per-slot Revert/Reset affordance lives in the slot detail
/// popup, not on the row.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SlotEditChrome {
    Unsaved,
    Live,
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ConfigSlotRow(
    slot: UiConfigSlot,
    depth: usize,
    index: usize,
    #[props(default = false)] initially_open: bool,
    #[props(default = None)] initially_expanded: Option<bool>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let child_record = match &slot.body {
        UiConfigSlotBody::Record(record) if !record.fields.is_empty() => Some(record.clone()),
        _ => None,
    };
    let child_asset = match &slot.body {
        UiConfigSlotBody::Asset(asset) => Some(asset.clone()),
        _ => None,
    };
    let has_children = child_record.is_some() || child_asset.is_some();
    let mut expanded = use_signal(|| initially_expanded.unwrap_or(depth > 0 || !has_children));
    let aspects = slot.visible_aspects();
    let primary = primary_affordance(&aspects);
    let chrome = slot_edit_chrome(&slot.state);
    let row_class = match chrome {
        Some(SlotEditChrome::Live) if slot.state.invalid.is_none() => live_row_class(),
        _ => slot_row_class(primary, index),
    };
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
                        strong { class: "tw:block tw:min-w-0 tw:text-sm tw:font-semibold tw:leading-tight tw:text-strong-foreground tw:break-words", "{slot.label}" }
                    }
                }
                div { class: "tw:flex tw:min-w-0 tw:items-center tw:justify-end tw:gap-2 tw:text-sm tw:leading-tight tw:text-muted-foreground",
                    if chrome == Some(SlotEditChrome::Unsaved) {
                        UnsavedBadge {}
                    }
                    if let Some(optionality) = slot.optionality {
                        OptionalSlotToggle { optionality }
                    }
                    SlotBodyPreview {
                        body: slot.body.clone(),
                        state: slot.state.clone(),
                        expanded: expanded(),
                        address: slot.address.clone(),
                        on_action,
                    }
                }
                SlotDetailButton {
                    label: slot.label.clone(),
                    aspects,
                    initially_open,
                    revert: slot_detail_revert(chrome, slot.address.clone(), on_action),
                }
            }
            if expanded() {
                if let Some(record) = child_record {
                    SlotRecordEditor {
                        record,
                        depth: depth + 1,
                        separated: true,
                        on_action,
                    }
                }
                if let Some(asset) = child_asset {
                    AssetSlotEditor { asset }
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn OptionalSlotToggle(optionality: UiSlotOptionality) -> Element {
    let title = if optionality.included {
        "Optional value enabled"
    } else {
        "Optional value disabled"
    };
    rsx! {
        label { class: "ux-slot-optional-toggle", title,
            input {
                class: "ux-slot-optional-toggle-input",
                r#type: "checkbox",
                checked: optionality.included,
                disabled: !optionality.can_toggle,
                aria_label: title,
            }
            span { class: "ux-slot-optional-toggle-track",
                span { class: "ux-slot-optional-toggle-thumb" }
            }
            span { class: "ux-slot-optional-toggle-label", "enabled" }
        }
    }
}

/// Compact "unsaved" pill marking a touched persisted slot row. Live rows
/// carry no badge: their tint plus the detail icon are the whole treatment.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn UnsavedBadge() -> Element {
    rsx! {
        span {
            class: "tw:flex-none tw:rounded-pill tw:border tw:border-status-warning-border tw:bg-status-warning-bg tw:px-1.5 tw:py-0.5 tw:text-[0.64rem] tw:font-bold tw:uppercase tw:text-status-warning-foreground",
            title: "Pending edit; Save writes it to the project files",
            "unsaved"
        }
    }
}

/// The detail-popup revert affordance for a touched slot: "Revert" for
/// unsaved (persisted) edits, "Reset" for live (transient) controls.
fn slot_detail_revert(
    chrome: Option<SlotEditChrome>,
    address: Option<ProjectSlotAddress>,
    on_action: Option<EventHandler<UiAction>>,
) -> Option<SlotDetailRevert> {
    let (label, title) = match chrome? {
        SlotEditChrome::Unsaved => ("Revert", "Discard this pending edit"),
        SlotEditChrome::Live => ("Reset", "Reset this live control to its authored value"),
    };
    Some(SlotDetailRevert {
        label,
        title,
        address: address?,
        on_action: on_action?,
    })
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn SlotBodyPreview(
    body: UiConfigSlotBody,
    state: UiSlotFieldState,
    expanded: bool,
    #[props(default = None)] address: Option<ProjectSlotAddress>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    match body {
        UiConfigSlotBody::Empty => rsx! {
            span { class: "tw:text-subtle-foreground", "unset" }
        },
        UiConfigSlotBody::Value(value) => rsx! {
            SlotValueEditor { value, state, address, on_action }
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
        UiConfigSlotBody::Asset(asset) => rsx! {
            code { class: "tw:min-w-0 tw:truncate tw:font-mono tw:text-xs tw:text-muted-foreground", "{asset.source}" }
        },
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn AssetSlotEditor(asset: lpa_studio_core::UiSlotAsset) -> Element {
    rsx! {
        div { class: "tw:border-t tw:border-border-muted tw:bg-page tw:px-2 tw:py-2",
            div { class: "tw:mb-1.5 tw:flex tw:min-w-0 tw:items-center tw:justify-between tw:gap-2",
                code { class: "tw:min-w-0 tw:truncate tw:font-mono tw:text-xs tw:text-subtle-foreground", "{asset.source}" }
                span { class: "tw:flex-none tw:text-xs tw:font-bold tw:text-subtle-foreground", "{asset.editor_label()}" }
            }
            if let Some(detail) = asset.detail.as_ref() {
                p { class: "tw:m-0 tw:mb-1.5 tw:text-xs tw:text-subtle-foreground tw:break-words", "{detail}" }
            }
            if let Some(content) = asset.content {
                pre { class: "tw:m-0 tw:max-h-48 tw:overflow-auto tw:rounded-xs tw:border tw:border-border-subtle tw:bg-terminal tw:p-3 tw:font-mono tw:text-xs tw:leading-normal tw:text-muted-foreground",
                    code { "{content}" }
                }
            } else {
                pre { class: "tw:m-0 tw:rounded-xs tw:border tw:border-border-subtle tw:bg-terminal tw:p-3 tw:font-mono tw:text-xs tw:leading-normal tw:text-subtle-foreground",
                    code { "// asset content not loaded" }
                }
            }
        }
    }
}

/// The edit chrome for a row, when its slot has been touched.
fn slot_edit_chrome(state: &UiSlotFieldState) -> Option<SlotEditChrome> {
    if state.dirty == UiNodeDirtyState::Clean {
        return None;
    }
    Some(if state.live {
        SlotEditChrome::Live
    } else {
        SlotEditChrome::Unsaved
    })
}

/// Row treatment for live-dirty rows: the dedicated live (blue) tint keeps a
/// touched runtime control distinct from both the warning-tinted unsaved
/// (persisted) rows and the good/success (green) treatments.
fn live_row_class() -> &'static str {
    "tw:grid tw:min-w-0 tw:grid-cols-[minmax(120px,0.4fr)_minmax(0,1fr)_32px] tw:items-center tw:gap-2 tw:bg-[linear-gradient(270deg,var(--studio-status-live-bg)_0%,var(--studio-status-live-bg)_34%,transparent_100%)] tw:px-2 tw:py-1.5"
}

fn record_summary_class(expanded: bool) -> &'static str {
    if expanded {
        "tw:text-xs tw:font-semibold tw:uppercase tw:text-subtle-foreground"
    } else {
        "tw:text-xs tw:font-semibold tw:uppercase tw:text-muted-foreground"
    }
}

fn expand_chevron_class(expanded: bool) -> &'static str {
    if expanded {
        "tw:inline-flex tw:rotate-90 tw:transition-transform"
    } else {
        "tw:inline-flex tw:transition-transform"
    }
}
