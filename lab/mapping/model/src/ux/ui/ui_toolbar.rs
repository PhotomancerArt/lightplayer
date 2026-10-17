//! The toolbar: the tool, Move & draw, undo and redo, the layout, and the
//! Debug switches at its right end.

use crate::ux::editor::Tool;
use crate::ux::ui::{LayoutKind, SizeClass};

#[derive(Debug, Clone, PartialEq)]
pub struct UiToolbar {
    pub tool: Tool,
    /// Drawing tools need Move & draw on.
    pub move_draw: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    /// The layout in use, and whether it is the one you chose (or the
    /// screen size's default).
    pub layout: LayoutKind,
    pub layout_chosen: bool,
    /// The screen's size class, and the one Debug forces, if any.
    pub size_class: SizeClass,
    pub forced_size_class: Option<SizeClass>,
    /// Debug: dragging a node pins it (D64).
    pub pinning: bool,
}
