//! The binding block inside the slot detail popover (roadmap M4).
//!
//! One coherent section tells the whole binding story: the current wiring
//! ("Published as bus:time" / "Reading from bus:time"), its origin (a slot-
//! declared default vs. an authored entry), any secondary routes, and the
//! authoring affordances — Bind/Edit/Unbind (2026-07-15 gate feedback:
//! no read-only summary above a disconnected "Bind" section).
//!
//! Bind, edit, and unbind are ordinary slot edits on the node's `bindings`
//! map — the section dispatches the same structural gestures the generic
//! editors use (`EnsurePresent` entry → `EnsurePresent` endpoint option →
//! `SetValue`; unbind is `RemoveValue` on the entry, which also re-enables
//! any slot-declared default). The channel picker is seeded from the shared
//! channel choices (observed ∪ well-known, provided as context by the
//! project workspace); free-text entry stays legal — the picker teaches the
//! naming norm, it does not gate (D9).

use dioxus::prelude::*;
use lpa_studio_core::{
    LpValue, UiAction, UiBindingAuthoring, UiChannelChoice, UiSlotAffordance, UiSlotAspect,
};

use crate::app::node::slot_detail_button::{SlotDetailRow, aspect_detail_rows, binding_title};
use crate::app::node::slot_edit_actions::{
    slot_ensure_present_action, slot_remove_value_action, slot_set_value_action,
};
use crate::base::{DetailSectionTint, StudioIcon, StudioIconName, detail_popover_section_class};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn BindingAuthoringSection(
    authoring: UiBindingAuthoring,
    on_action: EventHandler<UiAction>,
    /// The slot's current Binding aspect, folded into this block so wiring
    /// state and authoring affordances read as one story.
    #[props(default = None)]
    current: Option<UiSlotAspect>,
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

    // Current-wiring presentation from the folded Binding aspect.
    let bound = current
        .as_ref()
        .is_some_and(|aspect| aspect.affordance == Some(UiSlotAffordance::Bound));
    let heading = match (&current, bound) {
        (Some(aspect), true) => binding_title(aspect),
        _ => "Unbound".to_string(),
    };
    let endpoint_code = bound
        .then(|| {
            current
                .as_ref()
                .and_then(|aspect| aspect.rows.first())
                .map(|row| row.value.clone())
                .filter(|value| !value.is_empty())
        })
        .flatten();
    let detail_rows = current.as_ref().map(aspect_detail_rows).unwrap_or_default();
    let (tint, icon, icon_class, heading_class) = if bound {
        (
            DetailSectionTint::Bound,
            StudioIconName::BoundValue,
            "tw:inline-flex tw:flex-none tw:items-center tw:justify-center tw:text-status-bound-foreground",
            "tw:m-0 tw:text-xs tw:font-bold tw:text-status-bound-foreground",
        )
    } else {
        (
            DetailSectionTint::None,
            StudioIconName::UnboundValue,
            "tw:inline-flex tw:flex-none tw:items-center tw:justify-center tw:text-heading",
            "tw:m-0 tw:text-xs tw:font-bold tw:text-heading",
        )
    };

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
    let free_text_ready = !free_text_trimmed.is_empty() && free_text_issue.is_none();
    // Submit the free-text channel — shared by the Bind button and Enter in
    // the text field.
    let submit_free_text = {
        let bind = bind.clone();
        let name = free_text_trimmed.clone();
        move || {
            bind(&name);
            picker_open.set(false);
            free_text.set(String::new());
        }
    };

    rsx! {
        section { class: detail_popover_section_class(tint),
            header { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-1.5 tw:leading-none",
                span { class: icon_class,
                    StudioIcon { name: icon, size: 12 }
                }
                h3 { class: heading_class, "{heading}" }
                if let Some(code) = endpoint_code {
                    code { class: "tw:min-w-0 tw:truncate tw:font-mono tw:text-xs tw:text-muted-foreground", "{code}" }
                }
            }
            if !detail_rows.is_empty() {
                div { class: "tw:grid tw:min-w-0 tw:gap-0.5 tw:pl-[18px] tw:pt-0.5",
                    for row in detail_rows {
                        SlotDetailRow { row, on_action }
                    }
                }
            }
            if !picker_open() {
                div { class: "tw:flex tw:flex-wrap tw:items-center tw:gap-1.5 tw:pl-[18px] tw:pt-1",
                    button {
                        class: authoring_button_class(),
                        r#type: "button",
                        title: if authored.is_some() {
                            "Point this slot's authored binding at a different channel"
                        } else if bound {
                            "The slot declares this default wiring; authoring a binding overrides it"
                        } else {
                            "Author a binding from this slot to a bus channel"
                        },
                        onclick: move |event| {
                            event.stop_propagation();
                            picker_open.set(true);
                        },
                        if bound {
                            "Edit\u{2026}"
                        } else {
                            "Bind\u{2026}"
                        }
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
                div { class: "tw:grid tw:min-w-0 tw:gap-1 tw:pl-[18px] tw:pt-1",
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
                    div { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-1.5 tw:pt-0.5",
                        code { class: "tw:flex-none tw:font-mono tw:text-[11px] tw:text-subtle-foreground", "bus:" }
                        input {
                            class: "tw:min-w-0 tw:flex-1 tw:rounded-xs tw:border tw:border-border-strong tw:bg-transparent tw:px-1.5 tw:py-0.5 tw:font-mono tw:text-[11px] tw:text-strong-foreground",
                            r#type: "text",
                            placeholder: "channel.name",
                            value: "{free_text_value}",
                            oninput: move |event| free_text.set(event.value()),
                            onkeydown: {
                                let mut submit_free_text = submit_free_text.clone();
                                move |event: Event<KeyboardData>| {
                                    if event.key() == Key::Enter && free_text_ready {
                                        event.stop_propagation();
                                        submit_free_text();
                                    }
                                }
                            },
                        }
                        button {
                            class: authoring_button_class(),
                            r#type: "button",
                            disabled: !free_text_ready,
                            title: "Bind to the entered channel (created lazily by reference)",
                            onclick: {
                                let mut submit_free_text = submit_free_text.clone();
                                move |event: Event<MouseData>| {
                                    event.stop_propagation();
                                    submit_free_text();
                                }
                            },
                            "Bind"
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

/// One pickable channel row: bus glyph, mono name, kind tag, well-known
/// marker, and the registry doc as the tooltip.
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
            class: "tw:flex tw:min-w-0 tw:cursor-pointer tw:appearance-none tw:items-center tw:gap-1.5 tw:overflow-hidden tw:rounded-xs tw:border tw:border-status-bound-border tw:bg-transparent tw:px-1.5 tw:py-0.5 tw:text-left tw:leading-none tw:text-status-bound-foreground tw:transition-colors tw:hover:border-status-bound-foreground",
            r#type: "button",
            title,
            onclick: move |event| {
                event.stop_propagation();
                on_pick.call(());
            },
            StudioIcon {
                name: StudioIconName::Bus,
                size: 10,
            }
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
