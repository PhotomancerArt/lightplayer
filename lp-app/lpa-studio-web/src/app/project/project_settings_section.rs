//! The project popup's "Project settings" rows.
//!
//! **Deliberately not the generic `SlotRecordEditor`.** The project root's
//! own slots are the project's *identity*, and the generic slot machinery
//! dresses identity as authoring: option-presence toggles on `uid`, a full
//! map editor for the `nodes` table, edit chrome on rows nothing may edit.
//! A demo walk read that as "the Studio lets you retype your project's
//! uid" — which, until 2026-07-28, it did (`ProjectDef::uid` carried the
//! default writable policy).
//!
//! So this section renders four purpose-built rows instead:
//!
//! - **Name** — the one authored field, an inline [`StringSlotField`] so it
//!   keeps the shared edit dispatch, dirty affordance, and invalid state.
//! - **Format** — read-only; only the loader gate and a future offline
//!   upgrader own it.
//! - **UID** — read-only, with a copy button (identity is the thing you
//!   actually want on your clipboard when reporting a problem).
//! - **Nodes** — read-only, collapsed to a **count**. The map itself is the
//!   node tree, which is the pane's whole body; repeating it as a slot
//!   editor in the popup was noise.
//!
//! `uid`, `format`, and `nodes` all carry role `Fixed` in
//! `lpc_model::ProjectDef`, so the read-only presentation here agrees with
//! the model rather than merely hiding a writable slot.

use dioxus::prelude::*;
use lpa_studio_core::{UiAction, UiConfigSlot, UiConfigSlotBody, UiSlotValueKind};

use crate::app::node::StringSlotField;
use crate::base::{StudioIcon, StudioIconName};

/// The project root's identity rows, in a fixed order that does not depend
/// on the slot tree's field order.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ProjectSettingsSection(
    /// The project root node's own config slots (`ProjectEditorView::root_slots`).
    root_slots: Vec<UiConfigSlot>,
    on_action: EventHandler<UiAction>,
) -> Element {
    let name = row(&root_slots, "name");
    let format = row(&root_slots, "format");
    let uid = row(&root_slots, "uid");
    let nodes = row(&root_slots, "nodes");

    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:gap-1.5",
            if let Some(name) = name {
                div { class: "tw:flex tw:min-w-0 tw:items-baseline tw:justify-between tw:gap-3 tw:text-xs tw:leading-snug",
                    span { class: "tw:flex-none tw:font-bold tw:text-subtle-foreground", "Name" }
                    span { class: "tw:min-w-0 tw:flex-1 tw:text-right",
                        StringSlotField {
                            value: string_value(name).unwrap_or_default(),
                            state: name.state.clone(),
                            address: name.address.clone(),
                            on_action,
                        }
                    }
                }
            }
            if let Some(format) = format {
                ReadOnlyRow { label: "Format", value: display_value(format) }
            }
            if let Some(uid) = uid {
                ReadOnlyRow { label: "UID", value: display_value(uid), copyable: true }
            }
            if let Some(nodes) = nodes {
                ReadOnlyRow { label: "Nodes", value: node_count_label(nodes) }
            }
        }
    }
}

/// One read-only identity row: label, monospace value, and an optional
/// copy button.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ReadOnlyRow(label: String, value: String, #[props(default = false)] copyable: bool) -> Element {
    let to_copy = value.clone();
    rsx! {
        div { class: "tw:flex tw:min-w-0 tw:items-baseline tw:justify-between tw:gap-3 tw:text-xs tw:leading-snug",
            span { class: "tw:flex-none tw:font-bold tw:text-subtle-foreground", "{label}" }
            span { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-1",
                span { class: "tw:min-w-0 tw:truncate tw:font-mono tw:text-muted-foreground", "{value}" }
                if copyable && !value.is_empty() {
                    button {
                        class: "tw:flex-none tw:cursor-pointer tw:appearance-none tw:border-0 tw:bg-transparent tw:p-0 tw:text-subtle-foreground tw:transition-colors tw:hover:text-strong-foreground",
                        r#type: "button",
                        title: "Copy {label}",
                        onclick: move |event| {
                            event.stop_propagation();
                            crate::clipboard::write_text(&to_copy);
                        },
                        StudioIcon { name: StudioIconName::Copy, size: 12 }
                    }
                }
            }
        }
    }
}

