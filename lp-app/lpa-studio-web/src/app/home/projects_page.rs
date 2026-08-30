//! The Projects page (`#/projects`, vision D9): the package library —
//! the project half of the old combined gallery, moved unchanged in the
//! P09 page split. New / Import / Paste header actions, page-wide
//! zip-drop and Cmd-V paste ride along verbatim.

use dioxus::html::HasFileData;
use dioxus::prelude::*;
use lpa_studio_core::{HomeOp, UiAction, UiHomeView, ZipBytes};

use crate::app::home::gallery_paste::{install_paste_listener, paste_from_clipboard};
use crate::app::home::gallery_preview::HoveredCard;
use crate::app::home::new_project_menu::NewProjectMenu;
use crate::app::home::package_card::{PackageCard, home_action};
use crate::app::home::{card_grid_class, section_title_class};
use crate::app::share::ArchivedProjectsSection;
use crate::base::{StudioIcon, StudioIconName};
use crate::core::quiet_action_class;

/// The project library. "New" keeps the open-in-sim behavior (this is
/// the library page, not the device flow).
///
/// ⚠️ The "Put on \<name\>" rows a connected-EMPTY board grew on every
/// card (state-flow §1-A — the library IS the chooser for a freshly
/// set-up board) went with M2 of the device-model rebuild.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ProjectsPage(
    home: UiHomeView,
    /// Fixed clock for stories; `None` uses the platform clock.
    #[props(default)]
    now_secs: Option<f64>,
    on_action: EventHandler<UiAction>,
) -> Element {
    // Hover-to-play is page-scoped: one signal names one hovered card, so
    // the whole grid holds at most one live lease at a time.
    use_context_provider(|| HoveredCard(Signal::new(None)));
    let mut drag_active = use_signal(|| 0_i32);
    // Cmd-V anywhere on the page installs a pasted project envelope.
    // The listener declines every paste that is not one — including
    // pastes aimed at a text field — so ordinary typing is untouched
    // (see `gallery_paste`).
    let _paste_listener = use_hook(move || install_paste_listener(on_action));
    let busy = home.opening.is_some();
    let import_dropped = import_handler(on_action);
    let import_picked = import_dropped.clone();

    rsx! {
        div {
            class: "tw:relative tw:grid tw:content-start tw:gap-7",
            // drag-anywhere zip import (D2: files exist at the edges)
            ondragover: move |event| event.prevent_default(),
            ondragenter: move |event| {
                event.prevent_default();
                drag_active += 1;
            },
            ondragleave: move |_| drag_active -= 1,
            ondrop: move |event| {
                event.prevent_default();
                drag_active.set(0);
                import_dropped(event.files());
            },

            section { class: "tw:grid tw:gap-3",
                header { class: "tw:flex tw:items-baseline tw:justify-between tw:gap-3",
                    h2 { class: section_title_class(), "Projects" }
                    if home.library_available {
                        div { class: "tw:flex tw:items-center tw:gap-2",
                            // "New": pick a template, then create-and-open
                            // (2026-07-27 deviation from D17 — see the ADR at
                            // docs/adr/2026-07-27-node-authoring-operations.md;
                            // the template menu is the module authoring
                            // unit's P4)
                            NewProjectMenu { busy, on_action }
                            // a real button (matching the ActionButton quiet
                            // chip exactly) that forwards to the hidden file
                            // input — a file dialog can't be a UiAction
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
                            input {
                                class: "tw:hidden",
                                id: "home-import-zip",
                                r#type: "file",
                                accept: ".zip",
                                onchange: move |event| import_picked(event.files()),
                            }
                            // Cmd-V anywhere on the page does this too (see
                            // `gallery_paste`); the button covers the cases
                            // where clipboard permission or focus does not
                            // deliver the event.
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
                if home.library_available {
                    if home.projects.is_empty() {
                        // create-first since the D17 deviation (2026-07-27):
                        // New makes a blank project; examples still seed a
                        // copy; imports arrive by button or drag
                        p { class: "tw:m-0 tw:rounded-md tw:border tw:border-dashed tw:border-border-strong tw:px-4 tw:py-5 tw:text-sm tw:text-muted-foreground",
                            "No projects yet — create a new project, or open an example from Explore (it becomes yours on the first save). You can also drop a project zip anywhere on this page, or paste a project JSON envelope."
                        }
                    } else {
                        div { class: card_grid_class(),
                            for card in home.projects.clone() {
                                // opens arrive keyed by uid (menu paths)
                                // or slug (href navigation) — match either
                                PackageCard {
                                    key: "{card.uid}",
                                    opening: home.opening.as_deref() == Some(card.uid.as_str())
                                        || home.opening.as_deref() == Some(card.slug.as_str()),
                                    busy,
                                    card,
                                    now_secs,
                                    on_action,
                                }
                            }
                        }
                    }
                } else {
                    p { class: "tw:m-0 tw:text-sm tw:text-muted-foreground",
                        "Local storage is unavailable, so there is no project library here. Examples still run."
                    }
                }
            }

            // The archive, last and collapsed (Q12). Reads the account's
            // own project list, so it renders nothing at all when signed
            // out, unreachable, or empty — an empty drawer is not news.
            ArchivedProjectsSection {}

            if drag_active() > 0 {
                div { class: "tw:pointer-events-none tw:absolute tw:inset-0 tw:z-10 tw:grid tw:place-items-center tw:rounded-md tw:border-2 tw:border-dashed tw:border-selection-border tw:bg-background/80",
                    p { class: "tw:m-0 tw:text-base tw:font-semibold tw:text-strong-foreground",
                        "Drop a project zip, or paste a project JSON envelope"
                    }
                }
            }
        }
    }
}

/// Forward the Import button to the hidden file input (a file dialog
/// cannot be a `UiAction`; the button still wears the shared quiet chip).
#[cfg(target_arch = "wasm32")]
fn open_import_picker() {
    use wasm_bindgen::JsCast;
    if let Some(input) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id("home-import-zip"))
        .and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok())
    {
        input.click();
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn open_import_picker() {}

/// Read every dropped/picked `.zip` and dispatch it as an import action.
fn import_handler(
    on_action: EventHandler<UiAction>,
) -> impl Fn(Vec<dioxus::html::FileData>) + Clone + 'static {
    move |files: Vec<dioxus::html::FileData>| {
        spawn(async move {
            for file in files {
                let name = file.name();
                if !name.to_lowercase().ends_with(".zip") {
                    log::warn!("import: skipping {name} (not a zip)");
                    continue;
                }
                match file.read_bytes().await {
                    Ok(bytes) => on_action.call(home_action(HomeOp::ImportZip {
                        file_name: name,
                        bytes: ZipBytes(bytes.to_vec()),
                    })),
                    Err(error) => log::warn!("import: could not read {name}: {error}"),
                }
            }
        });
    }
}
