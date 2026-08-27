//! The editor keyboard grammar: tools, view toggles, session undo, the
//! esc ladder, delete, and path-finish. The host routes keydown here while
//! a document is being edited; the ladder's LAST rung is the host's —
//! [`EditorKeyOutcome::ExitDive`] says "esc had nothing left to undo,
//! leave the dive" (Q4 of the one-project-canvas plan).

use dioxus::prelude::*;

use crate::editor_core::editor_session::MapEditorSession;
use crate::editor_core::map_tool::MapTool;
use crate::view::view_options::EditorViewOptions;

/// What a routed key asked of the host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditorKeyOutcome {
    /// Consumed (or ignored) inside the editor grammar.
    Handled,
    /// Esc walked off the end of the ladder: back out vertex → drop
    /// vertex → ascend group → clear selection → reset tool → EXIT DIVE.
    ExitDive,
}

pub fn handle_editor_key(
    mut session: Signal<MapEditorSession>,
    mut view_opts: Signal<EditorViewOptions>,
    mut fit_pending: Signal<bool>,
    on_committed: EventHandler<()>,
    evt: &Event<KeyboardData>,
) -> EditorKeyOutcome {
    let modifiers = evt.data().modifiers();
    let command = modifiers.meta() || modifiers.ctrl();
    match evt.data().key() {
        Key::Character(text) => {
            let key = text.to_lowercase();
            if command {
                match key.as_str() {
                    "z" => {
                        evt.prevent_default();
                        if modifiers.shift() {
                            session.write().redo();
                        } else {
                            session.write().undo();
                        }
                        on_committed.call(());
                    }
                    "a" => {
                        evt.prevent_default();
                        session.write().select_all();
                    }
                    _ => {}
                }
                return EditorKeyOutcome::Handled;
            }
            match key.as_str() {
                "v" => session.write().tool = MapTool::Select,
                "g" => session.write().tool = MapTool::Grid,
                "r" => session.write().tool = MapTool::Ring,
                "p" => session.write().tool = MapTool::path(),
                // `o` for outline: `p` is the path tool's and the polygon
                // tool's whole subject is a closed OUTLINE, whichever
                // population fills it.
                "o" => session.write().start_polygon_tool(),
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
            EditorKeyOutcome::Handled
        }
        Key::Escape => {
            let mut s = session.write();
            // The ladder (D6 + the selection/tree ADR, extended by Q4):
            // back out one path vertex, then drop a vertex sub-selection,
            // then ASCEND out of a descended group, then clear selection,
            // then reset the tool — and with nothing left, the dive itself
            // is the thing esc leaves.
            if s.path_backout() || s.polygon_backout() {
                return EditorKeyOutcome::Handled;
            }
            if s.selection.vertex.is_some() {
                s.selection.vertex = None;
                return EditorKeyOutcome::Handled;
            }
            if s.ascend() {
                return EditorKeyOutcome::Handled;
            }
            if !s.selection.is_empty() {
                s.selection.clear();
                return EditorKeyOutcome::Handled;
            }
            if !matches!(s.tool, MapTool::Select) {
                s.tool = MapTool::Select;
                return EditorKeyOutcome::Handled;
            }
            EditorKeyOutcome::ExitDive
        }
        Key::Enter => {
            // Enter is the ACCESSIBILITY fallback for both drawing tools:
            // the path's double-click and the polygon's close-on-first are
            // the primary gestures. A polygon draft with fewer than three
            // vertices refuses and KEEPS the draft (parent decision D11), so
            // the miss costs nothing.
            let tool = session.peek().tool.clone();
            let finished = match tool {
                MapTool::Path { .. } => session.write().path_finish(),
                MapTool::Polygon { .. } => session.write().polygon_finish(),
                _ => None,
            };
            if finished.is_some() {
                on_committed.call(());
            }
            EditorKeyOutcome::Handled
        }
        Key::Backspace | Key::Delete => {
            // Vertex-FIRST: with a corner selected the key is about that
            // corner, and a shape already at its floor (a run's two points, an
            // outline's three) simply refuses — `delete_selection` never lets a
            // refused corner become a deleted object.
            let had_selection = !session.peek().selection.is_empty();
            if had_selection {
                evt.prevent_default();
                session.write().delete_selection();
                on_committed.call(());
            }
            EditorKeyOutcome::Handled
        }
        _ => EditorKeyOutcome::Handled,
    }
}
