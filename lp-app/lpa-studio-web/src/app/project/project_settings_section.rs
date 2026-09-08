//! The project popup's "Project settings" rows.
//!
//! **Deliberately not the generic `SlotRecordEditor`.** The project's own
//! identity is not authoring, and the generic slot machinery dresses it as
//! such: option-presence toggles on `uid`, a full map editor for the
//! `nodes` table, edit chrome on rows nothing may edit. A demo walk read
//! that as "the Studio lets you retype your project's uid" — which, until
//! 2026-07-28, it did.
//!
//! Post-mitosis the identity (`format` / `uid` / `name`) no longer lives in
//! a def at all: it is the `project.json` container manifest — library-owned
//! workspace metadata, never authored def slots — so this section renders it
//! from [`UiProjectManifest`], and the ONE thing a person may change about
//! it is the name.
//!
//! - **Name** — an inline rename form when a library package backs the
//!   project (`rename_uid` + `on_action`), dispatching the same
//!   `HomeOp::RenamePackage` the gallery kebab does; the controller patches
//!   the open project's manifest through its own handle and re-slugs the
//!   library directory when the project closes. Yona could not find how to
//!   name a project while setting up a piece (2026-09-06): the name was
//!   shown here and editable only on a card two pages away. Read-only when
//!   nothing backs the project (the demo path, a device-hosted project).
//! - **Hardware** — the target the project declares (D41): Desktop or a
//!   catalog board, behind the same two-group menu the Devices page's add
//!   slot opens. Editable on the same terms as the name, and read-only for
//!   the same reason when nothing backs the project.
//! - **Format / UID** — read-only, from the manifest. UID keeps its copy
//!   button (identity is the thing you actually want on your clipboard when
//!   reporting a problem).
//! - **Nodes** — the one root-def row left, collapsed to a **count**. The
//!   map itself is the node tree, which is the pane's whole body; repeating
//!   it as a slot editor in the popup was noise. It carries role `Fixed` in
//!   `lpc_model::ModuleDef`, so the read-only presentation agrees with the
//!   model rather than merely hiding a writable slot.

use dioxus::prelude::*;
use lpa_studio_core::{HomeOp, UiAction, UiConfigSlot, UiConfigSlotBody, UiProjectManifest};

use crate::app::home::package_card::home_action;
use crate::app::home::target_pick_popover::HardwarePickPopover;
use crate::base::{StudioIcon, StudioIconName};
use crate::core::quiet_action_class;

/// The project's identity rows, in a fixed order that does not depend on
/// the slot tree's field order.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ProjectSettingsSection(
    /// The container-manifest identity, when a library package backs the
    /// open project.
    #[props(default)]
    manifest: Option<UiProjectManifest>,
    /// The project root node's own config slots (`ProjectEditorView::root_slots`).
    #[props(default)]
    root_slots: Vec<UiConfigSlot>,
    /// The library package the rename addresses (`prj…`). `None` = no
    /// package backs the project, and the name row is read-only.
    #[props(default)]
    rename_uid: Option<String>,
    /// Dispatch for the rename. `None` renders the name read-only too.
    #[props(default)]
    on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let nodes = row(&root_slots, "nodes");
    let manifest = manifest.unwrap_or_default();
    let target = manifest.target.clone();
    let rename = rename_uid.zip(on_action);

    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:gap-1.5",
            if let Some(name) = manifest.name {
                if let Some((uid, on_action)) = rename.clone() {
                    EditableNameRow { uid, name, on_action }
                } else {
                    ReadOnlyRow { label: "Name", value: name }
                }
            }
            // The hardware this project is FOR (D41). Editable when a
            // library package backs it, for the same reason the name is:
            // this is the project's own settings, and its hardware is one
            // of the two things about it a person actually chooses.
            if let Some((uid, on_action)) = rename.clone() {
                HardwareRow { uid, target: target.clone(), on_action }
            } else {
                ReadOnlyRow {
                    label: "Hardware",
                    value: lpa_studio_core::board_display_name(
                        target.as_deref().unwrap_or(lpa_studio_core::DESKTOP_BOARD_ID),
                    ),
                }
            }
            if let Some(format) = manifest.format {
                ReadOnlyRow { label: "Format", value: format.to_string() }
            }
            if let Some(uid) = manifest.uid {
                ReadOnlyRow { label: "UID", value: uid, copyable: true }
            }
            if let Some(nodes) = nodes {
                ReadOnlyRow { label: "Nodes", value: node_count_label(nodes) }
            }
        }
    }
}

