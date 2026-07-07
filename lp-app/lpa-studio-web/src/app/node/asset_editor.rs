//! Inline asset editor: a [`CodeEditor`] over one asset artifact, rendered
//! in place inside its config slot row (so the output stays visible beside
//! it, and any asset anywhere in the slot tree can be edited).
//!
//! Renders `UiAssetEditor` (content resolution, in-flight and failure
//! projections are all controller-produced). The component owns two pieces
//! of local state — both **editor-local by design** (editing-model ADR D8:
//! unapplied text never enters core state):
//!
//! - `text` mirrors the current editor text (via the editor's `on_change`),
//!   so the inline Apply button can carry it as the op payload;
//! - `modified` mirrors the editor's modified-vs-doc flag and drives the
//!   "Modified" chip plus Apply enablement.
//!
//! (A third, `reveal_line`, is pure transient plumbing for the error strip's
//! click-to-scroll gesture.)
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
    UiAction, UiAssetContentBody, UiAssetEditor as UiAssetEditorData, UiAssetEditorKind,
    UiShaderError,
};
use std::cell::RefCell;
use std::rc::Rc;

use crate::base::{CodeEditor, CodeEditorDiagnostic, CodeEditorLanguage};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn AssetEditor(
    editor: UiAssetEditorData,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    // Editor-local state (see module docs). `modified` gates the Apply
    // button; `text` carries the current body to the Apply action.
    let mut modified = use_signal(|| false);
    let mut text = use_signal(String::new);
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

    if let Some(content_text) = editor.content.as_ref().and_then(|content| content.text()) {
        *last_doc.borrow_mut() = Some(content_text.to_string());
    }
    if editor.content.is_some() {
        *fetch_requested.borrow_mut() = None;
    } else {
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

    let doc = last_doc.borrow().clone();
    let language = editor_language(editor.kind);

    // Compile-error presentation (QC5). Suppressed while an apply is in
    // flight — the old error refers to the body being replaced. Positions
    // refer to the last *applied* text and are never remapped while the
    // user types (the Modified chip is the honesty signal); clearing happens
    // view-side the moment the node status leaves its error state.
    let shader_error = (!editor.in_flight)
        .then_some(editor.shader_error.as_ref())
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

    let editable = editor.editable();
    let apply_disabled = !(editable && modified());
    // Apply gate shared by the button and the editor's Cmd/Ctrl+Enter path:
    // only unapplied changes on editable content are worth a mutation. Both
    // closures need the editor, so each gets its own clone.
    let button_editor = editor.clone();
    let keymap_editor = editor.clone();
    let on_apply = move |current_text: String| {
        if !keymap_editor.editable() || !modified() {
            return;
        }
        if let Some(handler) = on_action {
            handler.call(keymap_editor.apply_action(&current_text));
        }
    };

    rsx! {
        section { class: "tw:grid tw:min-w-0 tw:border-t tw:border-border-muted tw:bg-page",
            div { class: "tw:flex tw:min-w-0 tw:items-center tw:justify-between tw:gap-2 tw:px-3 tw:py-1.5",
                div { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-2",
                    code { class: "tw:min-w-0 tw:truncate tw:font-mono tw:text-xs tw:text-subtle-foreground", "{editor.source}" }
                    span { class: "tw:flex-none tw:text-xs tw:font-bold tw:text-subtle-foreground", "{editor.kind.editor_label()}" }
                }
                div { class: "tw:flex tw:flex-none tw:items-center tw:gap-1.5",
                    if modified() {
                        span {
                            class: chip_class(ChipTone::Neutral),
                            title: "The editor has changes that have not been applied yet",
                            "Modified"
                        }
                    }
                    if editor.in_flight {
                        span {
                            class: chip_class(ChipTone::Working),
                            title: "The applied body is awaiting the server acknowledgement",
                            "Applying…"
                        }
                    }
                    if editable {
                        button {
                            class: apply_button_class(apply_disabled),
                            r#type: "button",
                            disabled: apply_disabled,
                            title: "Apply the edited body to the running project (Cmd/Ctrl+Enter)",
                            onclick: move |event| {
                                event.stop_propagation();
                                if button_editor.editable() && modified() {
                                    if let Some(handler) = on_action {
                                        handler.call(button_editor.apply_action(&text()));
                                    }
                                }
                            },
                            "Apply"
                        }
                    }
                }
            }
            if let Some(reason) = editor.failure.as_ref() {
                p { class: "tw:m-0 tw:border-t tw:border-status-error-border tw:bg-status-error-bg tw:px-3 tw:py-1.5 tw:text-xs tw:leading-snug tw:text-status-error-foreground tw:break-words",
                    "Apply failed: {reason}"
                }
            }
            if let Some(error) = shader_error.as_ref() {
                ShaderErrorStrip {
                    error: error.clone(),
                    on_reveal: move |line| reveal_line.set(Some(line)),
                }
            }
            match (&editor.content.as_ref().map(|content| &content.body), &doc) {
                (Some(UiAssetContentBody::Binary { len }), _) => rsx! {
                    AssetEditorNote { note: format!("Binary asset ({len} bytes) — not editable.") }
                },
                (Some(UiAssetContentBody::Deleted), _) => rsx! {
                    AssetEditorNote { note: "A pending edit deletes this asset body.".to_string() }
                },
                (_, Some(doc)) => rsx! {
                    div { class: "tw:h-72 tw:min-w-0 tw:border-t tw:border-border-subtle",
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
                    AssetEditorNote { note: "Loading asset content…".to_string() }
                },
            }
        }
    }
}

/// Read-only placeholder body for the non-editable states (binary body,
/// deleted body, content still loading).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn AssetEditorNote(note: String) -> Element {
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
            class: "tw:flex tw:min-w-0 tw:items-baseline tw:gap-2 tw:border-t tw:border-status-error-border tw:bg-status-error-bg tw:px-3 tw:py-1.5 tw:text-xs tw:leading-snug tw:text-status-error-foreground",
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

fn apply_button_class(disabled: bool) -> &'static str {
    if disabled {
        "tw:inline-flex tw:flex-none tw:cursor-not-allowed tw:items-center tw:rounded-xs tw:border tw:border-border-subtle tw:bg-transparent tw:px-2 tw:py-0.5 tw:text-xs tw:font-bold tw:text-subtle-foreground"
    } else {
        "tw:inline-flex tw:flex-none tw:cursor-pointer tw:items-center tw:rounded-xs tw:border tw:border-accent-border tw:bg-transparent tw:px-2 tw:py-0.5 tw:text-xs tw:font-bold tw:text-accent tw:hover:bg-accent-wash"
    }
}

/// Tones for the editor-local chips. Deliberately NOT the unsaved (yellow)
/// or live (blue) families: unapplied editor text is neither — it exists
/// only in this editor until applied.
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
