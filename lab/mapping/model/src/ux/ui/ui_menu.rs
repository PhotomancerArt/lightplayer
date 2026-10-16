//! The right-click list: what you can do here, then everything under the
//! cursor.

use crate::domain::geom::Vec2;

#[derive(Debug, Clone, PartialEq)]
pub struct UiMenu {
    /// Where it opened, in screen pixels.
    pub at: Vec2,
    pub actions: Vec<UiMenuAction>,
    pub items: Vec<UiMenuItem>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UiMenuAction {
    pub label: String,
    /// The key or gesture that does the same.
    pub keys: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UiMenuItem {
    /// Indent: 0 for the shallowest thing listed.
    pub depth: usize,
    pub label: String,
    /// Under the pointer: outlined on the canvas too.
    pub hovered: bool,
}
