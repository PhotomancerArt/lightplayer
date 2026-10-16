//! Nodes: the things on the canvas, the product's node kinds (D52, D55).
//! A render source (a shader) makes a picture; a fixture samples it at its
//! lamps; an output sends lamps to pixels; a module holds nodes (D42,
//! D43); anything else is a plain card. They are wired by bus channels
//! (`channel.rs`, `bindings.rs`).
//!
//! The project is the root module: its `nodes` are the root's children, so
//! the canvas is a view of the root module's insides. Any node that holds
//! nodes — a module, a fixture with its drawing — is a *frame*: see-through
//! for picking, picked by its title tab or border, and Esc climbs to it.
//!
//! A node's id is minted by the project (`module1`, `fixture2`, `output1`)
//! and never reused, so a patch rule that names an output keeps naming it
//! — even after the output is gone, when the rule becomes a problem rather
//! than quietly pointing somewhere else. A node opened from a real project
//! keeps the product's name as its id (unique in the project; fit-check
//! §5.6).
//!
//! Nothing here assumes there is only one project (D44): ids are unique
//! within their project, and a level above the project can be added later
//! the way modules were.

use crate::fixture::Fixture;
use crate::geom::Vec2;
use crate::other_node::OtherNode;
use crate::output::Output;
use crate::render_source::RenderSource;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub String);

impl NodeId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub id: NodeId,
    pub name: String,
    pub kind: NodeKind,
    /// Where you dropped it, in its container's coordinates — `None` while
    /// the layout places it (D48). Authored, so in the app it is saved.
    pub pinned: Option<Vec2>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum NodeKind {
    /// A frame around other nodes, laid out inside it.
    Module { children: Vec<Node> },
    /// One physical thing, and what's drawn in it.
    Fixture(Fixture),
    /// Where pixels leave the board: a card with ports.
    Output(Output),
    /// A shader, or another node that makes a picture: a card writing its
    /// render product to a visual channel.
    Source(RenderSource),
    /// Anything else in a real project (a clock, a button): a plain card,
    /// carried along, never edited here.
    Other(OtherNode),
}

impl Node {
    /// What the user calls this kind of node.
    pub fn kind_word(&self) -> &'static str {
        match &self.kind {
            NodeKind::Module { .. } => "module",
            NodeKind::Fixture(_) => "fixture",
            NodeKind::Output(_) => "output",
            NodeKind::Source(s) if s.kind == "Shader" => "shader",
            NodeKind::Source(_) => "render source",
            NodeKind::Other(_) => "node",
        }
    }

    /// See-through for picking: a click inside hits what's inside.
    pub fn is_frame(&self) -> bool {
        matches!(self.kind, NodeKind::Module { .. } | NodeKind::Fixture(_))
    }

    /// The nodes inside a module; none for anything else.
    pub fn children(&self) -> &[Node] {
        match &self.kind {
            NodeKind::Module { children } => children,
            _ => &[],
        }
    }

    pub fn children_mut(&mut self) -> Option<&mut Vec<Node>> {
        match &mut self.kind {
            NodeKind::Module { children } => Some(children),
            _ => None,
        }
    }

    pub fn fixture(&self) -> Option<&Fixture> {
        match &self.kind {
            NodeKind::Fixture(f) => Some(f),
            _ => None,
        }
    }

    pub fn fixture_mut(&mut self) -> Option<&mut Fixture> {
        match &mut self.kind {
            NodeKind::Fixture(f) => Some(f),
            _ => None,
        }
    }

    pub fn output(&self) -> Option<&Output> {
        match &self.kind {
            NodeKind::Output(o) => Some(o),
            _ => None,
        }
    }

    pub fn output_mut(&mut self) -> Option<&mut Output> {
        match &mut self.kind {
            NodeKind::Output(o) => Some(o),
            _ => None,
        }
    }

    pub fn other(&self) -> Option<&OtherNode> {
        match &self.kind {
            NodeKind::Other(o) => Some(o),
            _ => None,
        }
    }

    pub fn source(&self) -> Option<&RenderSource> {
        match &self.kind {
            NodeKind::Source(s) => Some(s),
            _ => None,
        }
    }

    /// Drawn as a card (an output, a render source, a node the lab doesn't
    /// simulate), not a frame.
    pub fn is_card(&self) -> bool {
        matches!(
            self.kind,
            NodeKind::Output(_) | NodeKind::Source(_) | NodeKind::Other(_)
        )
    }

    /// A card that feeds what is to its right — a render source, or a node
    /// the lab doesn't simulate — so the layout puts it at the left.
    pub fn feeds_from_the_left(&self) -> bool {
        matches!(self.kind, NodeKind::Source(_) | NodeKind::Other(_))
    }
}

/// Depth-first, a node before what's inside it: the project's tree order.
pub fn walk<'a>(nodes: &'a [Node], out: &mut Vec<&'a Node>) {
    for n in nodes {
        out.push(n);
        walk(n.children(), out);
    }
}

pub(crate) fn find<'a>(nodes: &'a [Node], id: &NodeId) -> Option<&'a Node> {
    for n in nodes {
        if &n.id == id {
            return Some(n);
        }
        if let Some(found) = find(n.children(), id) {
            return Some(found);
        }
    }
    None
}

pub(crate) fn find_mut<'a>(nodes: &'a mut [Node], id: &NodeId) -> Option<&'a mut Node> {
    for n in nodes {
        if &n.id == id {
            return Some(n);
        }
        if let Some(children) = n.children_mut()
            && let Some(found) = find_mut(children, id)
        {
            return Some(found);
        }
    }
    None
}

/// The module holding `id`: `Some(None)` when it sits at the root.
pub(crate) fn parent_of<'a>(
    nodes: &'a [Node],
    id: &NodeId,
    parent: Option<&'a NodeId>,
) -> Option<Option<&'a NodeId>> {
    for n in nodes {
        if &n.id == id {
            return Some(parent);
        }
        if let Some(found) = parent_of(n.children(), id, Some(&n.id)) {
            return Some(found);
        }
    }
    None
}

pub(crate) fn remove_from(nodes: &mut Vec<Node>, id: &NodeId) -> Option<Node> {
    if let Some(i) = nodes.iter().position(|n| &n.id == id) {
        return Some(nodes.remove(i));
    }
    for n in nodes.iter_mut() {
        if let Some(children) = n.children_mut()
            && let Some(found) = remove_from(children, id)
        {
            return Some(found);
        }
    }
    None
}
