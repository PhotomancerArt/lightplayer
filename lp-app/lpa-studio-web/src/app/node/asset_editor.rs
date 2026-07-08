//! Inline asset editor: a [`CodeEditor`] over one asset artifact, rendered
//! in place inside its config slot row (so the output stays visible beside
//! it, and any asset anywhere in the slot tree can be edited).
//!
//! Renders `UiAssetEditor` (content resolution, in-flight and failure
//! projections are all controller-produced). The component owns two pieces
//! of local state — both **editor-local by design** (editing-model ADR D9:
//! unapplied text never enters core state):
//!
//! - `text` mirrors the current editor text (via the editor's `on_change`),
//!   so the inline Apply button can carry it as the op payload;
//! - `modified` mirrors the editor's modified-vs-doc flag and drives the
//!   "Modified" state plus Apply enablement.
//!
//! (A third, `reveal_line`, is pure transient plumbing for the error's
//! click-to-scroll gesture.)
//!
//! **No reflow.** All transient state (modified / applying / compile error /
//! apply failure / unsaved) is absorbed by a single **fixed-height status
//! bar** above a fixed editor — the compile error is a *state of the bar*,
//! not a strip inserted above the editor. The editor never changes size or
//! position. See `../../../Planning/lp2025/2026-07-07-glsl-editor-ux/`.
//!
//! Resync flows need no logic here: the `doc` prop is the controller's
//! effective content, and the [`CodeEditor`] reconciliation rules do the
//! rest (external doc wins while unmodified; a doc that catches up with the
//! user's text clears the modified state — that is how an Apply ack clears
//! the state). While a revert/save transiently drops the resolved content
//! (`content == None` until the re-fetch lands), the last resolved text
//! keeps the editor mounted so unapplied user text is never destroyed.

use dioxus::prelude::*;
use lpa_studio_core::{
    UiAction, UiAssetContentBody, UiAssetEditor as UiAssetEditorData, UiAssetEditorKind,
    UiShaderError,
};
use std::cell::RefCell;
use std::rc::Rc;

