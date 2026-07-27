//! The add-node kind picker: one popover listing every instantiable kind.
//!
//! Renders controller-produced [`UiAddNodeMenu`] data (authoring P4): each
//! entry carries a ready-to-dispatch create action, so a row click is a
//! plain dispatch — the renderer never assembles ops. One shared component
//! serves both attach surfaces: the project header's "+" (attach = project
//! root, [`PaneAddNodePicker`]) and the playlist strip's add chip (attach =
//! that playlist). Deliberately one flat popover, no submenu: the future
//! source dimension (blank/copy/import/examples) grows inside this panel.

use dioxus::prelude::*;
use lpa_studio_core::{UiAction, UiAddNodeMenu, UiAddNodeMenuEntry};

use crate::app::layout::pane_action_button_class;
use crate::base::{
    DetailPopover, DetailSection, PopoverPlacement, StudioIcon, StudioIconName, node_kind_icon,
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

/// The project-header variant: the picker behind a trigger styled exactly
/// like the header's generic `PaneActionButton`s, so the "+" reads as one of
/// the pane's action icons while its press opens the picker instead of
/// dispatching the wrapped default create.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn PaneAddNodePicker(
    menu: UiAddNodeMenu,
    /// Accessible label/tooltip (the intercepted add action's label).
    #[props(default = "Add node".to_string())]
    label: String,
    /// Open the picker immediately (stories only).
    #[props(default = false)]
    initially_open: bool,
    on_action: EventHandler<UiAction>,
) -> Element {
    let rest_class = pane_action_button_class(false, true).to_string();
    let open_class = format!("{rest_class} tw:bg-card-subtle/60 tw:text-strong-foreground");

    rsx! {
        AddNodePicker {
            menu,
            trigger: rsx! {
                StudioIcon { name: StudioIconName::Add, size: 15 }
            },
            trigger_class: rest_class,
            trigger_open_class: open_class,
            label,
            placement: PopoverPlacement::BottomEnd,
            initially_open,
            on_action,
        }
    }
}

/// One picker row: kind glyph + label, dispatching the entry's create
/// action. A bespoke row (not `ActionButton { variant: MenuItem }`) only
/// because the glyph is the KIND's, which is outside the `ActionMeta` icon
/// vocabulary — the classes and dispatch shape are the shared menu-row ones
/// (`package_card`'s export-zip precedent).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn AddNodeMenuRow(entry: UiAddNodeMenuEntry, on_action: EventHandler<UiAction>) -> Element {
    let icon = node_kind_icon(&entry.icon);
    let title = entry.action.meta().summary.clone();
    let action = entry.action.clone();

    rsx! {
        button {
            class: menu_item_action_class(),
            r#type: "button",
            title: "{title}",
            onclick: move |event| {
                event.stop_propagation();
                on_action.call(action.clone());
            },
            span { class: "tw:inline-flex tw:h-[15px] tw:w-[15px] tw:items-center tw:justify-center tw:text-subtle-foreground", aria_hidden: "true",
                StudioIcon { name: icon, size: 14 }
            }
            span { "{entry.label}" }
        }
    }
}
