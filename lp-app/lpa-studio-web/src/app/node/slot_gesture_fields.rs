//! Composite gesture field renderers: map entry add/remove, option some/none
//! toggle, and enum variant switch.
//!
//! Gestures ARE the wire ops (M3 decision D1): each control dispatches one
//! `SlotEditOp::EnsurePresent`/`RemoveValue` at the target address and the
//! server constructs all defaults — the client never composes composite
//! values. Structural ops never coalesce; rejections surface as error chrome
//! on the dispatching composite row via the prefix join.

use dioxus::prelude::*;
use lpa_studio_core::{
    ProjectSlotAddress, UiAction, UiSlotEnumComposite, UiSlotFieldState, UiSlotMapComposite,
    UiSlotMapKeyKind, UiSlotOptionality,
};

use crate::app::node::slot_edit_actions::{slot_ensure_present_action, slot_remove_value_action};
use crate::app::node::slot_fields::{dropdown_field_class, field_class, field_wiring};

/// Variant switcher for an enum composite row. Selecting a variant dispatches
/// `EnsurePresent enum_path.variant` (RAW declared ident, verbatim); the
/// payload rows re-render for the new variant on the next snapshot.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn EnumVariantField(
    composite: UiSlotEnumComposite,
    state: UiSlotFieldState,
    #[props(default = None)] address: Option<ProjectSlotAddress>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let invalid_title = state.invalid.clone().unwrap_or_default();

    if let Some((address, handler)) = field_wiring(&state, &address, on_action) {
        let active = composite.active.clone();
        return rsx! {
            select {
                class: dropdown_field_class(&state),
                title: "{invalid_title}",
                aria_label: "Switch variant",
                value: "{composite.active}",
                oninput: move |event| {
                    let variant = event.value();
                    if variant != active
                        && let Some(target) = address.child_field(&variant)
                    {
                        handler.call(slot_ensure_present_action(target));
                    }
                },
                for variant in composite.variants.clone() {
                    option {
                        value: "{variant}",
                        selected: variant == composite.active,
                        "{variant}"
                    }
                }
            }
        };
    }

    rsx! {
        span { class: field_class(&state), title: "{invalid_title}",
            span { class: "tw:min-w-0 tw:truncate", "{composite.active}" }
            span { class: "tw:text-subtle-foreground", "v" }
        }
    }
}

/// Add-entry affordance for a map composite row: a compact "+" button that
/// opens an inline key input typed by the map's key domain (numeric maps
/// prefill the next free index). Confirming dispatches
/// `EnsurePresent map_path[key]`; the server constructs the entry default.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn MapAddEntry(
    composite: UiSlotMapComposite,
    state: UiSlotFieldState,
    #[props(default = None)] address: Option<ProjectSlotAddress>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
    #[props(default = false)] initially_open: bool,
) -> Element {
    let mut open = use_signal(|| initially_open);
    let mut draft = use_signal(|| composite.suggested_key.clone());
    let Some((address, handler)) = field_wiring(&state, &address, on_action) else {
        return rsx! {};
    };
    let key_kind = composite.key_kind;

    if !open() {
        let suggested = composite.suggested_key.clone();
        return rsx! {
            button {
                class: gesture_button_class(),
                r#type: "button",
                title: "Add entry",
                aria_label: "Add entry",
                onclick: move |event| {
                    event.stop_propagation();
                    draft.set(suggested.clone());
                    open.set(true);
                },
                "+"
            }
        };
    }

    let confirm_address = address.clone();
    rsx! {
        span { class: "tw:inline-flex tw:min-w-0 tw:items-center tw:gap-1",
            input {
                class: key_input_class(key_kind),
                r#type: if key_kind.is_numeric() { "number" } else { "text" },
                min: if key_kind == UiSlotMapKeyKind::U32 { Some("0".to_string()) } else { None },
                step: if key_kind.is_numeric() { Some("1".to_string()) } else { None },
                value: "{draft}",
                aria_label: "New entry key",
                oninput: move |event| draft.set(event.value()),
                onkeydown: move |event| match event.key() {
                    Key::Enter => {
                        if dispatch_map_add(key_kind, &draft(), &address, &handler) {
                            open.set(false);
                        }
                    }
                    Key::Escape => open.set(false),
                    _ => {}
                },
            }
            button {
                class: gesture_button_class(),
                r#type: "button",
                title: "Add entry with this key",
                aria_label: "Confirm add entry",
                onclick: move |event| {
                    event.stop_propagation();
                    if dispatch_map_add(key_kind, &draft(), &confirm_address, &handler) {
                        open.set(false);
                    }
                },
                "+"
            }
            button {
                class: gesture_button_class(),
                r#type: "button",
                title: "Cancel adding an entry",
                aria_label: "Cancel add entry",
                onclick: move |event| {
                    event.stop_propagation();
                    open.set(false);
                },
                "\u{00d7}"
            }
        }
    }
}

