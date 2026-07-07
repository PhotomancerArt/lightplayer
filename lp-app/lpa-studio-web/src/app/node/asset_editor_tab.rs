//! Node-pane "editor" tab body: a [`CodeEditor`] over one asset artifact.
//!
//! Renders `UiAssetEditorTab` (content resolution, in-flight and failure
//! projections are all controller-produced). The component owns exactly two
//! pieces of state, and both are **editor-local by design** (the
//! editing-model decision that unapplied text never enters core state):
//!
//! - `text` mirrors the current editor text (via the editor's `on_change`),
//!   so the Apply header action can carry it as the op payload;
//! - `modified` mirrors the editor's modified-vs-doc flag and drives the
//!   "Modified" chip plus Apply enablement.
//!
//! (A third, `reveal_line`, is pure transient plumbing for the error
//! strip's click-to-scroll gesture.)
//!
//! Both live in the parent [`super::NodePane`] (passed down as signals)
//! because the Apply action renders in the pane header, outside this tab.
//!
//! Resync flows need no logic here: the `doc` prop is the controller's
//! effective content, and the [`CodeEditor`] reconciliation rules do the
//! rest (external doc wins while unmodified; a doc that catches up with the
//! user's text clears the modified state — that is how an Apply ack clears
//! the chip). While a revert/save transiently drops the resolved content
//! (`content == None` until the re-fetch lands), the last resolved text
//! keeps the editor mounted so unapplied user text is never destroyed.

use dioxus::prelude::*;
use lpa_studio_core::{
    UiAction, UiAssetContentBody, UiAssetEditorKind, UiAssetEditorTab as UiAssetEditorTabData,
    UiShaderError,
};
use std::cell::RefCell;
use std::rc::Rc;

