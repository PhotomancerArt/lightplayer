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
    ProjectSlotAddress, SlotMapKey, SlotPath, SlotPathSegment, UiAction, UiSlotEnumComposite,
    UiSlotFieldState, UiSlotMapComposite, UiSlotMapKeyKind, UiSlotOptionality,
};

use crate::app::node::slot_edit_actions::{
    slot_ensure_present_action, slot_move_entry_action, slot_remove_value_action,
};
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

/// Add-entry affordance for a map composite row (M3 UX gate rework).
///
/// Numeric-keyed maps add **immediately**: "+" dispatches
/// `EnsurePresent map_path[first free index]` (the gap-filling suggested key
/// from the DTO — the server constructs the entry default; no inline value
/// entry), and a compact secondary "#" affordance opens the key input as an
/// optional override. String-keyed maps keep the key input as the primary
/// flow ("+" opens it) — string keys cannot be guessed.
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
        if key_kind.is_numeric() {
            let add_key = suggested.clone();
            let add_address = address.clone();
            let add_title = format!("Add entry at key {suggested}");
            return rsx! {
                span { class: "tw:inline-flex tw:flex-none tw:items-center tw:gap-1",
                    button {
                        class: gesture_button_class(),
                        r#type: "button",
                        title: "{add_title}",
                        aria_label: "{add_title}",
                        onclick: move |event| {
                            event.stop_propagation();
                            dispatch_map_add(key_kind, &add_key, &add_address, &handler);
                        },
                        "+"
                    }
                    button {
                        class: gesture_button_class(),
                        r#type: "button",
                        title: "Add entry at a chosen key",
                        aria_label: "Add entry at a chosen key",
                        onclick: move |event| {
                            event.stop_propagation();
                            draft.set(suggested.clone());
                            open.set(true);
                        },
                        "#"
                    }
                }
            };
        }
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

