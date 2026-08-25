//! The editor keyboard grammar: tools, view toggles, session undo, the
//! esc ladder, delete, and path-finish. The host routes keydown here while
//! a document is being edited; the ladder's LAST rung is the host's —
//! [`EditorKeyOutcome::ExitDive`] says "esc had nothing left to undo,
//! leave the dive" (Q4 of the one-project-canvas plan).
//!
//! Input arrives as a parsed [`EditorKeyInput`] rather than a DOM event:
//! hosts route keys from a view-scoped WINDOW listener (so the grammar
//! survives focus wandering into a dock), and a window listener speaks
//! `web_sys` while component handlers speak Dioxus. Both convert here at
//! the boundary; [`EditorKeyResult::prevent_default`] carries the one DOM
//! effect back out.

use core::str::FromStr as _;

use dioxus::prelude::*;

use crate::editor_core::editor_session::MapEditorSession;
use crate::editor_core::map_tool::MapTool;
use crate::view::view_options::EditorViewOptions;

/// A keydown, parsed off its DOM event at the host boundary.
#[derive(Clone, Debug, PartialEq)]
pub struct EditorKeyInput {
    pub key: Key,
    pub modifiers: Modifiers,
}

impl EditorKeyInput {
    /// From a Dioxus keyboard event (component-level handlers).
    #[must_use]
    pub fn from_event(evt: &Event<KeyboardData>) -> Self {
        Self {
            key: evt.data().key(),
            modifiers: evt.data().modifiers(),
        }
    }

    /// From a raw key string + modifier flags (window-level listeners
    /// parse their `web_sys` event into these at the host boundary).
    #[must_use]
    pub fn from_raw(key: &str, meta: bool, ctrl: bool, shift: bool, alt: bool) -> Self {
        let mut modifiers = Modifiers::empty();
        if meta {
            modifiers |= Modifiers::META;
        }
        if ctrl {
            modifiers |= Modifiers::CONTROL;
        }
        if shift {
            modifiers |= Modifiers::SHIFT;
        }
        if alt {
            modifiers |= Modifiers::ALT;
        }
        Self {
            key: Key::from_str(key).unwrap_or(Key::Unidentified),
            modifiers,
        }
    }
}

/// What a routed key asked of the host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditorKeyOutcome {
    /// Consumed (or ignored) inside the editor grammar.
    Handled,
    /// Esc walked off the end of the ladder: back out vertex → drop
    /// vertex → ascend group → clear selection → reset tool → EXIT DIVE.
    ExitDive,
}

/// The routed key's outcome plus whether the host must `preventDefault`
/// on the originating DOM event (browser-native ⌘Z/⌘A/⌫ behavior).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EditorKeyResult {
    pub outcome: EditorKeyOutcome,
    pub prevent_default: bool,
}

impl EditorKeyResult {
    fn handled(prevent_default: bool) -> Self {
        Self {
            outcome: EditorKeyOutcome::Handled,
            prevent_default,
        }
    }
}

pub fn handle_editor_key(
    mut session: Signal<MapEditorSession>,
    mut view_opts: Signal<EditorViewOptions>,
    mut fit_pending: Signal<bool>,
    on_committed: EventHandler<()>,
    input: &EditorKeyInput,
) -> EditorKeyResult {
    let modifiers = input.modifiers;
    let command = modifiers.meta() || modifiers.ctrl();
    match &input.key {
        Key::Character(text) => {
            let key = text.to_lowercase();
            if command {
                match key.as_str() {
                    "z" => {
                        if modifiers.shift() {
                            session.write().redo();
                        } else {
                            session.write().undo();
                        }
                        on_committed.call(());
                        return EditorKeyResult::handled(true);
                    }
                    "a" => {
                        session.write().select_all();
                        return EditorKeyResult::handled(true);
                    }
                    _ => {}
                }
                return EditorKeyResult::handled(false);
            }
            match key.as_str() {
                "v" => session.write().tool = MapTool::Select,
                "g" => session.write().tool = MapTool::Grid,
                "r" => session.write().tool = MapTool::Ring,
                "p" => session.write().tool = MapTool::path(),
                "n" => {
                    let current = view_opts.peek().numbers;
                    view_opts.write().numbers = !current;
                }
                "a" => {
                    let current = view_opts.peek().arrows;
                    view_opts.write().arrows = !current;
                }
                "l" => {
                    let current = view_opts.peek().live;
                    view_opts.write().live = !current;
                }
                "f" => {
                    let current = view_opts.peek().fit_preview;
                    view_opts.write().fit_preview = !current;
                }
                "b" => {
                    let current = view_opts.peek().reference;
                    view_opts.write().reference = !current;
                }
                "0" => fit_pending.set(true),
                _ => {}
            }
            EditorKeyResult::handled(false)
        }
        Key::Escape => {
            let mut s = session.write();
            // The ladder (D6 + the selection/tree ADR, extended by Q4):
            // back out one path vertex, then drop a vertex sub-selection,
            // then ASCEND out of a descended group, then clear selection,
            // then reset the tool — and with nothing left, the dive itself
            // is the thing esc leaves.
            if s.path_backout() {
                return EditorKeyResult::handled(false);
            }
            if s.selection.vertex.is_some() {
                s.selection.vertex = None;
                return EditorKeyResult::handled(false);
            }
            if s.ascend() {
                return EditorKeyResult::handled(false);
            }
            if !s.selection.is_empty() {
                s.selection.clear();
                return EditorKeyResult::handled(false);
            }
            if !matches!(s.tool, MapTool::Select) {
                s.tool = MapTool::Select;
                return EditorKeyResult::handled(false);
            }
            EditorKeyResult {
                outcome: EditorKeyOutcome::ExitDive,
                prevent_default: false,
            }
        }
        Key::Enter => {
            if matches!(session.peek().tool, MapTool::Path { .. })
                && session.write().path_finish().is_some()
            {
                on_committed.call(());
            }
            EditorKeyResult::handled(false)
        }
        Key::Backspace | Key::Delete => {
            let had_selection = !session.peek().selection.is_empty();
            if had_selection {
                session.write().delete_selection();
                on_committed.call(());
                return EditorKeyResult::handled(true);
            }
            EditorKeyResult::handled(false)
        }
        _ => EditorKeyResult::handled(false),
    }
}