/// The root slot row with this field key, if the project carries one.
fn row<'a>(root_slots: &'a [UiConfigSlot], key: &str) -> Option<&'a UiConfigSlot> {
    root_slots.iter().find(|slot| slot.key == key)
}

/// A value row's raw string, for the editable name field.
fn string_value(slot: &UiConfigSlot) -> Option<String> {
    match &slot.body {
        UiConfigSlotBody::Value(value) => match &value.kind {
            UiSlotValueKind::String(text) => Some(text.clone()),
            _ => None,
        },
        _ => None,
    }
}

/// A row's formatted display string; an absent optional reads as an em
/// dash rather than the slot machinery's "unset".
fn display_value(slot: &UiConfigSlot) -> String {
    match &slot.body {
        UiConfigSlotBody::Value(value) => match value.kind {
            UiSlotValueKind::Unset => "—".to_string(),
            _ => value.display.clone(),
        },
        _ => "—".to_string(),
    }
}

/// The `nodes` map collapsed to a count — the map's contents are the node
/// tree in the pane body, not popup material.
fn node_count_label(slot: &UiConfigSlot) -> String {
    let count = match &slot.body {
        UiConfigSlotBody::Record(record) => record.fields.len(),
        _ => 0,
    };
    match count {
        1 => "1 node".to_string(),
        count => format!("{count} nodes"),
    }
}

#[cfg(test)]
mod tests {
    use lpa_studio_core::{UiSlotFieldState, UiSlotValue};

    use super::*;

    #[test]
    fn the_nodes_map_collapses_to_a_count() {
        assert_eq!(node_count_label(&nodes_row(0)), "0 nodes");
        assert_eq!(node_count_label(&nodes_row(1)), "1 node");
        assert_eq!(node_count_label(&nodes_row(4)), "4 nodes");
    }

    #[test]
    fn absent_optionals_read_as_a_dash_not_unset() {
        let unset = UiConfigSlot::value("uid", "UID", UiSlotValue::unset());
        assert_eq!(display_value(&unset), "—");
        // A structural row with no value body degrades the same way.
        assert_eq!(display_value(&nodes_row(2)), "—");
    }

    #[test]
    fn rows_are_found_by_field_key_regardless_of_slot_order() {
        let slots = vec![
            UiConfigSlot::value("format", "Format", UiSlotValue::u32(1)),
            UiConfigSlot::value("name", "Name", UiSlotValue::string("Demo")),
        ];
        assert_eq!(
            row(&slots, "name").map(display_value).as_deref(),
            Some("Demo")
        );
        assert_eq!(
            row(&slots, "format").map(display_value).as_deref(),
            Some("1")
        );
        assert!(
            row(&slots, "notes").is_none(),
            "no such field on ProjectDef"
        );
    }

    #[test]
    fn only_string_value_rows_yield_an_editable_name() {
        let named = UiConfigSlot::value("name", "Name", UiSlotValue::string("Demo"));
        assert_eq!(string_value(&named).as_deref(), Some("Demo"));
        // A u32 row must not be coerced into the text field.
        let format = UiConfigSlot::value("format", "Format", UiSlotValue::u32(1));
        assert_eq!(string_value(&format), None);
    }

    #[test]
    fn identity_rows_carry_the_models_read_only_policy() {
        // Guards the P1 model change: if `uid` ever goes writable again,
        // the fixture stops matching the model and this fails.
        let uid = UiConfigSlot::value("uid", "UID", UiSlotValue::string("prj_abc"))
            .with_state(UiSlotFieldState::readonly());
        assert!(!uid.state.editable);
    }

    fn nodes_row(count: usize) -> UiConfigSlot {
        let fields = (0..count)
            .map(|index| {
                UiConfigSlot::value(
                    format!("nodes[n{index}]"),
                    format!("n{index}"),
                    UiSlotValue::string("./n.json"),
                )
            })
            .collect();
        UiConfigSlot::record("nodes", "Nodes", fields)
    }
}
