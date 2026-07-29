//! The fixture face's in-place mapping editor: the embeddable
//! [`MapEditor`] wired to the asset pipeline — fetch the `.map2d.json`
//! body, edit locally (editor-owned undo), apply committed documents
//! whole-body (`AssetEditOp::ApplyBody`), Save = project `SaveOverlay`,
//! Revert = drop the applied edit. The "one home" flip: this mounts inside
//! the fixture face's output section, no separate pane.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use base64::Engine as _;
use dioxus::prelude::*;
use dioxus_icons::lucide::{Download, Upload};
use lpa_mapping_editor::{EditorViewOptions, Map2dDoc, MapEditor};
use lpa_studio_core::{UiAction, UiAssetEditor};

use crate::base::icon::{StudioIcon, StudioIconName};

/// Monotonic ids for the hidden upload inputs (one per mounted editor).
static NEXT_UPLOAD_INPUT_ID: AtomicU64 = AtomicU64::new(0);

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn MappingAssetEditor(
    editor: UiAssetEditor,
    /// Face-owned view options (the output section's toggle bar).
    #[props(default)]
    shared_view: Option<Signal<EditorViewOptions>>,
    /// Live lamp colors by wiring index (the face's control preview feed).
    #[props(default)]
    live_colors: Vec<[u8; 3]>,
    /// Bumped by the face to request a zoom-to-fit (full-page expand).
    #[props(default)]
    refit_epoch: u64,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    // One-shot base-body fetch per artifact (the code editor's guard).
    let fetch_requested = use_hook(|| Rc::new(RefCell::new(None::<String>)));
    if editor.content.is_none() {
        let uri = editor.artifact.to_uri();
        let mut requested = fetch_requested.borrow_mut();
        if requested.as_deref() != Some(uri.as_str()) {
            *requested = Some(uri);
            if let Some(handler) = on_action {
                let fetch = editor.fetch_action();
                spawn(async move {
                    handler.call(fetch);
                });
            }
        }
    }

    // Seed / re-seed the editor from pipeline content, suppressing the echo
    // of our own applies so in-editor undo history survives its round-trip.
    let mut seeded = use_signal(|| None::<(u64, Map2dDoc)>);
    let last_applied = use_hook(|| Rc::new(RefCell::new(None::<String>)));
    let mut parse_failure = use_signal(|| None::<String>);
    let content_text = editor
        .content
        .as_ref()
        .and_then(|content| content.text().map(str::to_string));
    if let Some(text) = &content_text {
        let echo = last_applied.borrow().as_deref() == Some(text.as_str());
        if !echo {
            // Compare PARSED documents, never serialized text: the stored
            // file is pretty-printed while the editor emits compact JSON,
            // so text comparison never matches and every render would bump
            // the epoch — re-seeding the session (wiping selection/undo)
            // and re-arming fit on every host re-render (per-frame, now
            // that the live color feed re-renders this component).
            // All signal writes below are guarded: this runs during render.
            match Map2dDoc::from_json(text) {
                Ok(doc) => {
                    let same = seeded
                        .peek()
                        .as_ref()
                        .is_some_and(|(_, current)| *current == doc);
                    if !same {
                        let epoch = seeded.peek().as_ref().map_or(0, |(epoch, _)| epoch + 1);
                        seeded.set(Some((epoch, doc)));
                    }
                    if parse_failure.peek().is_some() {
                        parse_failure.set(None);
                    }
                }
                Err(error) => {
                    let message = error.to_string();
                    if parse_failure.peek().as_deref() != Some(message.as_str()) {
                        parse_failure.set(Some(message));
                    }
                }
            }
        }
    }

    let dirty = editor.content.as_ref().is_some_and(|content| content.dirty);
    let apply_editor = editor.clone();
    let apply_last = Rc::clone(&last_applied);
    let on_doc_change = move |json: String| {
        *apply_last.borrow_mut() = Some(json.clone());
        if let Some(handler) = &on_action {
            handler.call(apply_editor.apply_action(&json));
        }
    };

    // File in/out: mappings are worth keeping as local files. Download is a
    // data-URL of the current (applied) body, pretty-printed; upload parses
    // the picked file and applies it whole-body — the editor re-seeds from
    // the pipeline echo like any external change.
    let upload_input_id = use_hook(|| {
        format!(
            "lpme-upload-{}",
            NEXT_UPLOAD_INPUT_ID.fetch_add(1, Ordering::Relaxed)
        )
    });
    let mut upload_error = use_signal(|| None::<String>);
    let download_href = content_text.as_ref().map(|text| {
        let pretty = Map2dDoc::from_json(text)
            .map(|doc| doc.to_json_pretty())
            .unwrap_or_else(|_| text.clone());
        format!(
            "data:application/json;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(pretty.as_bytes())
        )
    });
    let upload_editor = editor.clone();

    rsx! {
        div { class: "lpme-face-editor",
            if let Some(failure) = parse_failure() {
                div { class: "lpme-error", "{editor.source}: {failure}" }
            } else if let Some((epoch, doc)) = seeded() {
                MapEditor {
                    doc_epoch: epoch,
                    doc,
                    shared_view,
                    live_colors: live_colors.clone(),
                    refit_epoch,
                    on_doc_change,
                }
                div { class: "lpme-face-editor-bar",
                    span { class: "lpme-status", "{editor.source}" }
                    if editor.in_flight {
                        span { class: "lpme-status", "applying…" }
                    }
                    if let Some(failure) = &editor.failure {
                        span { class: "lpme-face-editor-failure", "{failure}" }
                    }
                    if let Some(failure) = upload_error() {
                        span { class: "lpme-face-editor-failure", "{failure}" }
                    }
                    div { class: "lpme-spacer" }
                    button {
                        class: "lpme-btn",
                        title: "load a .map2d.json from disk (applies to this fixture)",
                        onclick: {
                            let input_id = upload_input_id.clone();
                            move |_| click_element(&input_id)
                        },
                        Upload { size: 13 }
                    }
                    if let Some(href) = &download_href {
                        a {
                            class: "lpme-btn",
                            title: "download the current mapping document",
                            href: "{href}",
                            download: "{editor.source}",
                            Download { size: 13 }
                        }
                    }
                    input {
                        id: "{upload_input_id}",
                        class: "lpme-hidden-input",
                        r#type: "file",
                        accept: ".json,application/json",
                        onchange: move |evt| {
                            let file = evt.files().first().cloned();
                            let editor = upload_editor.clone();
                            async move {
                                let Some(file) = file else { return };
                                let name = file.name();
                                match file.read_string().await {
                                    Ok(text) => match Map2dDoc::from_json(&text) {
                                        Ok(parsed) => {
                                            upload_error.set(None);
                                            if let Some(handler) = &on_action {
                                                handler.call(editor.apply_action(&parsed.to_json()));
                                            }
                                        }
                                        Err(error) => {
                                            upload_error.set(Some(format!("{name}: {error}")));
                                        }
                                    },
                                    Err(error) => {
                                        upload_error.set(Some(format!("could not read {name}: {error}")));
                                    }
                                }
                            }
                        },
                    }
                    span {
                        class: if dirty { "lpme-status lpme-dirty" } else { "lpme-status" },
                        if dirty { "Unsaved" } else { "Saved" }
                    }
                    button {
                        class: "lpme-btn",
                        title: "discard the applied edit and return to the saved file",
                        disabled: !dirty,
                        onclick: {
                            let editor = editor.clone();
                            let last = Rc::clone(&last_applied);
                            move |_| {
                                *last.borrow_mut() = None;
                                if let Some(handler) = &on_action {
                                    handler.call(editor.revert_action());
                                }
                            }
                        },
                        StudioIcon { name: StudioIconName::Revert, size: 13 }
                        "Revert"
                    }
                    button {
                        class: "lpme-btn",
                        title: "save the project (writes the applied mapping to disk)",
                        disabled: !dirty,
                        onclick: move |_| {
                            if let Some(handler) = &on_action {
                                handler.call(save_overlay_action());
                            }
                        },
                        StudioIcon { name: StudioIconName::Save, size: 13 }
                        "Save"
                    }
                }
            } else {
                div { class: "lpme-face-editor-loading", "loading {editor.source}…" }
            }
        }
    }
}

/// Click a DOM element by id (file dialogs only open from a user gesture,
/// which the bar button provides).
fn click_element(id: &str) {
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

fn save_overlay_action() -> UiAction {
    use lpa_studio_core::{ControllerId, ProjectController, ProjectOp};
    UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        ProjectOp::SaveOverlay,
    )
}