use crate::base::{CodeEditor, CodeEditorDiagnostic, CodeEditorLanguage};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn AssetEditorTab(
    tab: UiAssetEditorTabData,
    /// Editor-local mirror of the current text (see module docs).
    text: Signal<String>,
    /// Editor-local modified flag (see module docs).
    modified: Signal<bool>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    // The last resolved text, kept across renders so a transient
    // `content == None` window (revert/save invalidated the cache) does not
    // unmount the editor and destroy unapplied user text. Non-reactive on
    // purpose: it changes only as a byproduct of a render that already
    // carries the new content.
    let last_doc = use_hook(|| Rc::new(RefCell::new(None::<String>)));
    // Artifact whose content fetch was already requested, so an unresolved
    // view dispatches the fetch exactly once (a failed fetch does not loop;
    // any successful resolution clears the guard for the next invalidation).
    let fetch_requested = use_hook(|| Rc::new(RefCell::new(None::<String>)));

    if let Some(content_text) = tab.content.as_ref().and_then(|content| content.text()) {
        *last_doc.borrow_mut() = Some(content_text.to_string());
    }
    if tab.content.is_some() {
        *fetch_requested.borrow_mut() = None;
    } else {
        let uri = tab.artifact.to_uri();
        let mut requested = fetch_requested.borrow_mut();
        if requested.as_deref() != Some(uri.as_str()) {
            *requested = Some(uri);
            if let Some(handler) = on_action {
                let fetch = tab.fetch_action();
                spawn(async move {
                    handler.call(fetch);
                });
            }
        }
    }

    let doc = last_doc.borrow().clone();
    let language = editor_language(tab.kind);

    // Compile-error presentation (QC5). Suppressed while an apply is in
    // flight — the old error refers to the body being replaced. Positions
    // refer to the last *applied* text and are never remapped while the
    // user types (the Modified chip is the honesty signal); clearing happens
    // view-side the moment the node status leaves its error state.
    let shader_error = (!tab.in_flight)
        .then_some(tab.shader_error.as_ref())
        .flatten()
        .cloned();
    let diagnostics = shader_error
        .as_ref()
        .map(|error| shader_error_diagnostics(error))
        .unwrap_or_default();
    // `None` between reveals so clicking the same line twice still scrolls
    // (the editor acts on `reveal_line` transitions, not values).
    let mut reveal_line = use_signal(|| None::<u32>);
    let reveal_request = reveal_line();
    if reveal_request.is_some() {
        // One render later the request resets, arming the next click.
        spawn(async move {
            reveal_line.set(None);
        });
    }

    let apply_tab = tab.clone();
    let on_apply = move |current_text: String| {
        // Same gate as the header Apply action: only unapplied changes on
        // editable content are worth a mutation.
        if !apply_tab.editable() || !modified() {
            return;
        }
        if let Some(handler) = on_action {
            handler.call(apply_tab.apply_action(&current_text));
        }
    };

    rsx! {
        section { class: "tw:grid tw:min-w-0",
            div { class: "tw:flex tw:min-w-0 tw:items-center tw:justify-between tw:gap-2 tw:border-b tw:border-border-muted tw:bg-card-subtle tw:px-3 tw:py-1.5",
                div { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-2",
                    code { class: "tw:min-w-0 tw:truncate tw:font-mono tw:text-xs tw:text-subtle-foreground", "{tab.source}" }
                    span { class: "tw:flex-none tw:text-xs tw:font-bold tw:text-subtle-foreground", "{tab.kind.editor_label()}" }
                }
                div { class: "tw:flex tw:flex-none tw:items-center tw:gap-1.5",
                    if modified() {
                        span {
                            class: chip_class(ChipTone::Neutral),
                            title: "The editor has changes that have not been applied yet",
                            "Modified"
                        }
                    }
                    if tab.in_flight {
                        span {
                            class: chip_class(ChipTone::Working),
                            title: "The applied body is awaiting the server acknowledgement",
                            "Applying…"
                        }
                    }
                }
            }
            if let Some(reason) = tab.failure.as_ref() {
                p { class: "tw:m-0 tw:border-b tw:border-status-error-border tw:bg-status-error-bg tw:px-3 tw:py-1.5 tw:text-xs tw:leading-snug tw:text-status-error-foreground tw:break-words",
                    "Apply failed: {reason}"
                }
            }
            if let Some(error) = shader_error.as_ref() {
                ShaderErrorStrip {
                    error: error.clone(),
                    on_reveal: move |line| reveal_line.set(Some(line)),
                }
            }
            match (&tab.content.as_ref().map(|content| &content.body), &doc) {
                (Some(UiAssetContentBody::Binary { len }), _) => rsx! {
                    EditorTabNote { note: format!("Binary asset ({len} bytes) — not editable.") }
                },
                (Some(UiAssetContentBody::Deleted), _) => rsx! {
                    EditorTabNote { note: "A pending edit deletes this asset body.".to_string() }
                },
                (_, Some(doc)) => rsx! {
                    div { class: "tw:h-80 tw:min-w-0",
                        CodeEditor {
                            doc: doc.clone(),
                            language,
                            diagnostics,
                            reveal_line: reveal_request,
                            on_modified: move |value| modified.set(value),
                            on_change: move |value| text.set(value),
                            on_apply,
                        }
                    }
                },
                (_, None) => rsx! {
                    EditorTabNote { note: "Loading asset content…".to_string() }
                },
            }
        }
    }
}

/// Read-only placeholder body for the non-editable states (binary body,
/// deleted body, content still loading).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn EditorTabNote(note: String) -> Element {
    rsx! {
        p { class: "tw:m-0 tw:px-3 tw:py-4 tw:text-sm tw:text-subtle-foreground", "{note}" }
    }
}

