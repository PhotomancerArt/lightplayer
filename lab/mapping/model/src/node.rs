//! Nodes: the things on the canvas. A fixture is one kind of node; an
//! output is another; a module holds nodes (D42, D43).
//!
//! The project is the root module: its `nodes` are the root's children, so
//! the canvas is a view of the root module's insides. Any node that holds
//! nodes — a module, a fixture with its drawing — is a *frame*: see-through
//! for picking, picked by its title tab or border, and Esc climbs to it.
//!
//! A node's id is minted by the project (`module1`, `fixture2`, `output1`)
//! and never reused, so a patch rule that names an output keeps naming it
//! — even after the output is gone, when the rule becomes a problem rather
//! than quietly pointing somewhere else.
//!
//! Nothing here assumes there is only one project (D44): ids are unique
//! within their project, and a level above the project can be added later
//! the way modules were.

use crate::fixture::Fixture;
use crate::output::Output;

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
}

#[derive(Debug, Clone, PartialEq)]
pub enum NodeKind {
    /// A frame around other nodes, laid out inside it.
    Module { children: Vec<Node> },
    /// One physical thing, and what's drawn in it.
    Fixture(Fixture),
    /// Where pixels leave the board: a card with ports.
    Output(Output),
}

impl Node {
    /// What the user calls this kind of node.
    pub fn kind_word(&self) -> &'static str {
        match self.kind {
            NodeKind::Module { .. } => "module",
            NodeKind::Fixture(_) => "fixture",
            NodeKind::Output(_) => "output",
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
