//! The toolbar: the tool, Move & draw, undo and redo, the layout, and the
//! Debug switches at its right end.

use crate::ux::editor::Tool;
use crate::ux::ui::LayoutKind;

#[derive(Debug, Clone, PartialEq)]
pub struct UiToolbar {
    pub tool: Tool,
    /// Drawing tools need Move & draw on.
    pub move_draw: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    pub layout: LayoutKind,
    /// Debug: dragging a node pins it (D64).
    pub pinning: bool,
}
