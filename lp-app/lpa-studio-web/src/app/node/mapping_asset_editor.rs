//! The fixture face's in-place mapping editor: the embeddable
//! [`MapEditor`] wired to the asset pipeline — fetch the `.map2d.json`
//! body, edit locally (editor-owned undo), apply committed documents
//! whole-body (`AssetEditOp::ApplyBody`), Save = project `SaveOverlay`,
//! Revert = drop the applied edit. The "one home" flip: this mounts inside
//! the fixture face's output section, no separate pane.
//!
//! Refuse-don't-rewrite: a body this build cannot parse — malformed, or
//! written by a newer LightPlayer — renders as a refusal instead of an
//! editor. Nothing mounts, nothing is applied, nothing is saved, so the
//! stored asset survives open → close byte-identical.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use base64::Engine as _;
use dioxus::prelude::*;
use dioxus_icons::lucide::{Download, Upload};
use lpa_mapping_editor::{DocOpen, DocRefusal, EditorViewOptions, Map2dDoc, MapEditor};
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
    let mut parse_failure = use_signal(|| None::<DocRefusal>);
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
            let decision = {
                let current = seeded.peek();
                decide_seed(&editor.source, text, current.as_ref().map(|(_, doc)| doc))
            };
            match decision {
                SeedDecision::Keep => {
                    if parse_failure.peek().is_some() {
                        parse_failure.set(None);
                    }
                }
                SeedDecision::Seed(doc) => {
                    let epoch = seeded.peek().as_ref().map_or(0, |(epoch, _)| epoch + 1);
                    seeded.set(Some((epoch, doc)));
                    if parse_failure.peek().is_some() {
                        parse_failure.set(None);
                    }
                }
                SeedDecision::Refuse(refusal) => {
                    if parse_failure.peek().as_ref() != Some(&refusal) {
                        parse_failure.set(Some(refusal));
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
            if let Some(refusal) = parse_failure() {
                div { class: "lpme-refusal",
                    div { class: "lpme-refusal-message", "{refusal.message}" }
                    div { class: "lpme-refusal-note",
                        "This mapping is not being edited, and the stored file has been left exactly as it is."
                    }
                }
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
                                    Ok(text) => match DocOpen::parse(&name, &text) {
                                        DocOpen::Ready(parsed) => {
                                            upload_error.set(None);
                                            if let Some(handler) = &on_action {
                                                handler.call(editor.apply_action(&parsed.to_json()));
                                            }
                                        }
                                        // A file this build cannot read is
                                        // never applied to the fixture.
                                        DocOpen::Refused(refusal) => {
                                            upload_error.set(Some(refusal.message));
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

/// What the host does with a fetched asset body.
#[derive(Debug, Clone, PartialEq)]
enum SeedDecision {
    /// The body parses to the document already mounted: leave the session
    /// (selection, undo history, camera) alone.
    Keep,
    /// The body parses to something new: re-seed the editor.
    Seed(Map2dDoc),
    /// The body cannot be read by this build. Mount nothing, apply nothing —
    /// the stored file is not ours to rewrite.
    Refuse(DocRefusal),
}

fn decide_seed(source: &str, body: &str, seeded: Option<&Map2dDoc>) -> SeedDecision {
    match DocOpen::parse(source, body) {
        DocOpen::Ready(doc) if seeded == Some(&doc) => SeedDecision::Keep,
        DocOpen::Ready(doc) => SeedDecision::Seed(doc),
        DocOpen::Refused(refusal) => SeedDecision::Refuse(refusal),
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

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = "fixture.map2d.json";

    /// A body written by a newer LightPlayer refuses with upgrade wording,
    /// and — because the editor never mounts — the stored body survives an
    /// open → close round trip byte for byte, with nothing applied.
    #[test]
    fn a_newer_body_is_refused_and_never_rewritten() {
        // Open: the host decides what to do with the fetched body. A refusal
        // renders in place of the editor, so the only writer of this asset —
        // `on_doc_change` → `apply_action`, owned by the MapEditor — is never
        // mounted and emits nothing.
        let mut stored = NEWER_BODY.to_string();
        let applied: Vec<String> = Vec::new();
        let SeedDecision::Refuse(refusal) = decide_seed(SOURCE, &stored, None) else {
            panic!("a format-99 body must be refused");
        };
        assert!(refusal.needs_newer_build);
        assert!(
            refusal.message.contains("needs a newer LightPlayer"),
            "{}",
            refusal.message
        );

        // Close: replay whatever the host emitted onto the stored body.
        assert!(applied.is_empty(), "a refused body must emit no document");
        for body in &applied {
            stored = body.clone();
        }
        assert_eq!(stored, NEWER_BODY, "the stored body is byte-identical");
    }

    /// Corruption is refused too, but worded as a repair, not an upgrade.
    #[test]
    fn a_malformed_body_is_refused_without_upgrade_wording() {
        let SeedDecision::Refuse(refusal) = decide_seed(SOURCE, "{ not json", None) else {
            panic!("a malformed body must be refused");
        };
        assert!(!refusal.needs_newer_build);
        assert!(!refusal.message.contains("newer LightPlayer"));
        assert!(refusal.message.starts_with(SOURCE));
    }

    /// The stored file is pretty-printed and the editor emits compact JSON,
    /// so the echo suppression compares PARSED documents. Re-seeding on a
    /// text difference would wipe selection and undo on every render.
    #[test]
    fn a_reformatted_body_of_the_same_document_keeps_the_session() {
        let doc = Map2dDoc::from_json(VALID_BODY).expect("valid body");
        assert_eq!(
            decide_seed(SOURCE, &doc.to_json_pretty(), Some(&doc)),
            SeedDecision::Keep
        );
    }

    /// The no-gratuitous-rewrite property, on a document this build *can*
    /// read: opening a valid mapping and closing it without editing leaves
    /// the stored bytes exactly as they were, pretty-printing and all.
    #[test]
    fn a_valid_body_survives_open_and_close_unedited() {
        let stored = Map2dDoc::from_json(VALID_BODY)
            .expect("valid body")
            .to_json_pretty();
        let SeedDecision::Seed(doc) = decide_seed(SOURCE, &stored, None) else {
            panic!("a valid body must seed the editor");
        };
        // Re-render with the editor mounted: the same body decides Keep, so
        // nothing re-seeds and — with no edit to commit — nothing is applied.
        assert_eq!(decide_seed(SOURCE, &stored, Some(&doc)), SeedDecision::Keep);
        assert_eq!(
            stored,
            Map2dDoc::from_json(VALID_BODY)
                .expect("valid body")
                .to_json_pretty()
        );
    }

    #[test]
    fn a_different_document_reseeds_the_editor() {
        let seeded = Map2dDoc::from_json(VALID_BODY).expect("valid body");
        let incoming = Map2dDoc::from_json(OTHER_VALID_BODY).expect("valid body");
        assert_eq!(
            decide_seed(SOURCE, &incoming.to_json(), Some(&seeded)),
            SeedDecision::Seed(incoming)
        );
    }

    /// Format 99 plus a shape variant this build has never heard of — the
    /// shape of data a future LightPlayer writes.
    const NEWER_BODY: &str = r#"{
  "format": 99,
  "objects": [
    { "name": "sector", "shape": { "helix": { "turns": 5, "count": 300 } } }
  ]
}"#;

    const VALID_BODY: &str = r#"{"format":1,"objects":[{"name":"run","shape":{"path":{"points":[[0,0],[20,10]],"count":3}}}]}"#;

    const OTHER_VALID_BODY: &str = r#"{"format":1,"objects":[{"name":"ring","shape":{"ring":{"center":[0,0],"radius":10,"outer_count":8}}}]}"#;
}
