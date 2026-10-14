//! The home page's ways in for a project file, page-wide: a `.zip` dropped
//! anywhere on the page, a project envelope pasted anywhere (Cmd-V), and
//! the hidden file input the add row's Import opens.
//!
//! Moved from the Projects page with its behaviour unchanged (PD8). The
//! wrapper is always there, so the page under it never remounts when the
//! library comes and goes; while the library is not available it listens
//! to nothing, and a drop is the browser's again.

use dioxus::html::HasFileData;
use dioxus::prelude::*;
use lpa_studio_core::{HomeOp, UiAction, ZipBytes};

use crate::app::home::gallery_paste::install_paste_listener;
use crate::app::home::package_card::home_action;

/// The hidden file input's id: the add row's Import opens it.
pub(crate) const IMPORT_INPUT_ID: &str = "home-import-zip";

/// The page's root element: drop a zip anywhere on it, paste an envelope
/// anywhere.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn LibraryDrop(
    /// The root element's own classes (the page's layout); the drop
    /// overlay is positioned over it.
    class: &'static str,
    /// The library is mounted: imports have somewhere to go.
    enabled: bool,
    on_action: EventHandler<UiAction>,
    children: Element,
) -> Element {
    let mut drag_active = use_signal(|| 0_i32);
    let import_dropped = import_handler(on_action);
    let import_picked = import_dropped.clone();

    rsx! {
        div {
            class: "tw:relative {class}",
            // drag-anywhere zip import (D2: files exist at the edges)
            ondragover: move |event| {
                if enabled {
                    event.prevent_default();
                }
            },
            ondragenter: move |event| {
                if enabled {
                    event.prevent_default();
                    drag_active += 1;
                }
            },
            ondragleave: move |_| {
                if enabled {
                    drag_active -= 1;
                }
            },
            ondrop: move |event| {
                if enabled {
                    event.prevent_default();
                    drag_active.set(0);
                    import_dropped(event.files());
                }
            },
            {children}
            if enabled {
                // Cmd-V anywhere on the page installs a pasted project
                // envelope. The listener declines every paste that is not
                // one — including pastes aimed at a text field — so
                // ordinary typing is untouched (see `gallery_paste`).
                PasteListener { on_action }
                input {
                    class: "tw:hidden",
                    id: IMPORT_INPUT_ID,
                    r#type: "file",
                    accept: ".zip",
                    onchange: move |event| import_picked(event.files()),
                }
            }
            if enabled && drag_active() > 0 {
                div { class: "tw:pointer-events-none tw:absolute tw:inset-0 tw:z-10 tw:grid tw:place-items-center tw:rounded-md tw:border-2 tw:border-dashed tw:border-selection-border tw:bg-background/80",
                    p { class: "tw:m-0 tw:text-base tw:font-semibold tw:text-strong-foreground",
                        "Drop a project zip, or paste a project JSON envelope"
                    }
                }
            }
        }
    }
}

/// The page's paste listener, for as long as this is mounted: installed
/// once, removed with it.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PasteListener(on_action: EventHandler<UiAction>) -> Element {
    let _listener = use_hook(move || install_paste_listener(on_action));
    rsx! {}
}

/// Forward the Import button to the hidden file input (a file dialog
/// cannot be a `UiAction`; the button still wears the shared quiet chip).
#[cfg(target_arch = "wasm32")]
pub(crate) fn open_import_picker() {
    use wasm_bindgen::JsCast;
    if let Some(input) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id(IMPORT_INPUT_ID))
        .and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok())
    {
        input.click();
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn open_import_picker() {}

/// Read every dropped or picked `.zip` and dispatch it as an import.
fn import_handler(
    on_action: EventHandler<UiAction>,
) -> impl Fn(Vec<dioxus::html::FileData>) + Clone + 'static {
    move |files: Vec<dioxus::html::FileData>| {
        spawn(async move {
            for file in files {
                let name = file.name();
                if !is_zip_name(&name) {
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

/// Whether a dropped or picked file is a project archive: its name ends in
/// `.zip`, in any case.
fn is_zip_name(name: &str) -> bool {
    name.to_lowercase().ends_with(".zip")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_zip_names_are_read() {
        assert!(is_zip_name("porch-sign.zip"));
        assert!(is_zip_name("PORCH.ZIP"));
        assert!(!is_zip_name("porch-sign.json"));
        assert!(!is_zip_name("zip"));
        assert!(!is_zip_name("archive.zip.txt"));
    }
}
