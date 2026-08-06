//! The standalone editor page: [`MapEditor`] plus file open/save and
//! localStorage autosave. This is the `#/mapping` route body; it knows
//! browser persistence and nothing about projects.
//!
//! Refuse-don't-rewrite: a document this build cannot parse — malformed, or
//! simply newer — never reaches the editor and never gets written back. The
//! autosave slot keeps the bytes it already had until the user explicitly
//! starts fresh or opens a document that does parse. See
//! [`crate::editor_core::doc_refusal`].

use base64::Engine as _;
use dioxus::html::HasFileData as _;
use dioxus::prelude::*;
use lpc_mapping::Map2dDoc;

use crate::editor_core::doc_refusal::{DocOpen, DocRefusal};
use crate::view::map_editor::{EditorFileOps, MapEditor, ReferenceOps};
use crate::view::reference::{DEFAULT_REFERENCE_OPACITY, ReferenceImage, svg_reference_size};

#[cfg(target_arch = "wasm32")]
const AUTOSAVE_KEY: &str = "lp-mapping-editor-doc";
/// The reference image's own slot: editor-side tracing state, deliberately
/// separate from the document autosave (losing one must never touch the
/// other).
#[cfg(target_arch = "wasm32")]
const REFERENCE_KEY: &str = "lp-mapping-editor-reference";
/// What the refusal panel calls the autosave slot.
const AUTOSAVE_LABEL: &str = "autosaved document";
const OPEN_INPUT_ID: &str = "lpme-open-input";
const REFERENCE_INPUT_ID: &str = "lpme-reference-input";

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn MapEditorPage() -> Element {
    let restored = use_hook(|| autosave_restore(autosave_read()));
    let doc = use_signal(|| match &restored {
        Restored::Ready(doc) => doc.clone(),
        _ => Map2dDoc::new(),
    });
    let doc_epoch = use_signal(|| 0u64);
    let mut error = use_signal(|| None::<String>);
    // Set while the autosaved document is unreadable: the editor stays
    // unmounted so nothing can overwrite those bytes.
    let refused = use_signal(|| match &restored {
        Restored::Refused(refusal) => Some(refusal.clone()),
        _ => None,
    });

    // The reference image restores from its own slot; anything unreadable
    // there is simply "no reference" — tracing state earns no refusal flow.
    let mut reference = use_signal(|| {
        reference_read()
            .as_deref()
            .and_then(ReferenceImage::from_json)
    });

    let on_doc_change = move |json: String| {
        autosave_store(&json);
    };
    let file_ops = EditorFileOps {
        on_new: EventHandler::new(move |()| start_fresh(doc, doc_epoch, error, refused)),
        on_open: EventHandler::new(move |()| click_input(OPEN_INPUT_ID)),
    };
    let reference_ops = ReferenceOps {
        on_pick: EventHandler::new(move |()| click_input(REFERENCE_INPUT_ID)),
        on_change: EventHandler::new(move |value: Option<ReferenceImage>| {
            adopt_reference(&mut reference, error, value);
        }),
    };

    rsx! {
        div {
            class: "lpme-page",
            // Drag a .map2d.json anywhere onto the editor to open it — this
            // is also the way out of a refusal, so it stays on the page
            // element rather than the editor.
            ondragover: move |evt| evt.prevent_default(),
            ondrop: move |evt| {
                evt.prevent_default();
                let file = evt.data().files().first().cloned();
                async move {
                    if let Some(file) = file {
                        apply_file(doc, doc_epoch, error, refused, file).await;
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
            if let Some(refusal) = refused() {
                div { class: "lpme-refusal",
                    div { class: "lpme-refusal-message", "{refusal.message}" }
                    div { class: "lpme-refusal-note",
                        "The saved document has been left exactly as it was. Open a document this build can read, or start a new one."
                    }
                    button {
                        class: "lpme-btn",
                        onclick: move |_| click_input(OPEN_INPUT_ID),
                        "Open a document…"
                    }
                    button {
                        class: "lpme-btn",
                        onclick: move |_| start_fresh(doc, doc_epoch, error, refused),
                        "Discard and start new"
                    }
                }
            } else {
                MapEditor {
                    doc_epoch: doc_epoch(),
                    doc: doc(),
                    on_doc_change,
                    file_ops,
                    scene_menu: true,
                    reference: reference(),
                    reference_ops,
                }
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
                            apply_file(doc, doc_epoch, error, refused, file).await;
                        }
                    }
                },
            }
            input {
                id: REFERENCE_INPUT_ID,
                class: "lpme-hidden-input",
                r#type: "file",
                accept: ".svg,.png,.jpg,.jpeg",
                onchange: move |evt| {
                    let file = evt.files().first().cloned();
                    async move {
                        if let Some(file) = file {
                            apply_reference_file(reference, error, file).await;
                        }
                    }
                },
            }
        }
    }
}

