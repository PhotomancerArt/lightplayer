//! The standalone editor page: [`MapEditor`] plus file open/save and
//! localStorage autosave. This is the `#/mapping` route body; it knows
//! browser persistence and nothing about projects.

use dioxus::html::HasFileData as _;
use dioxus::prelude::*;
use lpc_mapping::Map2dDoc;

use crate::view::map_editor::{EditorFileOps, MapEditor};

#[cfg(target_arch = "wasm32")]
const AUTOSAVE_KEY: &str = "lp-mapping-editor-doc";
const OPEN_INPUT_ID: &str = "lpme-open-input";

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn MapEditorPage() -> Element {
    let mut doc = use_signal(|| autosave_load().unwrap_or_default());
    let mut doc_epoch = use_signal(|| 0u64);
    let mut error = use_signal(|| None::<String>);

    let on_doc_change = move |json: String| {
        autosave_store(&json);
    };
    let file_ops = EditorFileOps {
        on_new: EventHandler::new(move |()| {
            let fresh = Map2dDoc::new();
            autosave_store(&fresh.to_json());
            doc.set(fresh);
            doc_epoch += 1;
            error.set(None);
        }),
        on_open: EventHandler::new(move |()| click_open_input()),
    };

    rsx! {
        div {
            class: "lpme-page",
            // Drag a .map2d.json anywhere onto the editor to open it.
            ondragover: move |evt| evt.prevent_default(),
            ondrop: move |evt| {
                evt.prevent_default();
                let file = evt.data().files().first().cloned();
                async move {
                    if let Some(file) = file {
                        apply_file(doc, doc_epoch, error, file).await;
                    }
                }
            },
            if let Some(message) = error() {
                div { class: "lpme-error",
                    "{message}"
                    button {
                        class: "lpme-btn",
                        onclick: move |_| error.set(None),
                        "dismiss"
                    }
                }
            }
            MapEditor {
                doc_epoch: doc_epoch(),
                doc: doc(),
                on_doc_change,
                file_ops,
                scene_menu: true,
            }
            input {
                id: OPEN_INPUT_ID,
                class: "lpme-hidden-input",
                r#type: "file",
                accept: ".json,application/json",
                onchange: move |evt| {
                    let file = evt.files().first().cloned();
                    async move {
                        if let Some(file) = file {
                            apply_file(doc, doc_epoch, error, file).await;
                        }
                    }
                },
            }
        }
    }
}

/// Parse and adopt an opened/dropped file (shared by the picker and
/// drag-and-drop paths).
async fn apply_file(
    mut doc: Signal<Map2dDoc>,
    mut doc_epoch: Signal<u64>,
    mut error: Signal<Option<String>>,
    file: dioxus::html::FileData,
) {
    let name = file.name();
    match file.read_string().await {
        Ok(text) => match Map2dDoc::from_json(&text) {
            Ok(parsed) => {
                autosave_store(&parsed.to_json());
                doc.set(parsed);
                doc_epoch += 1;
                error.set(None);
            }
            Err(parse_error) => {
                error.set(Some(format!("{name}: {parse_error}")));
            }
        },
        Err(read_error) => {
            error.set(Some(format!("could not read {name}: {read_error}")));
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

fn autosave_load() -> Option<Map2dDoc> {
    #[cfg(target_arch = "wasm32")]
    {
        let storage = web_sys::window()?.local_storage().ok()??;
        let json = storage.get_item(AUTOSAVE_KEY).ok()??;
        Map2dDoc::from_json(&json).ok()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        None
    }
}

fn autosave_store(json: &str) {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(storage) = web_sys::window().and_then(|window| window.local_storage().ok()?) {
            let _ = storage.set_item(AUTOSAVE_KEY, json);
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = json;
    }
}
