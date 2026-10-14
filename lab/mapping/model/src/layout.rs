//! Layout: where every node sits on the canvas. Automatic, and derived —
//! never stored (D37). Hand-placed positions would be persisted data.
//!
//! It works inside-out (D43): each container lays out its children, then
//! sits in its parent as one block. Inside any container there are two
//! columns:
//!
//! - **left:** what isn't an output (fixtures, modules), top to bottom in
//!   authoring order, a gap between each;
//! - **right:** the outputs, each at the height of the first fixture
//!   patched to it, pushed down so no two overlap.
//!
//! So a patch edge mostly runs left to right, and a fixture sits beside
//! the output it feeds. A module's frame is its children's box plus
//! padding, with room for their title tabs.
//!
//! Everything here is in world units, including output cards: a card zooms
//! with the canvas like a fixture does.

use std::collections::BTreeMap;

use crate::geom::{Rect, Vec2};
use crate::node::{Node, NodeId, NodeKind};
use crate::patch::PatchRule;
use crate::project::{Project, union};

/// Between nodes stacked in a column.
pub const NODE_GAP: f64 = 24.0;
/// Between the left column and the outputs: room for the edges.
pub const COLUMN_GAP: f64 = 56.0;
/// Inside a module's frame, around its children…
pub const MODULE_PAD: f64 = 10.0;
/// …and above them, so their title tabs sit inside the frame.
pub const MODULE_PAD_TOP: f64 = 18.0;
/// An empty module's inside.
pub const EMPTY_MODULE: Vec2 = Vec2::new(40.0, 20.0);
/// An output card: its width, its title header, and one row per port.
pub const CARD_WIDTH: f64 = 60.0;
pub const CARD_HEADER: f64 = 11.0;
pub const CARD_ROW: f64 = 9.0;
pub const CARD_FOOT: f64 = 2.0;

/// Every node's box, in world units.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Layout {
    pub rects: BTreeMap<NodeId, Rect>,
}

impl Project {
    /// Lay the whole project out, the root at `(0, 0)`.
    pub fn layout(&self) -> Layout {
        let mut layout = Layout::default();
        self.lay_out(&self.nodes, Vec2::default(), &mut layout);
        layout
    }

    /// A node's box in world units.
    pub fn node_rect(&self, id: &NodeId) -> Option<Rect> {
        self.layout().rects.get(id).copied()
    }

    /// Where a fixture's `(0, 0)` lands in world units.
    pub fn origin(&self, id: &NodeId) -> Vec2 {
        self.node_rect(id).map(|r| r.min).unwrap_or_default()
    }

    /// One port's row on its output's card.
    pub fn port_rect(&self, output: &NodeId, key: &str) -> Option<Rect> {
        let card = self.node_rect(output)?;
        let i = self
            .output(output)?
            .ports
            .iter()
            .position(|p| p.key == key)?;
        let top = card.min.y + CARD_HEADER + i as f64 * CARD_ROW;
        Some(Rect::new(
            Vec2::new(card.min.x, top),
            Vec2::new(card.max.x, top + CARD_ROW),
        ))
    }

    /// Everything on the canvas, for "fit".
    pub fn extent(&self) -> Rect {
        union(self.layout().rects.into_values())
            .unwrap_or(Rect::new(Vec2::default(), Vec2::new(100.0, 60.0)))
    }

    /// Place `nodes` with their top-left at `at`; returns their extent.
    fn lay_out(&self, nodes: &[Node], at: Vec2, layout: &mut Layout) -> Vec2 {
        let (outputs, left): (Vec<&Node>, Vec<&Node>) =
            nodes.iter().partition(|n| n.output().is_some());

        let mut y = 0.0;
        let mut left_w: f64 = 0.0;
        let mut left_h: f64 = 0.0;
        for n in &left {
            let top_left = at + Vec2::new(0.0, y);
            let size = match &n.kind {
                NodeKind::Fixture(f) => f.size,
                NodeKind::Module { children } => {
                    let inner = if children.is_empty() {
                        EMPTY_MODULE
                    } else {
                        self.lay_out(
                            children,
                            top_left + Vec2::new(MODULE_PAD, MODULE_PAD_TOP),
                            layout,
                        )
                    };
                    inner + Vec2::new(2.0 * MODULE_PAD, MODULE_PAD_TOP + MODULE_PAD)
                }
                NodeKind::Output(_) => unreachable!("outputs go right"),
            };
            layout
                .rects
                .insert(n.id.clone(), Rect::new(top_left, top_left + size));
            left_w = left_w.max(size.x);
            left_h = y + size.y;
            y += size.y + NODE_GAP;
        }

        let x = if left.is_empty() {
            0.0
        } else {
            left_w + COLUMN_GAP
        };
        let mut free = 0.0;
        let mut right_h: f64 = 0.0;
        for n in &outputs {
            let wanted = self
                .first_fixture_patched_to(&left, &n.id)
                .and_then(|f| layout.rects.get(&f))
                .map(|r| r.min.y - at.y)
                .unwrap_or(free);
            let top = wanted.max(free);
            let size = card_size(n);
            let top_left = at + Vec2::new(x, top);
            layout
                .rects
                .insert(n.id.clone(), Rect::new(top_left, top_left + size));
            right_h = top + size.y;
            free = top + size.y + NODE_GAP;
        }

        let width = if outputs.is_empty() {
            left_w
        } else {
            x + CARD_WIDTH
        };
        Vec2::new(width, left_h.max(right_h))
    }

    /// The first fixture under `nodes` (tree order) with a rule to `output`.
    fn first_fixture_patched_to(&self, nodes: &[&Node], output: &NodeId) -> Option<NodeId> {
        let mut all = Vec::new();
        for n in nodes {
            crate::node::walk(std::slice::from_ref(*n), &mut all);
        }
        all.into_iter()
            .find(|n| {
                n.fixture().is_some_and(|f| {
                    f.rules
                        .iter()
                        .any(|r| matches!(r, PatchRule::Auto { output: o } if o == output))
                })
            })
            .map(|n| n.id.clone())
    }
}

/// An output card's size: a header and one row per port.
fn card_size(n: &Node) -> Vec2 {
    let ports = n.output().map_or(0, |o| o.ports.len());
    Vec2::new(
        CARD_WIDTH,
        CARD_HEADER + CARD_ROW * ports as f64 + CARD_FOOT,
    )
}
