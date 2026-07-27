//! The add-node kind picker: one popover listing every instantiable kind.
//!
//! Renders controller-produced [`UiAddNodeMenu`] data (authoring P4): each
//! entry carries a ready-to-dispatch create action, so a row click is a
//! plain dispatch — the renderer never assembles ops. One shared component
//! serves every add surface: the node tree's "Add node…" row, the
//! workspace's add button ([`WorkspaceAddNodeButton`]) — both attach at the
//! project root — and the playlist strip's add chip (attach = that
//! playlist). Deliberately one flat popover, no submenu: the future source
//! dimension (blank/copy/import/examples) grows inside this panel.

use dioxus::prelude::*;
use lpa_studio_core::{UiAction, UiAddNodeMenu, UiAddNodeMenuEntry};

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
            DetailSection { title: "Add node",
                div { class: "tw:grid tw:gap-0.5",
                    for entry in menu.entries.clone() {
                        AddNodeMenuRow { entry, on_action }
                    }
                }
            }
        }
    }
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
    let title = entry.action.meta().summary.clone();
    let action = entry.action.clone();
    let close = try_consume_context::<PopoverCloseHandle>();

    rsx! {
        button {
            class: menu_item_action_class(),
            r#type: "button",
            title: "{title}",
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
        }
    }
}
