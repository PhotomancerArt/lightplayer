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
    /// How much bigger than its own box each fixture is drawn: 1 on the
    /// canvas; in the stack, a fixture fills the column's width.
    pub scales: BTreeMap<NodeId, f64>,
}

/// Where a fixture's own units land in world units: its `(0, 0)` and how
/// big one of its units is. Everything drawn in a fixture goes through
/// this, both ways.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    pub origin: Vec2,
    pub scale: f64,
}

impl Frame {
    pub fn to_world(&self, p: Vec2) -> Vec2 {
        self.origin + p * self.scale
    }

    pub fn to_local(&self, w: Vec2) -> Vec2 {
        (w - self.origin) * (1.0 / self.scale)
    }

    /// A length in the fixture's units, in world units.
    pub fn len_world(&self, l: f64) -> f64 {
        l * self.scale
    }

    /// A move in world units, in the fixture's units.
    pub fn delta_local(&self, d: Vec2) -> Vec2 {
        d * (1.0 / self.scale)
    }
}

/// The placement a project carries, and how many it has been handed. Never
/// compared: see the module docs.
#[derive(Debug, Clone, Default)]
pub(crate) struct Carried(pub(crate) Arc<Placement>, u64);

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

    /// How many placements this project has been handed: what changes when
    /// nodes move without an edit (the revision stays put).
    pub fn placements(&self) -> u64 {
        self.placement.1
    }

    /// Take where the view placed every node. Returns whether anything
    /// moved; when it did, what was worked out in world units is worked out
    /// again.
    pub fn set_placement(&mut self, placement: Placement) -> bool {
        if *self.placement.0 == placement {
            return false;
        }
        self.placement = Carried(Arc::new(placement), self.placement.1 + 1);
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

    /// Where a fixture's own units land in world units.
    pub fn frame(&self, id: &NodeId) -> Frame {
        Frame {
            origin: self.origin(id),
            scale: self
                .placement()
                .scales
                .get(id)
                .copied()
                .filter(|s| *s > 0.0)
                .unwrap_or(1.0),
        }
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