/// Per-entry remove affordance for map entry rows: dispatches
/// `RemoveValue entry_path` (add-then-remove normalizes away server-side).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn MapEntryRemoveButton(
    address: ProjectSlotAddress,
    on_action: EventHandler<UiAction>,
) -> Element {
    rsx! {
        button {
            class: gesture_button_class(),
            r#type: "button",
            title: "Remove this entry",
            aria_label: "Remove this entry",
            onclick: move |event| {
                event.stop_propagation();
                on_action.call(slot_remove_value_action(address.clone()));
            },
            "\u{00d7}"
        }
    }
}

/// Some/none toggle for an option row. On dispatches
/// `EnsurePresent opt_path.some` (server default value); off dispatches
/// `RemoveValue opt_path`.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn OptionToggleField(
    optionality: UiSlotOptionality,
    #[props(default = None)] address: Option<ProjectSlotAddress>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let included = optionality.included;
    let wired = if optionality.can_toggle {
        address.zip(on_action)
    } else {
        None
    };
    let disabled = wired.is_none();
    let title = if included {
        "Optional value enabled"
    } else {
        "Optional value disabled"
    };

    rsx! {
        label { class: "ux-slot-optional-toggle", title,
            input {
                class: "ux-slot-optional-toggle-input",
                r#type: "checkbox",
                checked: included,
                disabled,
                aria_label: title,
                onchange: move |_| {
                    let Some((address, handler)) = wired.clone() else {
                        return;
                    };
                    if included {
                        handler.call(slot_remove_value_action(address));
                    } else if let Some(some) = address.child_field("some") {
                        handler.call(slot_ensure_present_action(some));
                    }
                },
            }
            span { class: "ux-slot-optional-toggle-track",
                span { class: "ux-slot-optional-toggle-thumb" }
            }
            span { class: "ux-slot-optional-toggle-label", "enabled" }
        }
    }
}

/// Parse the drafted key and dispatch the map add gesture. Returns whether
/// an op was dispatched (unparseable/empty keys never dispatch).
fn dispatch_map_add(
    key_kind: UiSlotMapKeyKind,
    raw: &str,
    address: &ProjectSlotAddress,
    handler: &EventHandler<UiAction>,
) -> bool {
    let Some(key) = key_kind.parse_key(raw) else {
        return false;
    };
    handler.call(slot_ensure_present_action(address.child_map_entry(key)));
    true
}

/// Compact square button shared by the add/remove/cancel gesture controls.
fn gesture_button_class() -> &'static str {
    "tw:inline-flex tw:h-6 tw:w-6 tw:flex-none tw:cursor-pointer tw:appearance-none tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-border-subtle tw:bg-page tw:p-0 tw:text-sm tw:font-bold tw:text-muted-foreground tw:hover:text-strong-foreground"
}

fn key_input_class(key_kind: UiSlotMapKeyKind) -> &'static str {
    if key_kind.is_numeric() {
        "tw:w-16 tw:min-w-0 tw:rounded-xs tw:border tw:border-border-subtle tw:bg-page tw:px-1.5 tw:py-0.5 tw:text-right tw:font-mono tw:text-sm tw:text-muted-foreground tw:outline-none"
    } else {
        "tw:w-24 tw:min-w-0 tw:rounded-xs tw:border tw:border-border-subtle tw:bg-page tw:px-1.5 tw:py-0.5 tw:text-sm tw:text-muted-foreground tw:outline-none"
    }
}
