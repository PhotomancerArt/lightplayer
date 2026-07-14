//! Binding authoring inside the slot detail popover (roadmap M4).
//!
//! Bind, retarget, and unbind are ordinary slot edits on the node's
//! `bindings` map — the section dispatches the same structural gestures the
//! generic editors use (`EnsurePresent` entry → `EnsurePresent` endpoint
//! option → `SetValue`; unbind is `RemoveValue` on the entry, which also
//! re-enables any slot-declared default). The channel picker is seeded from
//! the shared channel choices (observed ∪ well-known, provided as context by
//! the project workspace); free-text entry stays legal — the picker teaches
//! the naming norm, it does not gate (D9).

use dioxus::prelude::*;
use lpa_studio_core::{LpValue, UiAction, UiBindingAuthoring, UiChannelChoice};

use crate::app::node::slot_edit_actions::{
    slot_ensure_present_action, slot_remove_value_action, slot_set_value_action,
};
use crate::base::{DetailSection, DetailSectionTint};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn BindingAuthoringSection(
    authoring: UiBindingAuthoring,
    on_action: EventHandler<UiAction>,
    /// Open the channel picker on first render (story/testing affordance).
    #[props(default = false)]
    initially_picking: bool,
) -> Element {
    let mut picker_open = use_signal(|| initially_picking);
    let mut free_text = use_signal(String::new);
    let choices = try_consume_context::<Signal<Vec<UiChannelChoice>>>()
        .map(|signal| signal())
        .unwrap_or_default();

    let authored = authoring.authored.clone();
    let entry_address = authoring.entry_address();
    let endpoint_address = authoring.endpoint_value_address();
    // Kind of the currently wired channel, for retarget mismatch hints.
    let current_kind = authored
        .as_ref()
        .and_then(|endpoint| endpoint.label.strip_prefix("bus:"))
        .and_then(|name| choices.iter().find(|choice| choice.name == name))
        .and_then(|choice| choice.kind.clone());

    let bind = {
        let entry_address = entry_address.clone();
        let endpoint_address = endpoint_address.clone();
        move |channel: &str| {
            let Some(endpoint_address) = endpoint_address.clone() else {
                return;
            };
            on_action.call(slot_ensure_present_action(entry_address.clone()));
            on_action.call(slot_ensure_present_action(endpoint_address.clone()));
            on_action.call(slot_set_value_action(
                endpoint_address,
                LpValue::String(format!("bus:{channel}")),
            ));
        }
    };

    let free_text_value = free_text();
    let free_text_trimmed = free_text_value.trim().to_string();
    let free_text_issue = channel_name_issue(&free_text_trimmed);
    let verb = if authored.is_some() {
        "Retarget"
    } else {
        "Bind"
    };

    rsx! {
        DetailSection { title: "Bind", tint: DetailSectionTint::Bound,
            if !picker_open() {
                div { class: "tw:flex tw:flex-wrap tw:items-center tw:gap-1.5 tw:pt-0.5",
                    button {
                        class: authoring_button_class(),
                        r#type: "button",
                        title: if authored.is_some() {
                            "Point this slot's authored binding at a different channel"
                        } else {
                            "Author a binding from this slot to a bus channel"
                        },
                        onclick: move |event| {
                            event.stop_propagation();
                            picker_open.set(true);
                        },
                        "{verb}\u{2026}"
                    }
                    if authored.is_some() {
                        button {
                            class: authoring_button_class(),
                            r#type: "button",
                            title: "Remove the authored binding entry; a slot-declared default (if any) takes over",
                            onclick: {
                                let entry_address = entry_address.clone();
                                move |event: Event<MouseData>| {
                                    event.stop_propagation();
                                    on_action.call(slot_remove_value_action(entry_address.clone()));
                                }
                            },
                            "Unbind"
                        }
                    }
                }
            } else {
                div { class: "tw:grid tw:gap-1 tw:pt-0.5",
                    for choice in choices.clone() {
                        BindingChannelChoice {
                            choice: choice.clone(),
                            mismatch: kind_mismatch(&current_kind, &choice),
                            on_pick: {
                                let bind = bind.clone();
                                let name = choice.name.clone();
                                move |_| {
                                    bind(&name);
                                    picker_open.set(false);
                                }
                            },
                        }
                    }
                    div { class: "tw:flex tw:items-center tw:gap-1.5 tw:pt-0.5",
                        code { class: "tw:flex-none tw:font-mono tw:text-[11px] tw:text-subtle-foreground", "bus:" }
                        input {
                            class: "tw:min-w-0 tw:flex-1 tw:rounded-xs tw:border tw:border-border-strong tw:bg-transparent tw:px-1.5 tw:py-0.5 tw:font-mono tw:text-[11px] tw:text-strong-foreground",
                            r#type: "text",
                            placeholder: "channel.name",
                            value: "{free_text_value}",
                            oninput: move |event| free_text.set(event.value()),
                        }
                        button {
                            class: authoring_button_class(),
                            r#type: "button",
                            disabled: free_text_trimmed.is_empty() || free_text_issue.is_some(),
                            title: "Bind to the entered channel (created lazily by reference)",
                            onclick: {
                                let bind = bind.clone();
                                let name = free_text_trimmed.clone();
                                move |event: Event<MouseData>| {
                                    event.stop_propagation();
                                    bind(&name);
                                    picker_open.set(false);
                                    free_text.set(String::new());
                                }
                            },
                            "{verb}"
                        }
                        button {
                            class: authoring_button_class(),
                            r#type: "button",
                            onclick: move |event| {
                                event.stop_propagation();
                                picker_open.set(false);
                            },
                            "Cancel"
                        }
                    }
                    if let Some(issue) = free_text_issue {
                        p { class: "tw:m-0 tw:text-[11px] tw:leading-snug tw:text-status-warning-foreground", "{issue}" }
                    }
                }
            }
        }
    }
}

