//! The add-node kind picker: one popover listing every instantiable kind.
//!
//! Renders controller-produced [`UiAddNodeMenu`] data (authoring P4): each
//! entry carries a ready-to-dispatch create action, so a row click is a
//! plain dispatch — the renderer never assembles ops. One shared component
//! serves every add surface: the node tree's "Add node…" row, the
//! workspace's add button ([`WorkspaceAddNodeButton`]) — both attach at the
//! project root — and the playlist strip's add chip (attach = that
//! playlist). Deliberately one flat popover, no submenu: the source
//! dimension (blank/copy/import/examples) grows inside this panel — the
//! "Paste node" row is its first inhabitant, and "Import pattern" (module
//! authoring unit, P5) its second.
//!
//! Both extra sources are sections in the same flat list, not a submenu:
//! the whole reason the picker is one panel is that "what am I adding" and
//! "where is it coming from" are one decision, and a submenu would make the
//! second one cost an extra gesture.

use dioxus::prelude::*;
use lpa_studio_core::{
    NODE_KIND, NodePasteOp, ProjectController, UiAction, UiAddNodeMenu, UiAddNodeMenuEntry,
    UiAttachTarget, peek_header,
};

use crate::base::{
    DetailPopover, DetailSection, PopoverCloseHandle, PopoverPlacement, StudioIcon, StudioIconName,
    node_kind_icon,
};
use crate::core::menu_item_action_class;

/// The kind picker behind an arbitrary trigger (the `DetailPopover`
/// custom-trigger mode): a vertical menu of the menu's entries — kind glyph
/// plus label — where one click dispatches the entry's create action.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn AddNodePicker(
    menu: UiAddNodeMenu,
    /// Trigger content (presentational; the popover base owns the button).
    trigger: Element,
    /// Button class for the trigger at rest.
    trigger_class: String,
    /// Button class for the trigger while open.
    trigger_open_class: String,
    /// Accessible label/tooltip for the trigger.
    label: String,
    #[props(default = PopoverPlacement::BottomEnd)] placement: PopoverPlacement,
    /// Open the picker immediately (stories only).
    #[props(default = false)]
    initially_open: bool,
    on_action: EventHandler<UiAction>,
) -> Element {
    rsx! {
        DetailPopover {
            icon: StudioIconName::Add,
            label,
            placement,
            initially_open,
            trigger: Some(trigger),
            trigger_class,
            trigger_open_class,
            // Every add trigger is a glyph PLUS a label, so its top-layer
            // copy must keep the trigger's own box (the default centred-glyph
            // treatment would re-flow the two children on open).
            layer_keeps_layout: true,
            DetailSection { title: "Add node",
                div { class: "tw:grid tw:gap-0.5",
                    for entry in menu.entries.clone() {
                        AddNodeMenuRow { entry, on_action }
                    }
                }
            }
            // The second source: a node someone copied, here or elsewhere.
            // The clipboard read is async and permission-gated, so this
            // cannot be a controller-built action like the kind rows —
            // the edge reads, then dispatches.
            DetailSection { title: "From clipboard",
                div { class: "tw:grid tw:gap-0.5",
                    PasteNodeMenuRow { attach: menu.attach.clone(), on_action }
                }
            }
            // The third source: a pattern already in your library, vendored
            // in as your own copy. Absent entirely on menus that are not an
            // import site (a playlist's, this round) — the controller says
            // so by leaving both the rows AND the empty-state reason unset.
            if !menu.imports.is_empty() || menu.imports_empty.is_some() {
                DetailSection { title: "Import pattern",
                    div { class: "tw:grid tw:gap-0.5",
                        for entry in menu.imports.clone() {
                            AddNodeMenuRow { entry, on_action }
                        }
                        if let Some(reason) = menu.imports_empty.clone() {
                            EmptySourceRow { reason }
                        }
                    }
                }
            }
        }
    }
}

/// The empty-state row of a source that has nothing to offer: the same menu
/// row, disabled, saying why. A source that vanished when empty would leave
/// a hole where an affordance was, and no answer to "where did import go?"
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn EmptySourceRow(reason: String) -> Element {
    rsx! {
        button {
            class: "{menu_item_action_class()} tw:opacity-55",
            r#type: "button",
            disabled: true,
            span { class: "tw:inline-flex tw:h-[15px] tw:w-[15px] tw:items-center tw:justify-center", aria_hidden: "true",
                StudioIcon { name: StudioIconName::Add, size: 14 }
            }
            span { "{reason}" }
        }
    }
}