use crate::base::{
    CodeEditor, CodeEditorDiagnostic, CodeEditorLanguage, DetailPopover, DetailSection,
    IconMenuTone, StudioIconName,
};

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

    // Compile-error presentation. Suppressed while an apply is in flight —
    // the old error refers to the body being replaced. Positions refer to
    // the last *applied* text and are never remapped while the user types
    // (the Modified state is the honesty signal); clearing happens view-side
    // the moment the node status leaves its error state.
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
    // The overlay carries an applied-but-unsaved edit (distinct from
    // unapplied editor text): drives the amber "Unsaved" bar state.
    let dirty = editor.content.as_ref().is_some_and(|content| content.dirty);
    let bar_state = EditorBarState::compute(
        editor.failure.clone(),
        editor.in_flight,
        shader_error.clone(),
        modified(),
        dirty,
    );

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
        section { class: "tw:grid tw:min-w-0 tw:border-t tw:border-border-muted",
            // Fixed-height status bar: absorbs every transient state so the
            // editor below never moves. `h-8` + `overflow-hidden` guarantee a
            // constant height regardless of content.
            div { class: bar_class(bar_state.tone()),
                div { class: "tw:flex tw:min-w-0 tw:flex-1 tw:items-center tw:gap-2",
                    EditorBarStatus {
                        state: bar_state.clone(),
                        source: editor.source.clone(),
                        kind_label: editor.kind.editor_label(),
                        on_reveal: move |line| reveal_line.set(Some(line)),
                    }
                }
                div { class: "tw:flex tw:flex-none tw:items-center tw:gap-1.5",
                    if let EditorBarState::CompileError { error } = &bar_state {
                        FullErrorPopover { raw: error.raw.clone() }
                    }
                    if let EditorBarState::ApplyFailed { reason } = &bar_state {
                        FullErrorPopover { raw: reason.clone() }
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

/// The one status the bar shows, computed from the editor's projections plus
/// the editor-local `modified` flag. Exactly one state at a time, in
/// attention-first priority (see [`Self::compute`]); the tone and the left
/// zone content both derive from it, so the fixed-height bar never grows.
#[derive(Clone, Debug, PartialEq)]
enum EditorBarState {
    /// The last apply was rejected (server rejection or the client size
    /// guard); carries the reason. Highest priority.
    ApplyFailed { reason: String },
    /// An applied body is awaiting its server acknowledgement.
    Applying,
    /// The applied body failed to compile; carries the parsed error.
    CompileError { error: UiShaderError },
    /// The editor has unapplied local changes.
    Modified,
    /// An applied edit is not yet written to the project file.
    Unsaved,
    /// Clean and compiling: identity only.
    Clean,
}

impl EditorBarState {
    /// Attention-first priority: a failed apply outranks an in-flight one
    /// outranks a compile error outranks unapplied text outranks an unsaved
    /// edit outranks clean. `shader_error` is expected already suppressed
    /// while in flight (the caller does that).
    fn compute(
        failure: Option<String>,
        in_flight: bool,
        shader_error: Option<UiShaderError>,
        modified: bool,
        dirty: bool,
    ) -> Self {
        if let Some(reason) = failure {
            Self::ApplyFailed { reason }
        } else if in_flight {
            Self::Applying
        } else if let Some(error) = shader_error {
            Self::CompileError { error }
        } else if modified {
            Self::Modified
        } else if dirty {
            Self::Unsaved
        } else {
            Self::Clean
        }
    }

    /// Bar background/border tone for the state.
    fn tone(&self) -> BarTone {
        match self {
            Self::ApplyFailed { .. } | Self::CompileError { .. } => BarTone::Error,
            Self::Applying => BarTone::Working,
            // Unapplied editor text is deliberately neutral (D9): not the
            // unsaved-yellow / live-blue family.
            Self::Modified => BarTone::Neutral,
            // An applied-but-unsaved edit IS the unsaved (amber) state.
            Self::Unsaved => BarTone::Unsaved,
            Self::Clean => BarTone::Plain,
        }
    }
}

/// Left-zone content of the status bar for one state. Identity (source +
/// kind) shows in the calm states; a compile/apply error takes the zone over
/// (QD-A) with the message truncated to one line and, when located, a
/// clickable `line:col` that reveals the editor line.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn EditorBarStatus(
    state: EditorBarState,
    source: String,
    kind_label: &'static str,
    on_reveal: EventHandler<u32>,
) -> Element {
    match state {
        EditorBarState::ApplyFailed { reason } => rsx! {
            span { class: "tw:flex-none tw:font-bold", "Apply failed" }
            span { class: "tw:min-w-0 tw:truncate", title: "{reason}", "{reason}" }
        },
        EditorBarState::CompileError { error } => rsx! {
            span { class: "tw:flex-none tw:font-bold", "Compile error" }
            span { class: "tw:min-w-0 tw:truncate", title: "{error.raw}", "{error.message}" }
            if let Some((line, col)) = error.line_col {
                button {
                    class: "tw:flex-none tw:cursor-pointer tw:border-0 tw:bg-transparent tw:p-0 tw:font-mono tw:font-bold tw:text-status-error-foreground tw:underline",
                    r#type: "button",
                    title: "Show line {line} in the editor",
                    onclick: move |_| on_reveal.call(line),
                    "{line}:{col}"
                }
            }
        },
        EditorBarState::Applying => rsx! {
            EditorBarIdentity { source, kind_label }
            span { class: "tw:flex-none tw:font-bold", "Applying…" }
        },
        EditorBarState::Modified => rsx! {
            EditorBarIdentity { source, kind_label }
            span { class: "tw:flex-none tw:font-bold", "Modified" }
        },
        EditorBarState::Unsaved => rsx! {
            EditorBarIdentity { source, kind_label }
            span { class: "tw:flex-none tw:font-bold", "Unsaved" }
        },
        EditorBarState::Clean => rsx! {
            EditorBarIdentity { source, kind_label }
        },
    }
}

/// Source path + kind label — the calm-state identity of the bar's left zone.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn EditorBarIdentity(source: String, kind_label: &'static str) -> Element {
    rsx! {
        code { class: "tw:min-w-0 tw:truncate tw:font-mono tw:text-xs tw:text-subtle-foreground", "{source}" }
        span { class: "tw:flex-none tw:text-xs tw:font-bold tw:text-subtle-foreground", "{kind_label}" }
    }
}

/// The detail-popup trigger that opens the full (multi-line) error text —
/// the fixed-height bar only shows a truncated line (QD-A).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn FullErrorPopover(raw: String) -> Element {
    rsx! {
        DetailPopover {
            icon: StudioIconName::StatusError,
            label: "Show the full error".to_string(),
            title: "Full error".to_string(),
            tone: IconMenuTone::Error,
            DetailSection {
                pre { class: "tw:m-0 tw:max-h-72 tw:overflow-auto tw:whitespace-pre-wrap tw:break-words tw:font-mono tw:text-xs tw:leading-snug tw:text-status-error-foreground",
                    "{raw}"
                }
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

/// Tone of the fixed-height status bar. Neutral (unapplied text) and Unsaved
/// (amber, applied-but-uncommitted) are deliberately distinct — one is
/// editor-local, the other counts toward Save (D9 color language).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BarTone {
    Plain,
    Neutral,
    Working,
    Unsaved,
    Error,
}

/// Fixed-height bar shell for a tone. `h-8` + `overflow-hidden` are the
/// no-reflow guarantee: the bar is one line tall in every state.
fn bar_class(tone: BarTone) -> String {
    let tone_class = match tone {
        BarTone::Plain => "tw:bg-card-subtle tw:text-subtle-foreground",
        BarTone::Neutral => {
            "tw:bg-status-neutral-bg tw:text-status-neutral-foreground tw:border-status-neutral-border"
        }
        BarTone::Working => {
            "tw:bg-status-working-bg tw:text-status-working-foreground tw:border-status-working-border"
        }
        BarTone::Unsaved => {
            "tw:bg-status-warning-bg tw:text-status-warning-foreground tw:border-status-warning-border"
        }
        BarTone::Error => {
            "tw:bg-status-error-bg tw:text-status-error-foreground tw:border-status-error-border"
        }
    };
    format!(
        "tw:flex tw:h-8 tw:min-w-0 tw:items-center tw:gap-2 tw:overflow-hidden tw:px-3 tw:text-xs tw:leading-none {tone_class}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn err() -> UiShaderError {
        UiShaderError::parse("shader compile: parse: error: bad\n --> <shader>:4:7")
    }

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
        assert_eq!(
            shader_error_diagnostics(&err()),
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
    fn bar_state_priority_is_attention_first() {
        // failure › applying › compile-error › modified › unsaved › clean.
        assert!(matches!(
            EditorBarState::compute(Some("too big".into()), true, Some(err()), true, true),
            EditorBarState::ApplyFailed { .. }
        ));
        assert_eq!(
            EditorBarState::compute(None, true, Some(err()), true, true),
            EditorBarState::Applying
        );
        assert!(matches!(
            EditorBarState::compute(None, false, Some(err()), true, true),
            EditorBarState::CompileError { .. }
        ));
        assert_eq!(
            EditorBarState::compute(None, false, None, true, true),
            EditorBarState::Modified
        );
        assert_eq!(
            EditorBarState::compute(None, false, None, false, true),
            EditorBarState::Unsaved
        );
        assert_eq!(
            EditorBarState::compute(None, false, None, false, false),
            EditorBarState::Clean
        );
    }

    #[test]
    fn tones_keep_unapplied_neutral_and_unsaved_amber() {
        // The one deliberate divergence from the dirty color language:
        // unapplied editor text is neutral, not the unsaved-amber family.
        assert_eq!(EditorBarState::Modified.tone(), BarTone::Neutral);
        assert_eq!(EditorBarState::Unsaved.tone(), BarTone::Unsaved);
        assert_eq!(EditorBarState::Applying.tone(), BarTone::Working);
        assert_eq!(EditorBarState::Clean.tone(), BarTone::Plain);
        assert!(bar_class(BarTone::Unsaved).contains("status-warning"));
        assert!(bar_class(BarTone::Neutral).contains("status-neutral"));
        // Fixed height is the no-reflow guarantee.
        assert!(bar_class(BarTone::Plain).contains("tw:h-8"));
    }
}