/// Compile-error strip: the parsed message plus, when located, a clickable
/// `line:col` that scrolls the editor there. The full original error text
/// rides the tooltip.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ShaderErrorStrip(error: UiShaderError, on_reveal: EventHandler<u32>) -> Element {
    rsx! {
        div {
            class: "tw:flex tw:min-w-0 tw:items-baseline tw:gap-2 tw:border-b tw:border-status-error-border tw:bg-status-error-bg tw:px-3 tw:py-1.5 tw:text-xs tw:leading-snug tw:text-status-error-foreground",
            title: "{error.raw}",
            span { class: "tw:flex-none tw:font-bold", "Compile error" }
            span { class: "tw:min-w-0 tw:break-words", "{error.message}" }
            if let Some((line, col)) = error.line_col {
                button {
                    class: "tw:flex-none tw:cursor-pointer tw:border-0 tw:bg-transparent tw:p-0 tw:font-mono tw:font-bold tw:text-status-error-foreground tw:underline",
                    r#type: "button",
                    title: "Show line {line} in the editor",
                    onclick: move |_| on_reveal.call(line),
                    "{line}:{col}"
                }
            }
        }
    }
}

/// The strip's location as editor lint chrome (one diagnostic today — the
/// compile pipeline reports the first error; see `UiShaderError`).
fn shader_error_diagnostics(error: &UiShaderError) -> Vec<CodeEditorDiagnostic> {
    let Some((line, col)) = error.line_col else {
        return Vec::new();
    };
    vec![CodeEditorDiagnostic {
        line,
        col,
        message: error.message.clone(),
    }]
}

fn editor_language(kind: UiAssetEditorKind) -> CodeEditorLanguage {
    match kind {
        UiAssetEditorKind::Glsl => CodeEditorLanguage::Glsl,
        UiAssetEditorKind::Svg => CodeEditorLanguage::Xml,
        UiAssetEditorKind::Text | UiAssetEditorKind::Binary => CodeEditorLanguage::Plain,
    }
}

/// Tones for the tab's editor-local chips. Deliberately NOT the unsaved
/// (yellow) or live (blue) families: unapplied editor text is neither — it
/// exists only in this editor until applied.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ChipTone {
    Neutral,
    Working,
}

fn chip_class(tone: ChipTone) -> &'static str {
    match tone {
        ChipTone::Neutral => {
            "tw:shrink-0 tw:whitespace-nowrap tw:rounded-pill tw:border tw:border-status-neutral-border tw:bg-status-neutral-bg tw:px-2 tw:py-0.5 tw:text-xs tw:font-bold tw:leading-none tw:text-status-neutral-foreground"
        }
        ChipTone::Working => {
            "tw:shrink-0 tw:whitespace-nowrap tw:rounded-pill tw:border tw:border-status-working-border tw:bg-status-working-bg tw:px-2 tw:py-0.5 tw:text-xs tw:font-bold tw:leading-none tw:text-status-working-foreground"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_language_maps_kinds_onto_editor_modes() {
        assert_eq!(
            editor_language(UiAssetEditorKind::Glsl),
            CodeEditorLanguage::Glsl
        );
        assert_eq!(
            editor_language(UiAssetEditorKind::Svg),
            CodeEditorLanguage::Xml
        );
        assert_eq!(
            editor_language(UiAssetEditorKind::Text),
            CodeEditorLanguage::Plain
        );
    }

    #[test]
    fn located_errors_become_one_editor_diagnostic() {
        let located = UiShaderError::parse("shader compile: parse: error: bad\n --> <shader>:4:7");
        assert_eq!(
            shader_error_diagnostics(&located),
            vec![CodeEditorDiagnostic {
                line: 4,
                col: 7,
                message: "bad".to_string(),
            }]
        );

        let line_less = UiShaderError::parse("shader compile: compilation blocked");
        assert!(shader_error_diagnostics(&line_less).is_empty());
    }

    #[test]
    fn editor_local_chips_use_neutral_and_working_families_only() {
        // The one deliberate divergence from the dirty color language:
        // unapplied text is not unsaved (yellow) and not live (blue).
        assert!(chip_class(ChipTone::Neutral).contains("status-neutral"));
        assert!(chip_class(ChipTone::Working).contains("status-working"));
    }
}
