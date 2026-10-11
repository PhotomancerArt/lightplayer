//! The row at the end of Other projects and Projects: New · Import · Paste.
//!
//! It is the projects sections' empty state too (PQ10): a newcomer sees
//! only it under Other projects, and no sentence explains it (D13). New is
//! the template menu, pressing `project/new` from the offer tree; Import
//! opens the page's hidden file input ([`LibraryDrop`]); Paste reads the
//! clipboard. A file dialog and a clipboard read cannot be `UiAction`s, so
//! Import and Paste are plain buttons that end in the same `HomeOp`s the
//! page-wide drop and paste send.
//!
//! [`LibraryDrop`]: super::library_drop::LibraryDrop

use dioxus::prelude::*;
use lpa_studio_core::UiAction;

use super::library_drop::open_import_picker;
use crate::app::home::gallery_paste::paste_from_clipboard;
use crate::app::home::new_project_menu::NewProjectMenu;
use crate::base::{StudioIcon, StudioIconName};
use crate::core::quiet_action_class;

/// New · Import · Paste, one quiet line.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn ProjectAddRow(
    /// An open or a create is already in flight.
    busy: bool,
    /// Stories only: mount New's template menu open.
    #[props(default)]
    menu_open: bool,
    on_action: EventHandler<UiAction>,
) -> Element {
    rsx! {
        div { class: "tw:flex tw:flex-wrap tw:items-center tw:gap-2",
            // "New": pick a template, then create-and-open (2026-07-27
            // deviation from D17 — see
            // docs/adr/2026-07-27-node-authoring-operations.md).
            NewProjectMenu { busy, initially_open: menu_open, on_action }
            button {
                class: quiet_action_class(),
                r#type: "button",
                title: "Install a project from a zip archive.",
                onclick: move |_| open_import_picker(),
                span { class: "tw:inline-flex tw:h-[15px] tw:w-[15px] tw:items-center tw:justify-center", aria_hidden: "true",
                    StudioIcon { name: StudioIconName::Upload, size: 14 }
                }
                span { "Import" }
            }
            // Cmd-V anywhere on the page does this too (see
            // `gallery_paste`); the button covers the cases where clipboard
            // permission or focus does not deliver the event.
            button {
                class: quiet_action_class(),
                r#type: "button",
                title: "Install a project from a JSON envelope on the clipboard.",
                onclick: move |_| paste_from_clipboard(on_action),
                span { class: "tw:inline-flex tw:h-[15px] tw:w-[15px] tw:items-center tw:justify-center", aria_hidden: "true",
                    StudioIcon { name: StudioIconName::Copy, size: 14 }
                }
                span { "Paste" }
            }
        }
    }
}
