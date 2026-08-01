//! The standalone editor page: [`BoardEditor`] plus load/save chrome and
//! localStorage autosave. This is the `#/boards/edit` route body; it knows
//! browser persistence and nothing about projects.
//!
//! Load paths: pick a checked-in board (embedded catalog), open/drop a
//! `.display.json`, or paste JSON. Save paths: download (data URL) or copy
//! to the clipboard — v1 loops through the filesystem on purpose; the defs
//! live in-repo.

use base64::Engine as _;
use dioxus::html::HasFileData as _;
use dioxus::prelude::*;
use lpa_boards::DISPLAY_MANIFEST_SOURCES;

use crate::editor_core::editor_doc::EditorDoc;
use crate::view::board_editor::BoardEditor;

#[cfg(target_arch = "wasm32")]
const AUTOSAVE_KEY: &str = "lp-board-editor-doc";
const OPEN_INPUT_ID: &str = "lpb-ed-open-input";

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn BoardEditorPage() -> Element {
    let mut doc = use_signal(|| autosave_load().unwrap_or_else(EditorDoc::new_template));
    let mut error = use_signal(|| None::<String>);
    let mut paste_open = use_signal(|| false);
    let mut paste_text = use_signal(String::new);
    let mut copied = use_signal(|| false);

    // Autosave every change (the effect re-runs whenever the doc signal
    // changes).
    use_effect(move || {
        let current = doc();
        autosave_store(&current.source_name, &current.export_json());
    });

    let (source_name, dirty, export, file_name) = {
        let current = doc.read();
        (
            current.source_name.clone(),
            current.dirty,
            current.export_json(),
            current.export_file_name(),
        )
    };
    let save_href = format!(
        "data:application/json;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(export.as_bytes())
    );

    let mut load = move |name: String, text: String| match EditorDoc::from_source(&name, &text) {
        Ok(parsed) => {
            doc.set(parsed);
            error.set(None);
            paste_open.set(false);
            copied.set(false);
        }
        Err(message) => error.set(Some(message)),
    };

    rsx! {
        div {
            class: "lpb-ed-page",
            // Drop a .display.json anywhere onto the editor to open it.
            ondragover: move |event| event.prevent_default(),
            ondrop: move |event| {
                event.prevent_default();
                let file = event.data().files().first().cloned();
                async move {
                    if let Some(file) = file {
                        let name = file.name();
                        match file.read_string().await {
                            Ok(text) => load(name, text),
                            Err(read_error) => {
                                error.set(Some(format!("could not read {name}: {read_error}")));
                            }
                        }
                    }
                }
            },
            header { class: "lpb-ed-header",
                h1 { "Board editor" }
                span { class: "lpb-ed-source",
                    "{source_name}"
                    if dirty {
                        span { class: "lpb-ed-dirty", title: "unsaved edits", " ●" }
                    }
                }
                span { class: "lpb-ed-spacer" }
                select {
                    class: "lpb-ed-select",
                    onchange: move |event| {
                        let picked = event.value();
                        if let Some((board_id, source)) = DISPLAY_MANIFEST_SOURCES
                            .iter()
                            .find(|(board_id, _)| *board_id == picked)
                        {
                            load((*board_id).to_string(), (*source).to_string());
                        }
                    },
                    option { value: "", selected: true, "Load a checked-in board…" }
                    for (board_id, _) in DISPLAY_MANIFEST_SOURCES {
                        option { value: "{board_id}", "{board_id}" }
                    }
                }
                button {
                    class: "lpb-ed-btn",
                    onclick: move |_| {
                        doc.set(EditorDoc::new_template());
                        error.set(None);
                    },
                    "New"
                }
                button {
                    class: "lpb-ed-btn",
                    onclick: move |_| click_open_input(),
                    "Open…"
                }
                button {
                    class: "lpb-ed-btn",
                    "aria-pressed": if paste_open() { "true" } else { "false" },
                    onclick: move |_| paste_open.set(!paste_open()),
                    "Paste"
                }
                button {
                    class: "lpb-ed-btn",
                    onclick: move |_| {
                        copy_to_clipboard(&doc.read().export_json());
                        copied.set(true);
                    },
                    if copied() { "Copied ✓" } else { "Copy JSON" }
                }
                a {
                    class: "lpb-ed-btn lpb-ed-btn--save",
                    href: "{save_href}",
                    download: "{file_name}",
                    "Save {file_name}"
                }
            }
            if let Some(message) = error() {
                div { class: "lpb-ed-error",
                    "{message}"
                    button {
                        class: "lpb-ed-btn",
                        onclick: move |_| error.set(None),
                        "dismiss"
                    }
                }
            }
            if paste_open() {
                div { class: "lpb-ed-paste",
                    textarea {
                        class: "lpb-ed-input lpb-ed-paste-text",
                        placeholder: "paste a .display.json here",
                        value: "{paste_text}",
                        oninput: move |event| paste_text.set(event.value()),
                    }
                    div { class: "lpb-ed-paste-actions",
                        button {
                            class: "lpb-ed-btn lpb-ed-btn--save",
                            onclick: move |_| load("pasted json".into(), paste_text()),
                            "Load JSON"
                        }
                        button {
                            class: "lpb-ed-btn",
                            onclick: move |_| paste_open.set(false),
                            "Cancel"
                        }
                    }
                }
            }
            BoardEditor { doc }
            input {
                id: OPEN_INPUT_ID,
                class: "lpb-ed-hidden-input",
                r#type: "file",
                accept: ".json,application/json",
                onchange: move |event| {
                    let file = event.files().first().cloned();
                    async move {
                        if let Some(file) = file {
                            let name = file.name();
                            match file.read_string().await {
                                Ok(text) => load(name, text),
                                Err(read_error) => {
                                    error
                                        .set(
                                            Some(format!("could not read {name}: {read_error}")),
                                        );
                                }
                            }
                        }
                    }
                },
            }
        }
    }
}

/// Click the hidden file input (browser file dialogs only open from a user
/// gesture, which the header button provides).
fn click_open_input() {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast;
        if let Some(element) = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.get_element_by_id(OPEN_INPUT_ID))
            && let Ok(element) = element.dyn_into::<web_sys::HtmlElement>()
        {
            element.click();
        }
    }
}

fn copy_to_clipboard(text: &str) {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(window) = web_sys::window() {
            let _ = window.navigator().clipboard().write_text(text);
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = text;
    }
}

fn autosave_load() -> Option<EditorDoc> {
    #[cfg(target_arch = "wasm32")]
    {
        let storage = web_sys::window()?.local_storage().ok()??;
        let envelope = storage.get_item(AUTOSAVE_KEY).ok()??;
        let value: serde_json::Value = serde_json::from_str(&envelope).ok()?;
        let name = value.get("name")?.as_str()?;
        let text = value.get("text")?.as_str()?;
        EditorDoc::from_source(name, text).ok()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        None
    }
}

fn autosave_store(name: &str, text: &str) {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(storage) = web_sys::window().and_then(|window| window.local_storage().ok()?) {
            let envelope = serde_json::json!({ "name": name, "text": text }).to_string();
            let _ = storage.set_item(AUTOSAVE_KEY, &envelope);
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (name, text);
    }
}
