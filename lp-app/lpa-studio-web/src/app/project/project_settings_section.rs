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
//! read-only from [`UiProjectManifest`]. Rename happens where the identity
//! lives: the home gallery's rename, which patches the manifest
//! (`LibraryStore::rename`).
//!
//! - **Name / Format / UID** — read-only, from the manifest. UID keeps its
//!   copy button (identity is the thing you actually want on your clipboard
//!   when reporting a problem).
//! - **Nodes** — the one root-def row left, collapsed to a **count**. The
//!   map itself is the node tree, which is the pane's whole body; repeating
//!   it as a slot editor in the popup was noise. It carries role `Fixed` in
//!   `lpc_model::ModuleDef`, so the read-only presentation agrees with the
//!   model rather than merely hiding a writable slot.

use dioxus::prelude::*;
use lpa_studio_core::{UiConfigSlot, UiConfigSlotBody, UiProjectManifest};

use crate::base::{StudioIcon, StudioIconName};

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
) -> Element {
    let nodes = row(&root_slots, "nodes");
    let manifest = manifest.unwrap_or_default();

    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:gap-1.5",
            if let Some(name) = manifest.name {
                ReadOnlyRow { label: "Name", value: name }
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
