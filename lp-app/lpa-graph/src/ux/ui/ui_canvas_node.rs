//! One node on the canvas: what it is, how much of it shows, and how it
//! stands out. Never where it is.

use lab_layout::LayoutRole;

use crate::domain::geom::Vec2;
use crate::domain::node::NodeId;
use crate::ux::ui::{UiNodeFace, UiNodeHeader};

#[derive(Debug, Clone, PartialEq)]
pub struct UiCanvasNode {
    pub id: NodeId,
    /// The module it sits in; `None` at the root.
    pub parent: Option<NodeId>,
    /// What it is to the layout.
    pub role: LayoutRole,
    /// The size core declares, in world units (`card_geometry.rs`).
    pub size: Vec2,
    pub frame: FrameKind,
    pub header: UiNodeHeader,
    /// What it shows on the canvas; a module shows only its frame.
    pub face: Option<UiNodeFace>,
    pub detail: DetailLevel,
    pub emphasis: Emphasis,
    pub selected: bool,
}

/// How a node is drawn around its face.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameKind {
    /// A filled card: an output, a shader, a plain node.
    Card,
    /// A see-through frame around its children, picked by its title tab
    /// or border.
    Module,
    /// A see-through frame around what's drawn in it.
    Fixture,
}

/// How much of a card fits, from its width on screen (its declared width ×
/// the zoom). Core picks it without knowing where the card is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DetailLevel {
    /// Too small for anything inside: just the box.
    Dot,
    /// The face, with no words: text would be too small to read.
    Face,
    /// Everything, words too.
    Full,
}

/// How something stands out. One meaning each, everywhere it is used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Emphasis {
    /// As it is.
    #[default]
    Full,
    /// Lit by the selection: selected, or what the selection reaches.
    Lit,
    /// Something else is selected: it steps back.
    Quiet,
    /// Outside the level you are working in.
    Dimmed,
}
