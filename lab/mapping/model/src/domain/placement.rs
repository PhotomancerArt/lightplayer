//! Placement: where each node sits, in world units.
//!
//! The model never lays anything out. Layout runs in the view layer (D63):
//! the page builds the layout input (`Editor::layout_input`), runs
//! `lab-layout`, and hands the result back as a fact
//! (`Editor::set_placement`). The model reads where things landed: to put
//! each fixture's drawing in world units, to pick, and to frame. Until a
//! placement is set, every node sits at the origin and nothing is
//! pickable.
//!
//! A project carries its placement beside what it holds. It is not part of
//! what the project holds: two projects are equal whatever the view did
//! with them, and an edit keeps the placement until the view hands over
//! the next one.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::domain::geom::{Rect, Vec2};
use crate::domain::node::NodeId;
use crate::domain::project::{Project, union};

/// Every node's box, and where each module's inside starts.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Placement {
    /// Every node's box, in world units, the root's inside at `(0, 0)`.
    pub rects: BTreeMap<NodeId, Rect>,
    /// Where each module's inside starts, in world units: what a pin in
    /// that module is measured from.
    pub insides: BTreeMap<NodeId, Vec2>,
}

/// The placement a project carries. Never compared: see the module docs.
#[derive(Debug, Clone, Default)]
pub(crate) struct Carried(pub(crate) Arc<Placement>);

impl PartialEq for Carried {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl Project {
    /// Where the view placed every node.
    pub fn placement(&self) -> &Placement {
        &self.placement.0
    }

    /// Take where the view placed every node. Returns whether anything
    /// moved; when it did, what was worked out in world units is worked out
    /// again.
    pub fn set_placement(&mut self, placement: Placement) -> bool {
        if *self.placement.0 == placement {
            return false;
        }
        self.placement = Carried(Arc::new(placement));
        self.moved();
        true
    }

    /// A node's box in world units.
    pub fn node_rect(&self, id: &NodeId) -> Option<Rect> {
        self.placement().rects.get(id).copied()
    }

    /// Where a fixture's `(0, 0)` lands in world units.
    pub fn origin(&self, id: &NodeId) -> Vec2 {
        self.node_rect(id).map(|r| r.min).unwrap_or_default()
    }

    /// Where the inside of the container holding `id` starts, in world
    /// units: the canvas's `(0, 0)` at the root, a module's padding inside
    /// it. A pin is measured from here.
    pub fn container_origin(&self, id: &NodeId) -> Vec2 {
        self.parent_module(id)
            .and_then(|m| self.placement().insides.get(m).copied())
            .unwrap_or_default()
    }

    /// Where a node sits in its container's coordinates: what pinning it
    /// right now would store.
    pub fn node_position(&self, id: &NodeId) -> Option<Vec2> {
        Some(self.node_rect(id)?.min - self.container_origin(id))
    }

    /// Everything on the canvas, for "fit".
    pub fn extent(&self) -> Rect {
        union(self.placement().rects.values().copied())
            .unwrap_or(Rect::new(Vec2::default(), Vec2::new(100.0, 60.0)))
    }

    /// The fixture whose box holds a world point.
    pub fn fixture_at(&self, p: Vec2) -> Option<NodeId> {
        self.fixtures()
            .into_iter()
            .find(|n| self.node_rect(&n.id).is_some_and(|r| r.contains(p)))
            .map(|n| n.id.clone())
    }
}
