//! A node as the layout sees it: an id, where it sits in the tree, its
//! role and its size. Ids are strings so the crate needs no model types.

use crate::{LayoutRole, Vec2};

#[derive(Debug, Clone, PartialEq)]
pub struct LayoutNode {
    pub id: String,
    /// The module it sits in; `None` at the root. A parent that isn't a
    /// module in the input counts as the root.
    pub parent: Option<String>,
    pub role: LayoutRole,
    /// Fixed per kind; a fixture's is its authored box. A module's is
    /// ignored: it wraps its children.
    pub size: Vec2,
}

impl LayoutNode {
    pub fn new(id: impl Into<String>, role: LayoutRole, size: Vec2) -> Self {
        Self {
            id: id.into(),
            parent: None,
            role,
            size,
        }
    }

    /// The same node, inside module `parent`.
    pub fn in_module(mut self, parent: impl Into<String>) -> Self {
        self.parent = Some(parent.into());
        self
    }
}
