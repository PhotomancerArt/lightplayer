//! The object tree: what a fixture's components render, as one tree.
//!
//! Every component renders exactly one node here (theory "Components and
//! objects"): a path, a polygon, a grid, a ring set or a filled polygon a
//! leaf that holds its lamps; a circle a container of its ring leaves; a
//! group a container of its children's nodes; a repeat a container of its
//! copies, where copy 0 is its child's own node and copy `k` the same
//! subtree with every id in copy `k` (`panel@k`) and every lamp turned.
//!
//! The walker (`walker.rs`) walks this tree depth first; the leaves in
//! that order are the order a fixture lists its lamps (`derived.rs`).

use crate::domain::component::{Component, ComponentKind};
use crate::domain::object::{Object, ObjectId, ring_leaves, single_leaf, turned_copy};
use crate::domain::resolve::Rotation;

/// One node of the object tree.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectNode {
    pub id: ObjectId,
    /// What this node is, for the tree and the inspector.
    pub kind: ObjectNodeKind,
    /// `Some` on a leaf: its lamps. Containers hold none.
    pub leaf: Option<Object>,
    /// In tree order. A leaf has none.
    pub children: Vec<ObjectNode>,
}

/// What made a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectNodeKind {
    /// A component's own node (copy 0, or a copy of it).
    Component,
    /// A node its component generated: a circle's ring.
    Generated,
}

impl ObjectNode {
    pub fn is_leaf(&self) -> bool {
        self.leaf.is_some()
    }

    /// Every leaf under it (itself, when it is one), in tree order.
    pub fn collect_leaves(self, out: &mut Vec<Object>) {
        if let Some(o) = self.leaf {
            out.push(o);
        }
        for child in self.children {
            child.collect_leaves(out);
        }
    }

    /// Every leaf under it, borrowed, in tree order.
    pub fn leaves(&self) -> Vec<&Object> {
        let mut out = Vec::new();
        self.push_leaves(&mut out);
        out
    }

    fn push_leaves<'a>(&'a self, out: &mut Vec<&'a Object>) {
        if let Some(o) = &self.leaf {
            out.push(o);
        }
        for child in &self.children {
            child.push_leaves(out);
        }
    }

    /// The node with this id, here or below.
    pub fn find(&self, id: &ObjectId) -> Option<&ObjectNode> {
        if &self.id == id {
            return Some(self);
        }
        self.children.iter().find_map(|c| c.find(id))
    }

    /// The same subtree in repeat copy `k`, turned.
    fn in_copy(&self, turn: &Rotation, k: u32) -> ObjectNode {
        ObjectNode {
            id: self.id.clone().in_copy(k),
            kind: self.kind,
            leaf: self.leaf.as_ref().map(|o| turned_copy(o, turn, k)),
            children: self.children.iter().map(|c| c.in_copy(turn, k)).collect(),
        }
    }
}

/// The node one component renders, with everything under it, every id in
/// its canonical form. `component` must not sit inside a repeat (its copies
/// would be missing from the ids): render a fixture's top-level components.
pub fn render(component: &Component) -> ObjectNode {
    canonical(render_raw(component))
}

fn canonical(node: ObjectNode) -> ObjectNode {
    ObjectNode {
        id: node.id.canonical(),
        leaf: node.leaf.map(|mut o| {
            o.id = o.id.canonical();
            o
        }),
        children: node.children.into_iter().map(canonical).collect(),
        kind: node.kind,
    }
}

fn render_raw(component: &Component) -> ObjectNode {
    let id = ObjectId::of(&component.id);
    match &component.kind {
        ComponentKind::Group { children } => ObjectNode {
            id,
            kind: ObjectNodeKind::Component,
            leaf: None,
            children: children.iter().map(render_raw).collect(),
        },
        ComponentKind::Circle { center, rings, .. } => ObjectNode {
            id,
            kind: ObjectNodeKind::Component,
            leaf: None,
            children: ring_leaves(&component.id, *center, rings)
                .into_iter()
                .map(|o| ObjectNode {
                    id: o.id.clone(),
                    kind: ObjectNodeKind::Generated,
                    leaf: Some(o),
                    children: Vec::new(),
                })
                .collect(),
        },
        ComponentKind::Repeat {
            child,
            center,
            count,
        } => {
            let original = render_raw(child);
            // Copy 0 is turned too (by nothing), as the product turns every
            // instance: its lamps must be the product's bit for bit.
            let children = (0..*count)
                .map(|k| original.in_copy(&Rotation::instance(*center, k, *count), k))
                .collect();
            ObjectNode {
                id,
                kind: ObjectNodeKind::Component,
                leaf: None,
                children,
            }
        }
        kind => ObjectNode {
            id: id.clone(),
            kind: ObjectNodeKind::Component,
            leaf: single_leaf(&component.id, kind),
            children: Vec::new(),
        },
    }
}