/// "Paste node": read the clipboard, and dispatch a paste if it holds an
/// `lp.node` envelope. A clipboard holding something else (or a denied
/// read) logs and does nothing — the row cannot know in advance, because
/// checking would itself need the permission-gated read.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PasteNodeMenuRow(attach: UiAttachTarget, on_action: EventHandler<UiAction>) -> Element {
    let close = try_consume_context::<PopoverCloseHandle>();
    rsx! {
        button {
            class: menu_item_action_class(),
            r#type: "button",
            title: "Create a node from a copied node on the clipboard.",
            onclick: move |event| {
                event.stop_propagation();
                paste_node_from_clipboard(attach.clone(), on_action);
                if let Some(mut close) = close {
                    close.close();
                }
            },
            span { class: "tw:inline-flex tw:h-[15px] tw:w-[15px] tw:items-center tw:justify-center", aria_hidden: "true",
                StudioIcon { name: StudioIconName::Copy, size: 14 }
            }
            span { "Paste node" }
        }
    }
}

/// Read the clipboard and dispatch the paste. Kept beside the row so the
/// classification rule — only `lp.node` envelopes paste here — is next to
/// the affordance that relies on it.
fn paste_node_from_clipboard(attach: UiAttachTarget, on_action: EventHandler<UiAction>) {
    crate::clipboard::read_text(move |text| match peek_header(&text) {
        Ok(header) if header.kind == NODE_KIND => {
            on_action.call(UiAction::from_op(
                ProjectController::NODE_ID,
                NodePasteOp {
                    envelope: text,
                    attach,
                },
            ));
        }
        Ok(header) => log::warn!(
            "paste node: the clipboard holds a {} envelope, not a node",
            header.kind
        ),
        Err(error) => log::warn!("paste node: {error}"),
    });
}

/// The workspace variant: a dashed card-family "Add node" button at the end
/// of the node-card column (and the empty project's call to action), opening
/// the same picker. The affordance lives where people look for it — beside
/// the node cards — mirroring the node tree's "Add node…" row.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn WorkspaceAddNodeButton(
    menu: UiAddNodeMenu,
    /// Open the picker immediately (stories only).
    #[props(default = false)]
    initially_open: bool,
    on_action: EventHandler<UiAction>,
) -> Element {
    const REST: &str = "tw:inline-flex tw:cursor-pointer tw:appearance-none tw:items-center tw:gap-2 tw:justify-self-start tw:rounded-md tw:border tw:border-dashed tw:border-border-subtle tw:bg-transparent tw:px-3 tw:py-2 tw:text-sm tw:text-subtle-foreground tw:hover:bg-card-muted tw:hover:text-soft-foreground";

    rsx! {
        AddNodePicker {
            menu,
            trigger: rsx! {
                StudioIcon { name: StudioIconName::Add, size: 15 }
                span { "Add node" }
            },
            trigger_class: REST.to_string(),
            trigger_open_class: format!("{REST} tw:bg-card-muted tw:text-soft-foreground"),
            label: "Add a node to this project".to_string(),
            placement: PopoverPlacement::BottomStart,
            initially_open,
            on_action,
        }
    }
}

/// One picker row: kind glyph + label, dispatching the entry's create
/// action and closing the popover (a selection is a completed gesture). A
/// bespoke row (not `ActionButton { variant: MenuItem }`) only because the
/// glyph is the KIND's, which is outside the `ActionMeta` icon vocabulary —
/// the classes and dispatch shape are the shared menu-row ones
/// (`package_card`'s export-zip precedent).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn AddNodeMenuRow(entry: UiAddNodeMenuEntry, on_action: EventHandler<UiAction>) -> Element {
    let icon = node_kind_icon(&entry.icon);
    // A kind the connected device cannot run stays in the list, DISABLED,
    // annotated with why — hiding it would teach a false catalog.
    let unavailable = entry.unavailable.clone();
    let title = match &unavailable {
        Some(reason) => format!("{reason} — {}", entry.action.meta().summary),
        None => entry.action.meta().summary.clone(),
    };
    let class = match unavailable {
        Some(_) => format!("{} tw:opacity-55", menu_item_action_class()),
        None => menu_item_action_class().to_string(),
    };
    let annotation = entry.unavailable.clone();
    let action = entry.action.clone();
    let disabled = annotation.is_some();
    let close = try_consume_context::<PopoverCloseHandle>();

    rsx! {
        button {
            class: "{class}",
            r#type: "button",
            title: "{title}",
            disabled,
            onclick: move |event| {
                event.stop_propagation();
                on_action.call(action.clone());
                if let Some(mut close) = close {
                    close.close();
                }
            },
            span { class: "tw:inline-flex tw:h-[15px] tw:w-[15px] tw:items-center tw:justify-center tw:text-subtle-foreground", aria_hidden: "true",
                StudioIcon { name: icon, size: 14 }
            }
            span { "{entry.label}" }
            if let Some(annotation) = annotation {
                span { class: "tw:ml-auto tw:pl-3 tw:whitespace-nowrap tw:text-[11px] tw:text-dim-foreground",
                    "{annotation}"
                }
            }
        }
    }
}
