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
//!   so an apply (auto or ⌘↵) can carry it as the op payload;
//! - `modified` mirrors the editor's modified-vs-doc flag and (with
//!   `in_flight`) drives the bar's subtle applying indicator.
//!
//! (A third, `reveal_line`, is pure transient plumbing for the error's
//! click-to-scroll gesture.)
//!
//! **Auto-apply, always.** Edits apply themselves [`AUTO_APPLY_DEBOUNCE_MS`]
//! after the last keystroke — the live-editing loop; there is no manual
//! mode (Apply vs Save is the two-axis model: applying is automatic and
//! transient, saving is deliberate). The debounce is epoch-guarded (only the
//! newest keystroke's timer fires), waits politely while an apply is in
//! flight, and never auto-retries after a failed/oversize apply (the failure
//! would just repeat; editing the text re-arms it, and ⌘↵ remains an
//! immediate apply-now). The engine keeps the last good program rendering
//! through a bad apply (keep-last-good), so mid-edit compile errors show in
//! the bar without blanking the output.
//!
//! **The gentle two-half bar.** One fixed-height (`h-8`) bar whose
//! *geometry never changes*: the background stays plain in every state, no
//! element appears, disappears, or moves, and state lives in color
//! transitions only. The **left half** carries the shader's compile/apply
//! truth: identity (`source` · kind) when calm, the truncated compile/apply
//! error (with `line:col` reveal + full-error popover) when not, plus a
//! small applying dot that fades in while an edit is unapplied or in
//! flight. The **right half** carries persistence independently:
//! `Saved`/`Unsaved` plus always-mounted Revert and Save (⌘S) buttons that
//! enable/disable in place — so an error never hides the unsaved state, and
//! Revert stays one click away even while the applied body fails to
//! compile. See `../../../Planning/lp2025/2026-07-14-shader-auto-apply/`.
//!
//! **Keyboard.** ⌘↵/Ctrl+Enter applies now and ⌘S/Ctrl+S saves while the
//! editor is focused (both captured in the CodeMirror keymap; ⌘S never
//! reaches the browser). Hints via [`crate::base::keyboard`].
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
    ControllerId, ProjectController, ProjectOp, UiAction, UiAssetContentBody,
    UiAssetEditor as UiAssetEditorData, UiAssetEditorKind, UiShaderError,
};
use std::cell::RefCell;
use std::rc::Rc;

use crate::base::{
    CodeEditor, CodeEditorDiagnostic, CodeEditorLanguage, DetailPopover, DetailSection,
    IconMenuTone, Platform, StudioIconName, keyboard,
};