/// Click-to-edit key label for a map entry row: the entry's key renders as
/// the row label, and clicking it opens a compact key input typed per the
/// map's key domain. Committing a changed, parseable key dispatches
/// `SlotEditOp::MoveEntry` on the parent map (`from_key` is the entry
/// address's terminal key segment); unchanged or unparseable input never
/// dispatches. An occupied target is rejected server-side
/// (`target_occupied`) and parks Failed on the map row.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn MapEntryKeyField(
    /// The entry's current key text (the row label).
    label: String,
    key_kind: UiSlotMapKeyKind,
    /// Address of the entry row; its terminal path segment is the key.
    address: ProjectSlotAddress,
    on_action: EventHandler<UiAction>,
    /// Open the key input on first render (stories).
    #[props(default = false)]
    initially_editing: bool,
) -> Element {
    let mut editing = use_signal(|| initially_editing);
    let mut draft = use_signal(|| label.clone());
    let Some((map_address, from_key)) = split_map_entry(&address) else {
        // Not a key-terminated address: fall back to the static label.
        return rsx! {
            strong { class: "tw:block tw:min-w-0 tw:text-sm tw:font-semibold tw:leading-tight tw:text-strong-foreground tw:break-words", "{label}" }
        };
    };

    if !editing() {
        let open_draft = label.clone();
        return rsx! {
            button {
                class: entry_key_label_class(),
                r#type: "button",
                title: "Edit this entry's key",
                aria_label: "Edit entry key",
                onclick: move |event| {
                    event.stop_propagation();
                    draft.set(open_draft.clone());
                    editing.set(true);
                },
                "{label}"
            }
        };
    }

    let change_from = from_key.clone();
    let enter_from = from_key;
    rsx! {
        input {
            class: key_input_class(key_kind),
            r#type: if key_kind.is_numeric() { "number" } else { "text" },
            min: if key_kind == UiSlotMapKeyKind::U32 { Some("0".to_string()) } else { None },
            step: if key_kind.is_numeric() { Some("1".to_string()) } else { None },
            value: "{draft}",
            aria_label: "Entry key",
            oninput: move |event| draft.set(event.value()),
            onchange: move |_| {
                if let Some(target) = entry_move_target(key_kind, &draft(), &change_from) {
                    if let Some(to_key) = target {
                        on_action
                            .call(
                                slot_move_entry_action(
                                    map_address.clone(),
                                    change_from.clone(),
                                    to_key,
                                ),
                            );
                    }
                    editing.set(false);
                }
            },
            onkeydown: move |event| match event.key() {
                // A changed key commits through the change event; Enter only
                // needs to close the unchanged case (which fires no change).
                Key::Enter => {
                    if entry_move_target(key_kind, &draft(), &enter_from) == Some(None) {
                        editing.set(false);
                    }
                }
                Key::Escape => editing.set(false),
                _ => {}
            },
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
///
/// The checkbox is **controlled**: the DTO's effective presence is the only
/// writer of its visual state, so the click handler prevents the browser's
/// native flip and only dispatches the gesture. Without this, a gesture that
/// normalizes to a no-op against base (the D2 base-relative edge — e.g.
/// toggling a base-present option off, then on) leaves the self-flipped DOM
/// checkbox permanently desynced from the DTO, because Dioxus sees no
/// attribute change to patch.
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
                onclick: move |event| {
                    event.prevent_default();
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

/// Split a map entry address into the parent map's address plus the entry
/// key. `None` when the terminal path segment is not a map key (the root, a
/// field, or a variant).
fn split_map_entry(address: &ProjectSlotAddress) -> Option<(ProjectSlotAddress, SlotMapKey)> {
    let (last, parent) = address.path.segments().split_last()?;
    let SlotPathSegment::Key(key) = last else {
        return None;
    };
    Some((
        ProjectSlotAddress::new(
            address.node.clone(),
            address.root.clone(),
            SlotPath::from_segments(parent.to_vec()),
        ),
        key.clone(),
    ))
}

/// What the drafted key text settles the key edit to: `None` keeps the input
/// open (unparseable/empty), `Some(None)` closes without dispatch (the key
/// is unchanged), and `Some(Some(to_key))` is a real move.
fn entry_move_target(
    key_kind: UiSlotMapKeyKind,
    raw: &str,
    from_key: &SlotMapKey,
) -> Option<Option<SlotMapKey>> {
    let to_key = key_kind.parse_key(raw)?;
    Some((to_key != *from_key).then_some(to_key))
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

/// The closed key-edit label: renders exactly like the plain row label
/// (block, semibold), with a dotted-underline hover affordance signalling
/// click-to-edit.
fn entry_key_label_class() -> &'static str {
    "tw:block tw:min-w-0 tw:cursor-pointer tw:appearance-none tw:border-0 tw:bg-transparent tw:p-0 tw:text-left tw:text-sm tw:font-semibold tw:leading-tight tw:text-strong-foreground tw:break-words tw:decoration-dotted tw:underline-offset-2 tw:hover:underline"
}

#[cfg(test)]
mod tests {
    use lpa_studio_core::{ProjectNodeAddress, ProjectSlotRoot};

    use super::*;

    fn entry_address(path: &str) -> ProjectSlotAddress {
        ProjectSlotAddress::new(
            ProjectNodeAddress::parse("/demo.project/clock.clock").unwrap(),
            ProjectSlotRoot::def(),
            SlotPath::parse(path).unwrap(),
        )
    }

    #[test]
    fn split_map_entry_returns_the_map_address_and_terminal_key() {
        let (map, key) = split_map_entry(&entry_address("mapping.paths[3]")).unwrap();
        assert_eq!(map.path, SlotPath::parse("mapping.paths").unwrap());
        assert_eq!(key, SlotMapKey::U32(3));

        let (map, key) = split_map_entry(&entry_address("presets[warm]")).unwrap();
        assert_eq!(map.path, SlotPath::parse("presets").unwrap());
        assert_eq!(key, SlotMapKey::String("warm".to_string()));
    }

    #[test]
    fn split_map_entry_rejects_non_key_terminals() {
        assert_eq!(split_map_entry(&entry_address("mapping.paths")), None);
        assert_eq!(split_map_entry(&entry_address("paths[3].diameter")), None);
        let root = ProjectSlotAddress::root(
            ProjectNodeAddress::parse("/demo.project/clock.clock").unwrap(),
            ProjectSlotRoot::def(),
        );
        assert_eq!(split_map_entry(&root), None);
    }

    #[test]
    fn entry_move_target_separates_stay_close_and_move() {
        let from = SlotMapKey::U32(2);
        assert_eq!(
            entry_move_target(UiSlotMapKeyKind::U32, "nope", &from),
            None,
            "unparseable input keeps editing"
        );
        assert_eq!(
            entry_move_target(UiSlotMapKeyKind::U32, "2", &from),
            Some(None),
            "unchanged key closes without dispatch"
        );
        assert_eq!(
            entry_move_target(UiSlotMapKeyKind::U32, "5", &from),
            Some(Some(SlotMapKey::U32(5))),
        );
    }
}