/// One pickable channel row: mono name, kind tag, well-known marker, and the
/// registry doc as the tooltip.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn BindingChannelChoice(
    choice: UiChannelChoice,
    mismatch: bool,
    on_pick: EventHandler<()>,
) -> Element {
    let title = match (choice.doc, mismatch) {
        (Some(doc), false) => doc.to_string(),
        (Some(doc), true) => format!("{doc} — kind differs from the current channel"),
        (None, true) => {
            "Observed in this project — kind differs from the current channel".to_string()
        }
        (None, false) => "Observed in this project".to_string(),
    };

    rsx! {
        button {
            class: "tw:flex tw:min-w-0 tw:cursor-pointer tw:appearance-none tw:items-center tw:gap-1.5 tw:rounded-xs tw:border tw:border-status-bound-border tw:bg-transparent tw:px-1.5 tw:py-0.5 tw:text-left tw:leading-none tw:text-status-bound-foreground tw:transition-colors tw:hover:border-status-bound-foreground",
            r#type: "button",
            title,
            onclick: move |event| {
                event.stop_propagation();
                on_pick.call(());
            },
            code { class: "tw:min-w-0 tw:truncate tw:font-mono tw:text-[11px] tw:font-semibold", "{choice.name}" }
            if let Some(kind) = &choice.kind {
                span { class: "tw:flex-none tw:text-[9px] tw:font-bold tw:uppercase tw:text-subtle-foreground", "{kind}" }
            }
            span { class: "tw:min-w-0 tw:flex-1" }
            if mismatch {
                span { class: "tw:flex-none tw:text-[9px] tw:font-bold tw:uppercase tw:text-status-warning-foreground", "kind?" }
            }
            if choice.well_known {
                span {
                    class: "tw:flex-none tw:text-[9px] tw:font-bold tw:uppercase tw:text-subtle-foreground",
                    title: "Well-known channel",
                    "wk"
                }
            }
        }
    }
}

fn kind_mismatch(current_kind: &Option<String>, choice: &UiChannelChoice) -> bool {
    matches!(
        (current_kind, &choice.kind),
        (Some(current), Some(choice_kind)) if current != choice_kind
    )
}

/// Warn-only validation for free-text channel names: the engine remains the
/// authority; this only catches names the ref grammar cannot represent.
fn channel_name_issue(name: &str) -> Option<&'static str> {
    if name.is_empty() {
        return None;
    }
    if name.contains('#') {
        return Some("`#` is reserved in bus refs (future field-within-channel syntax).");
    }
    if name.contains(':') || name.contains(char::is_whitespace) {
        return Some("Channel names cannot contain `:` or whitespace.");
    }
    None
}

fn authoring_button_class() -> &'static str {
    "tw:inline-flex tw:flex-none tw:cursor-pointer tw:appearance-none tw:items-center tw:gap-1 tw:rounded-xs tw:border tw:border-border-strong tw:bg-transparent tw:px-1.5 tw:py-0.5 tw:text-[0.68rem] tw:font-bold tw:text-muted-foreground tw:hover:bg-card-muted tw:hover:text-strong-foreground tw:disabled:cursor-default tw:disabled:opacity-50"
}