/// Discard whatever is in the autosave slot for an empty document. The one
/// path on this page that overwrites an unreadable autosave, and only ever
/// from an explicit click.
fn start_fresh(
    mut doc: Signal<Map2dDoc>,
    mut doc_epoch: Signal<u64>,
    mut error: Signal<Option<String>>,
    mut refused: Signal<Option<DocRefusal>>,
) {
    let fresh = Map2dDoc::new();
    autosave_store(&fresh.to_json());
    doc.set(fresh);
    doc_epoch += 1;
    error.set(None);
    refused.set(None);
}

/// What the page found in the autosave slot on mount.
#[derive(Debug, Clone, PartialEq)]
enum Restored {
    /// Nothing stored (or no storage at all): start on an empty document.
    Fresh,
    Ready(Map2dDoc),
    /// Stored bytes this build cannot read. They stay put.
    Refused(DocRefusal),
}

fn autosave_restore(stored: Option<String>) -> Restored {
    match stored {
        None => Restored::Fresh,
        Some(json) => match DocOpen::parse(AUTOSAVE_LABEL, &json) {
            DocOpen::Ready(doc) => Restored::Ready(doc),
            DocOpen::Refused(refusal) => Restored::Refused(refusal),
        },
    }
}

/// Parse and adopt an opened/dropped file (shared by the picker and
/// drag-and-drop paths). A file that will not parse is reported and
/// otherwise ignored — the current document and the autosave slot are both
/// left alone.
async fn apply_file(
    mut doc: Signal<Map2dDoc>,
    mut doc_epoch: Signal<u64>,
    mut error: Signal<Option<String>>,
    mut refused: Signal<Option<DocRefusal>>,
    file: dioxus::html::FileData,
) {
    let name = file.name();
    match file.read_string().await {
        Ok(text) => match DocOpen::parse(&name, &text) {
            DocOpen::Ready(parsed) => {
                autosave_store(&parsed.to_json());
                doc.set(parsed);
                doc_epoch += 1;
                error.set(None);
                refused.set(None);
            }
            DocOpen::Refused(refusal) => {
                error.set(Some(refusal.message));
            }
        },
        Err(read_error) => {
            error.set(Some(format!("could not read {name}: {read_error}")));
        }
    }
}

/// Click a hidden file input (browser file dialogs only open from a user
/// gesture, which the header buttons provide).
fn click_input(id: &str) {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast;
        if let Some(element) = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.get_element_by_id(id))
            && let Ok(element) = element.dyn_into::<web_sys::HtmlElement>()
        {
            element.click();
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = id;
    }
}

/// Adopt a reference change (load, opacity, clear) and persist it. A slot
/// write that fails — a photo-sized data URL can blow the localStorage
/// quota — keeps the in-memory reference and says so; the document autosave
/// lives in its own slot and is never at risk here.
fn adopt_reference(
    reference: &mut Signal<Option<ReferenceImage>>,
    mut error: Signal<Option<String>>,
    value: Option<ReferenceImage>,
) {
    let stored = reference_store(value.as_ref());
    reference.set(value);
    if !stored {
        error.set(Some(
            "reference image loaded, but too large for browser storage — it will not \
             survive a reload (the document autosave is unaffected)"
                .to_string(),
        ));
    }
}