/// The name row with its rename form: the label in the row's place, the
/// field where the value was, and a quiet Rename beside it. Submitting
/// dispatches `RenamePackage` for the backing package — the controller
/// decides whether that is a catalog rename (a closed project) or a patch
/// through the open project's own handle. Blank never submits.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn EditableNameRow(uid: String, name: String, on_action: EventHandler<UiAction>) -> Element {
    let mut value = use_signal(|| name);

    rsx! {
        form {
            class: "tw:flex tw:min-w-0 tw:items-center tw:gap-2 tw:text-xs tw:leading-snug",
            onsubmit: move |event| {
                event.prevent_default();
                let name = value.read().trim().to_string();
                if name.is_empty() {
                    return;
                }
                on_action.call(home_action(HomeOp::RenamePackage {
                    uid: uid.clone(),
                    name,
                }));
            },
            span { class: "tw:flex-none tw:font-bold tw:text-subtle-foreground", "Name" }
            input {
                class: "tw:min-w-0 tw:flex-1 tw:rounded tw:border tw:border-border tw:bg-terminal tw:px-2 tw:py-0.5 tw:text-xs tw:text-strong-foreground",
                aria_label: "Project name",
                value: "{value}",
                oninput: move |event| value.set(event.value()),
            }
            button { class: quiet_action_class(), r#type: "submit", "Rename" }
        }
    }
}

/// The Hardware row: the target's display name behind a menu of Desktop
/// and the boards, with the hint that says what "open" will do with it.
///
/// Nothing here says emu or sim (D41): a board is just a board, and what a
/// *device* is is the device's own business. The hint is the one place the
/// consequence is spelled out, because "hardware" on a project is a new
/// idea and the reader's next question is what happens when they press
/// Open.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn HardwareRow(uid: String, target: Option<String>, on_action: EventHandler<UiAction>) -> Element {
    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:gap-1",
            div { class: "tw:flex tw:min-w-0 tw:items-center tw:justify-between tw:gap-3 tw:text-xs tw:leading-snug",
                span { class: "tw:flex-none tw:font-bold tw:text-subtle-foreground", "Hardware" }
                HardwarePickPopover { uid, target, on_action }
            }
            p { class: "tw:m-0 tw:text-[11px] tw:leading-relaxed tw:text-dim-foreground",
                "The hardware this project runs on. Open starts an emu of it in this tab \
                 (a sim when no emulator exists yet); to put it on a real board, use that \
                 board's card."
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
    use lpa_studio_core::{UiConfigSlot, UiSlotValue};

    use super::*;

    #[test]
    fn the_nodes_map_collapses_to_a_count() {
        assert_eq!(node_count_label(&nodes_row(0)), "0 nodes");
        assert_eq!(node_count_label(&nodes_row(1)), "1 node");
        assert_eq!(node_count_label(&nodes_row(4)), "4 nodes");
    }

    fn nodes_row(count: usize) -> UiConfigSlot {
        let fields = (0..count)
            .map(|index| {
                UiConfigSlot::value(
                    format!("node{index}"),
                    format!("node{index}"),
                    UiSlotValue::string("x"),
                )
            })
            .collect();
        UiConfigSlot::record("nodes", "Nodes", fields)
    }
}
