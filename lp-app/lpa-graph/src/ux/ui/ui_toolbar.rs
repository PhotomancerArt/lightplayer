//! The toolbar: the tool, Move & draw, the layout, and the Debug switches
//! at its right end.

use crate::ux::debug_flags::Presentation;
use crate::ux::editor::Tool;
use crate::ux::ui::{LayoutKind, SizeClass};

#[derive(Debug, Clone, PartialEq)]
pub struct UiToolbar {
    pub tool: Tool,
    /// Drawing tools need Move & draw on.
    pub move_draw: bool,
    /// The layout in use, and whether it is the one you chose (or the
    /// screen size's default).
    pub layout: LayoutKind,
    pub layout_chosen: bool,
    /// The size class Debug forces, if any.
    pub forced_size_class: Option<SizeClass>,
    /// Debug: how the inspector shows a start against its flow (Q82).
    pub presentation: Presentation,
    /// Debug: the patcher's steps under the inspector.
    pub cursor_ops: bool,
}
