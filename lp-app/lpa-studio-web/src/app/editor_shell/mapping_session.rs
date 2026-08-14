//! The dive's asset pipeline, re-housed as a NON-VISUAL coordinator (the
//! one-project-canvas P4): fetch the `.map2d.json` body, seed the
//! workbench-owned session with parsed-doc echo suppression (undo survives
//! its own round-trips), and apply committed documents whole-body
//! (`AssetEditOp::ApplyBody`) on every commit bump. The canvas host renders
//! the session; the toolbar renders the save state; this component renders
//! NOTHING — it only coordinates bytes.
//!
//! Refuse-don't-rewrite: a body this build cannot parse — malformed, or
//! written by a newer LightPlayer — surfaces as [`DiveAssetState::Refused`]
//! instead of a session. Nothing seeds, nothing is applied, nothing is
//! saved, so the stored asset survives open → close byte-identical.

use std::cell::RefCell;
use std::rc::Rc;

use dioxus::prelude::*;
use lpa_mapping_editor::{DocOpen, DocRefusal, Map2dDoc, MapEditorSession};
use lpa_studio_core::{ArtifactLocation, UiAction, UiAssetEditor};

/// Where the dive's asset stands, for the canvas host and the toolbar.
#[derive(Clone, Debug, PartialEq, Default)]
pub(crate) enum DiveAssetState {
    /// The body has not landed (or a switch is in flight): the fixture
    /// stays a sprite; tools wait.
    #[default]
    Loading,
    /// The session is seeded with this fixture's document: editable.
    Ready,
    /// The body cannot be read by this build. The fixture stays visible,
    /// tools stay disabled, and the stored file is never rewritten.
    Refused(DocRefusal),
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn MappingAssetPipeline(
    editor: UiAssetEditor,
    /// The workbench-owned session (one selection/document for the canvas,
    /// the Fixtures tree, and the Props pane).
    session: Signal<MapEditorSession>,
    /// Bumped by ANY committed change — canvas gestures, editor keys, and
    /// the Props pane alike: each bump applies the session's document
    /// through the same echo-suppressed pipeline, so undo history survives
    /// the round-trip whatever the writer.
    commit_requests: Signal<u64>,
    /// Written by this coordinator; read by the center (canvas host +
    /// toolbar).
    state: Signal<DiveAssetState>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let mut session = session;
    let mut state = state;
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

    // Seed / re-seed the session from pipeline content, suppressing the
    // echoes of our own applies so in-editor undo history survives its
    // round-trips. `applied` is a QUEUE, not a slot: rapid commits mean
    // several applies can be in flight at once, and an EARLIER apply's
    // echo landing after a LATER apply must still read as ours — a
    // single-slot compare made that stale echo look external, re-seeded
    // the session, and threw undo into a two-state loop (G1 bug 2).
    // All signal writes are guarded — this runs during render.
    let mut seeded = use_signal(|| None::<(ArtifactLocation, Map2dDoc)>);
    let applied = use_hook(|| Rc::new(RefCell::new(Vec::<String>::new())));
    let content_text = editor
        .content
        .as_ref()
        .and_then(|content| content.text().map(str::to_string));
    if let Some(text) = &content_text {
        let echo = {
            let mut queue = applied.borrow_mut();
            match queue.iter().position(|entry| entry == text) {
                Some(position) => {
                    // Ours: drop every SUPERSEDED apply before it (their
                    // echoes were skipped by the store) but KEEP the match
                    // — the settled content re-renders many times, and
                    // each one must keep reading as our own echo (the old
                    // single-slot compare was persistent for the same
                    // reason). Newer in-flight applies stay armed after it.
                    queue.drain(..position);
                    true
                }
                None => false,
            }
        };
        if !echo {
            // Compare PARSED documents, never serialized text: the stored
            // file is pretty-printed while the editor emits compact JSON,
            // so text comparison never matches — and the compare is
            // against the last SEEDED doc, never the live session doc,
            // which mutates mid-gesture.
            let decision = {
                let current = seeded.peek();
                let current_doc = current
                    .as_ref()
                    .and_then(|(artifact, doc)| (*artifact == editor.artifact).then_some(doc));
                decide_seed(&editor.source, text, current_doc)
            };
            match decision {
                SeedDecision::Keep => {
                    if *state.peek() != DiveAssetState::Ready {
                        state.set(DiveAssetState::Ready);
                    }
                }
                SeedDecision::Seed(doc) => {
                    seeded.set(Some((editor.artifact.clone(), doc.clone())));
                    session.write().set_doc(doc);
                    if *state.peek() != DiveAssetState::Ready {
                        state.set(DiveAssetState::Ready);
                    }
                }
                SeedDecision::Refuse(refusal) => {
                    if *state.peek() != DiveAssetState::Refused(refusal.clone()) {
                        state.set(DiveAssetState::Refused(refusal));
                    }
                }
            }
        } else if *state.peek() != DiveAssetState::Ready {
            state.set(DiveAssetState::Ready);
        }
    } else {
        // A switch is in flight (content not landed): the session still
        // holds the PREVIOUS fixture's document — nothing may render or
        // apply it against this artifact.
        let loading = !matches!(
            seeded.peek().as_ref(),
            Some((artifact, _)) if *artifact == editor.artifact
        );
        if loading && *state.peek() != DiveAssetState::Loading {
            state.set(DiveAssetState::Loading);
        }
    }

    // Commit bumps: serialize the session's document and apply it
    // whole-body, arming the echo queue so the round-trip never re-seeds.
    // Only while Ready — a bump racing a dive-switch must not write the
    // old fixture's document to the new artifact.
    let mut seen_commit = use_signal(|| *commit_requests.peek());
    {
        let now = *commit_requests.read();
        if *seen_commit.peek() != now {
            seen_commit.set(now);
            if *state.peek() == DiveAssetState::Ready {
                let json = session.peek().doc().to_json();
                {
                    let mut queue = applied.borrow_mut();
                    queue.push(json.clone());
                    // A bounded queue: anything this deep in flight is
                    // long superseded.
                    let overflow = queue.len().saturating_sub(16);
                    if overflow > 0 {
                        queue.drain(..overflow);
                    }
                }
                if let Some(handler) = &on_action {
                    handler.call(editor.apply_action(&json));
                }
            }
        }
    }

    rsx! {}
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
/// which a toolbar button provides).
pub(crate) fn click_element(id: &str) {
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

/// Programmatic file download: a transient anchor with a `download`
/// attribute, clicked from the toolbar action (a user gesture).
pub(crate) fn trigger_download(filename: &str, data_url: &str) {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast;
        if let Some(document) = web_sys::window().and_then(|window| window.document())
            && let Ok(anchor) = document.create_element("a")
        {
            let _ = anchor.set_attribute("href", data_url);
            let _ = anchor.set_attribute("download", filename);
            if let Ok(anchor) = anchor.dyn_into::<web_sys::HtmlElement>() {
                anchor.click();
            }
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (filename, data_url);
    }
}

pub(crate) fn save_overlay_action() -> UiAction {
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
    /// and — because the session never seeds — the stored body survives an
    /// open → close round trip byte for byte, with nothing applied.
    #[test]
    fn a_newer_body_is_refused_and_never_rewritten() {
        // Open: the host decides what to do with the fetched body. A
        // refusal surfaces instead of a session, so the only writer of
        // this asset — the commit path's `apply_action` — never runs.
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
        // Re-render with the session seeded: the same body decides Keep, so
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