/// Read a picked reference file into doc-space: SVGs size from their
/// width/height or viewBox (the Illustrator shape), rasters from their
/// decoded pixel size. Files we cannot size are reported and ignored.
async fn apply_reference_file(
    reference: Signal<Option<ReferenceImage>>,
    mut error: Signal<Option<String>>,
    file: dioxus::html::FileData,
) {
    let mut reference = reference;
    let name = file.name();
    let lower = name.to_lowercase();
    let loaded = if lower.ends_with(".svg") {
        match file.read_string().await {
            Ok(text) => svg_reference_size(&text)
                .map(|size| ReferenceImage {
                    data_url: format!(
                        "data:image/svg+xml;base64,{}",
                        base64::engine::general_purpose::STANDARD.encode(text.as_bytes())
                    ),
                    opacity: DEFAULT_REFERENCE_OPACITY,
                    size,
                })
                .ok_or_else(|| format!("{name}: could not read an SVG size (no viewBox?)")),
            Err(read_error) => Err(format!("could not read {name}: {read_error}")),
        }
    } else if lower.ends_with(".png") || lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        let mime = if lower.ends_with(".png") {
            "image/png"
        } else {
            "image/jpeg"
        };
        match file.read_bytes().await {
            Ok(bytes) => {
                let data_url = format!(
                    "data:{mime};base64,{}",
                    base64::engine::general_purpose::STANDARD.encode(&bytes)
                );
                match decode_image_size(&data_url).await {
                    Some(size) => Ok(ReferenceImage {
                        data_url,
                        opacity: DEFAULT_REFERENCE_OPACITY,
                        size,
                    }),
                    None => Err(format!("{name}: could not decode the image")),
                }
            }
            Err(read_error) => Err(format!("could not read {name}: {read_error}")),
        }
    } else {
        Err(format!("{name}: not a .svg / .png / .jpg reference"))
    };
    match loaded {
        Ok(image) => adopt_reference(&mut reference, error, Some(image)),
        Err(message) => error.set(Some(message)),
    }
}

/// Decode a raster data URL for its natural pixel size.
async fn decode_image_size(data_url: &str) -> Option<[f32; 2]> {
    #[cfg(target_arch = "wasm32")]
    {
        let image = web_sys::HtmlImageElement::new().ok()?;
        image.set_src(data_url);
        wasm_bindgen_futures::JsFuture::from(image.decode())
            .await
            .ok()?;
        let size = [image.natural_width() as f32, image.natural_height() as f32];
        (size[0] > 0.0 && size[1] > 0.0).then_some(size)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = data_url;
        None
    }
}

fn autosave_read() -> Option<String> {
    #[cfg(target_arch = "wasm32")]
    {
        let storage = web_sys::window()?.local_storage().ok()??;
        storage.get_item(AUTOSAVE_KEY).ok()?
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

fn reference_read() -> Option<String> {
    #[cfg(target_arch = "wasm32")]
    {
        let storage = web_sys::window()?.local_storage().ok()??;
        storage.get_item(REFERENCE_KEY).ok()?
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        None
    }
}

/// Persist (or clear) the reference slot. `false` means the write itself
/// failed — in practice a quota overflow — and the caller should say so.
fn reference_store(reference: Option<&ReferenceImage>) -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        let Some(storage) = web_sys::window().and_then(|window| window.local_storage().ok()?)
        else {
            return true; // no storage at all: nothing to fail
        };
        match reference {
            Some(image) => storage.set_item(REFERENCE_KEY, &image.to_json()).is_ok(),
            None => {
                let _ = storage.remove_item(REFERENCE_KEY);
                true
            }
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = reference;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_slot_starts_fresh() {
        assert_eq!(autosave_restore(None), Restored::Fresh);
    }

    #[test]
    fn a_readable_autosave_is_restored() {
        let stored = lpc_mapping::corpus::cat_ears().to_json();
        assert_eq!(
            autosave_restore(Some(stored)),
            Restored::Ready(lpc_mapping::corpus::cat_ears())
        );
    }

    /// The autosave slot is the user's only copy on this page, so a document
    /// written by a newer build must park the editor rather than be replaced
    /// by an empty one.
    #[test]
    fn a_newer_autosave_is_refused_and_left_in_place() {
        let stored = r#"{"format":99,"objects":[
            { "name": "sector", "shape": { "helix": { "turns": 5, "count": 300 } } }
        ]}"#;
        let Restored::Refused(refusal) = autosave_restore(Some(stored.to_string())) else {
            panic!("a format-99 autosave must be refused");
        };
        assert!(refusal.needs_newer_build);
        assert!(refusal.message.contains("needs a newer LightPlayer"));
        // Restoring is a pure read: it hands back a refusal and writes
        // nothing, so the stored bytes are whatever they were.
        assert_eq!(
            autosave_restore(Some(stored.to_string())),
            Restored::Refused(refusal)
        );
    }

    #[test]
    fn a_corrupt_autosave_is_refused_too() {
        let Restored::Refused(refusal) = autosave_restore(Some("{not json".to_string())) else {
            panic!("a malformed autosave must be refused");
        };
        assert!(!refusal.needs_newer_build);
        assert!(refusal.message.starts_with(AUTOSAVE_LABEL));
    }
}