/// Quiet period after the last keystroke before an auto-apply fires — long
/// enough to not race normal typing, short enough to feel live next to the
/// ~200 ms device compile.
const AUTO_APPLY_DEBOUNCE_MS: u32 = 500;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn AssetEditor(
    editor: UiAssetEditorData,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
    /// Platform for shortcut-hint display (`⌘S` vs `Ctrl+S`). Defaults to
    /// runtime detection; stories pin it for deterministic captures.
    #[props(default)]
    platform: Option<Platform>,
) -> Element {
    // Editor-local state (see module docs).
    let mut modified = use_signal(|| false);
    let mut text = use_signal(String::new);
    // Auto-apply plumbing: the keystroke epoch (only the newest keystroke's
    // debounce timer fires) and a non-reactive mirror of the controller
    // projections the timer must read *at fire time* (values captured at
    // spawn time would be half a second stale). The epoch is deliberately
    // NOT a signal: signal writes inside a keystroke burst are not
    // observable by the burst's later handler calls (write batching), which
    // would give every keystroke the same epoch and let timers fire
    // mid-typing. The RefCell increments immediately.
    let edit_epoch = use_hook(|| Rc::new(RefCell::new(0_u64)));
    let auto_apply_gate = use_hook(|| Rc::new(RefCell::new(AutoApplyGate::default())));
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
    // (the applying dot is the honesty signal); clearing happens view-side
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
    // unapplied editor text): drives the right half's persistence state.
    let dirty = editor.content.as_ref().is_some_and(|content| content.dirty);
    // Left half: the shader's compile/apply truth.
    let error_state = EditorErrorState::compute(editor.failure.clone(), shader_error.clone());
    // The subtle applying indicator covers the whole "text not acked yet"
    // window: unapplied local edits and the in-flight apply.
    let busy = modified() || editor.in_flight;
    // The persistence cluster mounts once per editor kind — never per state —
    // so Revert/Save exist from first paint and only enable/disable in place.
    let supports_editing = editor.kind.supports_editor();

    // Keep the fire-time gate current with this render's projections.
    *auto_apply_gate.borrow_mut() = AutoApplyGate {
        editable,
        in_flight: editor.in_flight,
        apply_failed: editor.failure.is_some(),
    };
    let platform = platform.unwrap_or_else(Platform::detect);
    // The editor's Cmd/Ctrl+Enter path stays an immediate apply-now: only
    // unapplied changes on editable content are worth a mutation.
    let keymap_editor = editor.clone();
    let on_apply = move |current_text: String| {
        if !keymap_editor.editable() || !modified() {
            return;
        }
        if let Some(handler) = on_action {
            handler.call(keymap_editor.apply_action(&current_text));
        }
    };
    // Save gate shared by the bar's Save button and the editor's Cmd/Ctrl+S
    // path: SaveOverlay is project-level, so only an applied-but-unsaved edit
    // is worth dispatching — a stray ⌘S is a harmless no-op (the editor
    // swallows the keystroke either way, so the browser dialog never opens).
    let on_save = move |_: ()| {
        if !dirty {
            return;
        }
        if let Some(handler) = on_action {
            handler.call(save_overlay_action());
        }
    };
    // Revert discards the applied-but-unsaved edit and returns the running
    // project to the saved file — deliberately available in every dirty
    // state, including while the applied body fails to compile (U8).
    let revert_editor = editor.clone();
    let on_revert = move |_: ()| {
        if !dirty {
            return;
        }
        if let Some(handler) = on_action {
            handler.call(revert_editor.revert_action());
        }
    };
    // Debounced auto-apply: every text change bumps the epoch and arms a
    // fresh timer; a timer whose epoch was superseded exits silently, so only
    // the newest keystroke's timer can fire. External resyncs also land here,
    // but their timers resolve to Skip (nothing is modified). The verdict
    // re-reads live state at fire time — see [`auto_apply_verdict`].
    let auto_editor = editor.clone();
    let auto_gate = auto_apply_gate.clone();
    let epoch_cell = edit_epoch.clone();
    let on_change = move |value: String| {
        text.set(value);
        let epoch = {
            let mut cell = epoch_cell.borrow_mut();
            *cell = cell.wrapping_add(1);
            *cell
        };
        let apply_editor = auto_editor.clone();
        let gate = auto_gate.clone();
        let epoch_watch = epoch_cell.clone();
        spawn(async move {
            loop {
                gloo_timers::future::TimeoutFuture::new(AUTO_APPLY_DEBOUNCE_MS).await;
                if *epoch_watch.borrow() != epoch {
                    return;
                }
                let gate = *gate.borrow();
                match auto_apply_verdict(gate, *modified.peek()) {
                    AutoApplyVerdict::Fire => {
                        if let Some(handler) = on_action {
                            handler.call(apply_editor.apply_action(text.peek().as_str()));
                        }
                        return;
                    }
                    AutoApplyVerdict::Skip => return,
                    // An apply is in flight: keep waiting and re-check.
                    AutoApplyVerdict::Wait => {}
                }
            }
        });
    };

    rsx! {
        section { class: "tw:grid tw:min-w-0 tw:border-t tw:border-border-muted",
            // The gentle fixed-height status bar: plain background in every
            // state, constant geometry (`h-8` + overflow-hidden), state in
            // color transitions only.
            div { class: BAR_CLASS,
                // Left half: compile/apply truth + the subtle applying dot.
                div { class: "tw:flex tw:min-w-0 tw:flex-1 tw:items-center tw:gap-2",
                    span { class: applying_dot_class(busy) }
                    match &error_state {
                        EditorErrorState::Calm => rsx! {
                            EditorBarIdentity {
                                source: editor.source.clone(),
                                kind_label: editor.kind.editor_label(),
                            }
                        },
                        EditorErrorState::ApplyFailed { reason } => rsx! {
                            div { class: ERROR_BLOCK_CLASS,
                                span { class: "tw:flex-none tw:font-bold", "Apply failed" }
                                span { class: "tw:min-w-0 tw:truncate", title: "{reason}", "{reason}" }
                                FullErrorPopover { raw: reason.clone() }
                            }
                        },
                        EditorErrorState::CompileError { error } => rsx! {
                            div { class: ERROR_BLOCK_CLASS,
                                span { class: "tw:flex-none tw:font-bold", "Compile error" }
                                span { class: "tw:min-w-0 tw:truncate", title: "{error.raw}", "{error.message}" }
                                if let Some((line, col)) = error.line_col {
                                    button {
                                        class: "tw:flex-none tw:cursor-pointer tw:border-0 tw:bg-transparent tw:p-0 tw:font-mono tw:font-bold tw:text-status-error-foreground tw:underline",
                                        r#type: "button",
                                        title: "Show line {line} in the editor",
                                        onclick: move |_| reveal_line.set(Some(line)),
                                        "{line}:{col}"
                                    }
                                }
                                FullErrorPopover { raw: error.raw.clone() }
                            }
                        },
                    }
                }
                // Right half: persistence, independent of the error state.
                // Everything here is always mounted; only colors/opacity move.
                if supports_editing {
                    div { class: "tw:flex tw:flex-none tw:items-center tw:gap-1.5",
                        span { class: persistence_word_class(dirty),
                            if dirty { "Unsaved" } else { "Saved" }
                        }
                        button {
                            class: persist_button_class(dirty),
                            r#type: "button",
                            disabled: !dirty,
                            title: "Discard the applied edit and return to the saved file",
                            onclick: move |event| {
                                event.stop_propagation();
                                on_revert(());
                            },
                            "Revert"
                        }
                        button {
                            class: persist_button_class(dirty),
                            r#type: "button",
                            disabled: !dirty,
                            title: "Write the applied edits to the project files ({keyboard::SAVE.display(platform)})",
                            onclick: move |event| {
                                event.stop_propagation();
                                on_save(());
                            },
                            "Save"
                            ShortcutHint { text: keyboard::SAVE.display(platform) }
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
                            on_change,
                            on_apply,
                            on_save,
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

/// The bar shell: plain in every state (colors live on the content, animated
/// there). `h-8` + `overflow-hidden` are the no-reflow guarantee.
const BAR_CLASS: &str = "tw:flex tw:h-8 tw:min-w-0 tw:items-center tw:gap-2 tw:overflow-hidden tw:bg-card-subtle tw:px-3 tw:text-xs tw:leading-none tw:text-subtle-foreground";

/// The error block eases in via `@starting-style` instead of snapping —
/// part of the gentle-bar contract (U5).
const ERROR_BLOCK_CLASS: &str = "tw:flex tw:min-w-0 tw:flex-1 tw:items-center tw:gap-2 tw:text-status-error-foreground tw:opacity-100 tw:transition-opacity tw:duration-300 tw:starting:opacity-0";

/// The left half's compile/apply truth. Persistence (saved/unsaved) is the
/// right half's independent axis — an error never hides it (U7).
#[derive(Clone, Debug, PartialEq)]
enum EditorErrorState {
    /// The last apply was rejected (server rejection or the client size
    /// guard); carries the reason. Outranks a stale compile error.
    ApplyFailed { reason: String },
    /// The applied body failed to compile; carries the parsed error.
    CompileError { error: UiShaderError },
    /// Nothing wrong: identity shows.
    Calm,
}

impl EditorErrorState {
    /// `shader_error` is expected already suppressed while in flight (the
    /// caller does that).
    fn compute(failure: Option<String>, shader_error: Option<UiShaderError>) -> Self {
        if let Some(reason) = failure {
            Self::ApplyFailed { reason }
        } else if let Some(error) = shader_error {
            Self::CompileError { error }
        } else {
            Self::Calm
        }
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

/// The keyboard hint riding a bar affordance (`⌘S` / `Ctrl+S`), OS-correct
/// via [`keyboard::Shortcut::display`].
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ShortcutHint(text: String) -> Element {
    rsx! {
        span { class: "tw:ml-1.5 tw:font-mono tw:text-[10px] tw:font-normal tw:opacity-60", "{text}" }
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

/// The controller-projection inputs an expired auto-apply timer reads at
/// fire time, mirrored non-reactively each render (values captured when the
/// timer was armed would be half a second stale).
#[derive(Clone, Copy, Default)]
struct AutoApplyGate {
    editable: bool,
    in_flight: bool,
    apply_failed: bool,
}

/// What an expired auto-apply debounce timer should do.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AutoApplyVerdict {
    /// Dispatch the apply now.
    Fire,
    /// An apply is in flight: keep waiting and re-check.
    Wait,
    /// Drop the request: nothing applicable, or the last apply failed —
    /// auto-retrying a failed/oversize apply would just repeat the failure;
    /// editing the text (new epoch) or ⌘↵ resumes.
    Skip,
}

/// The auto-apply fire decision, pure so the rules unit-test without a DOM.
/// The failure latch outranks everything but the basic gates: once an apply
/// parks as Failed, only a fresh edit or an explicit ⌘↵ resumes the
/// automation.
fn auto_apply_verdict(gate: AutoApplyGate, modified: bool) -> AutoApplyVerdict {
    if !gate.editable || !modified || gate.apply_failed {
        return AutoApplyVerdict::Skip;
    }
    if gate.in_flight {
        return AutoApplyVerdict::Wait;
    }
    AutoApplyVerdict::Fire
}

/// The subtle applying indicator: a small working-toned dot that fades in
/// while an edit is unapplied or in flight and fades back out. Opacity-only
/// — deliberately below the eye-catching motion threshold (U5).
fn applying_dot_class(busy: bool) -> String {
    let visibility = if busy {
        "tw:opacity-70"
    } else {
        "tw:opacity-0"
    };
    format!(
        "tw:h-1.5 tw:w-1.5 tw:flex-none tw:rounded-full tw:bg-status-working-foreground tw:transition-opacity tw:duration-300 {visibility}"
    )
}

/// The right half's persistence word: muted `Saved`, amber `Unsaved`, with
/// the color easing between them.
fn persistence_word_class(dirty: bool) -> String {
    let color = if dirty {
        "tw:text-status-warning-foreground"
    } else {
        "tw:text-dim-foreground"
    };
    format!("tw:flex-none tw:font-bold tw:transition-colors tw:duration-300 {color}")
}

/// Shared class for the always-mounted persistence buttons (Revert, Save):
/// constant geometry, enable/disable in place via color + opacity eases.
fn persist_button_class(enabled: bool) -> String {
    let state = if enabled {
        "tw:cursor-pointer tw:border-accent-border tw:text-accent tw:opacity-100 tw:hover:bg-accent-wash"
    } else {
        "tw:cursor-default tw:border-border-subtle tw:text-subtle-foreground tw:opacity-40"
    };
    format!(
        "tw:inline-flex tw:flex-none tw:items-center tw:rounded-xs tw:border tw:bg-transparent tw:px-2 tw:py-0.5 tw:text-xs tw:font-bold tw:transition tw:duration-300 {state}"
    )
}

/// The project-level Save action the editor's ⌘S and the bar's Save button
/// both dispatch — the same `SaveOverlay` op as the project pane's Save.
fn save_overlay_action() -> UiAction {
    UiAction::from_op(
        ControllerId::new(ProjectController::NODE_ID),
        ProjectOp::SaveOverlay,
    )
}

fn editor_language(kind: UiAssetEditorKind) -> CodeEditorLanguage {
    match kind {
        UiAssetEditorKind::Glsl => CodeEditorLanguage::Glsl,
        UiAssetEditorKind::Svg => CodeEditorLanguage::Xml,
        UiAssetEditorKind::Text | UiAssetEditorKind::Binary => CodeEditorLanguage::Plain,
    }
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
    fn error_state_ranks_apply_failure_over_compile_error() {
        assert!(matches!(
            EditorErrorState::compute(Some("too big".into()), Some(err())),
            EditorErrorState::ApplyFailed { .. }
        ));
        assert!(matches!(
            EditorErrorState::compute(None, Some(err())),
            EditorErrorState::CompileError { .. }
        ));
        assert_eq!(
            EditorErrorState::compute(None, None),
            EditorErrorState::Calm
        );
    }

    #[test]
    fn auto_apply_fires_only_when_quiet_modified_and_idle() {
        let gate = |editable: bool, in_flight: bool, apply_failed: bool| AutoApplyGate {
            editable,
            in_flight,
            apply_failed,
        };

        assert_eq!(
            auto_apply_verdict(gate(true, false, false), true),
            AutoApplyVerdict::Fire
        );
        // Not editable / unmodified → Skip.
        assert_eq!(
            auto_apply_verdict(gate(false, false, false), true),
            AutoApplyVerdict::Skip
        );
        assert_eq!(
            auto_apply_verdict(gate(true, false, false), false),
            AutoApplyVerdict::Skip
        );
        // A parked failure never auto-retries — even outranking in-flight.
        assert_eq!(
            auto_apply_verdict(gate(true, false, true), true),
            AutoApplyVerdict::Skip
        );
        assert_eq!(
            auto_apply_verdict(gate(true, true, true), true),
            AutoApplyVerdict::Skip
        );
        // In flight (no failure) → Wait and re-check.
        assert_eq!(
            auto_apply_verdict(gate(true, true, false), true),
            AutoApplyVerdict::Wait
        );
    }

    #[test]
    fn save_action_targets_the_project_controllers_save_overlay() {
        let action = save_overlay_action();
        assert!(action.is_for_node(ProjectController::NODE_ID));
        assert_eq!(action.op_as::<ProjectOp>(), Some(&ProjectOp::SaveOverlay));
    }

    #[test]
    fn bar_chrome_is_gentle_by_construction() {
        // Fixed height + plain background: the no-reflow, no-flash shell.
        assert!(BAR_CLASS.contains("tw:h-8"));
        assert!(BAR_CLASS.contains("tw:bg-card-subtle"));
        // The applying dot only ever changes opacity, with a transition.
        assert!(applying_dot_class(true).contains("tw:opacity-70"));
        assert!(applying_dot_class(false).contains("tw:opacity-0"));
        assert!(applying_dot_class(false).contains("tw:transition-opacity"));
        // Persistence chrome keeps constant geometry across enable/disable
        // and eases its colors.
        for enabled in [true, false] {
            let class = persist_button_class(enabled);
            assert!(class.contains("tw:px-2"));
            assert!(class.contains("tw:transition"));
        }
        assert!(persistence_word_class(true).contains("status-warning"));
        assert!(persistence_word_class(false).contains("tw:text-dim-foreground"));
        assert!(persistence_word_class(true).contains("tw:transition-colors"));
        // The error block eases in rather than snapping.
        assert!(ERROR_BLOCK_CLASS.contains("tw:starting:opacity-0"));
    }
}
