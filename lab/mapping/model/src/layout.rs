//! Layout: where every node sits on the canvas. Automatic and derived,
//! except where you have pinned a node by dragging it (D48, amending D37).
//!
//! It works inside-out (D43): each container lays out its children, then
//! sits in its parent as one block. Inside any container:
//!
//! 1. **pinned nodes** sit where you dropped them, in the container's own
//!    coordinates (a module's inside starts at its padding, so a pinned
//!    node moves with its module). A pin inside a module never goes above
//!    or left of its inside. Pinned nodes are placed first, in authoring
//!    order; one dropped on top of an earlier one moves down until it's
//!    clear.
//! 2. **nodes the lab doesn't edit** (a shader, a clock: `OtherNode`) stack
//!    in a column at the left, since they feed what is to their right;
//! 3. **fixtures and modules** stack top to bottom in authoring order, a
//!    gap between each, stepping down past anything pinned in the way;
//! 4. **each output** sits just to the right of the first fixture that
//!    feeds it, a fixed gap away, level with its top — so a patch edge is
//!    short and mostly runs left to right. An output fed by several
//!    fixtures sits beside the first; one fed by none goes to the right of
//!    everything. Outputs move down to clear whatever is there.
//!
//! Nothing laid out overlaps anything else at its level. A module's frame
//! is its children's box plus padding, with room for their title tabs.
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
/// Between a fixture and the output beside it: room for the edges.
pub const OUTPUT_GAP: f64 = 30.0;
/// Inside a module's frame, around its children…
pub const MODULE_PAD: f64 = 10.0;
/// …and above them, so their title tabs sit inside the frame.
pub const MODULE_PAD_TOP: f64 = 18.0;
/// An empty module's inside.
pub const EMPTY_MODULE: Vec2 = Vec2::new(40.0, 20.0);
/// An output card: its width, its title header, and one row per port. A
/// row holds the port's name over its pixels, as the board shows them.
pub const CARD_WIDTH: f64 = 60.0;
pub const CARD_HEADER: f64 = 11.0;
pub const CARD_ROW: f64 = 14.0;
pub const CARD_FOOT: f64 = 2.0;
/// A port's pixels sit in its row under its name (`pixel_grid.rs`): this far
/// in from each side, from this far below the row's top, and this far above
/// its bottom.
pub const PIXEL_PAD_X: f64 = 2.7;
pub const PIXEL_TOP: f64 = 7.0;
pub const PIXEL_BOTTOM: f64 = 1.4;

/// Every node's box, in world units.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Layout {
    pub rects: BTreeMap<NodeId, Rect>,
}

impl Project {
    /// The whole project laid out, the root's inside at `(0, 0)`. Kept
    /// until the project changes (`derived.rs`).
    pub fn layout(&self) -> &Layout {
        self.kept_layout()
    }

    /// Lay the whole project out, afresh.
    pub(crate) fn compute_layout(&self) -> Layout {
        let (rects, _) = self.lay_out(self.nodes(), false);
        Layout {
            rects: rects.into_iter().collect(),
        }
    }

    /// A node's box in world units.
    pub fn node_rect(&self, id: &NodeId) -> Option<Rect> {
        self.layout().rects.get(id).copied()
    }

    /// Where a fixture's `(0, 0)` lands in world units.
    pub fn origin(&self, id: &NodeId) -> Vec2 {
        self.node_rect(id).map(|r| r.min).unwrap_or_default()
    }

    /// Where the inside of the container holding `id` starts, in world
    /// units: the canvas's `(0, 0)` at the root, a module's padding inside
    /// it. A pin is measured from here.
    pub fn container_origin(&self, id: &NodeId) -> Vec2 {
        match self.parent_module(id).and_then(|m| self.node_rect(m)) {
            Some(r) => r.min + Vec2::new(MODULE_PAD, MODULE_PAD_TOP),
            None => Vec2::default(),
        }
    }

    /// Where a node sits in its container's coordinates: what pinning it
    /// right now would store.
    pub fn node_position(&self, id: &NodeId) -> Option<Vec2> {
        Some(self.node_rect(id)?.min - self.container_origin(id))
    }

    /// Pin a node at `at`, in its container's coordinates (D48). Inside a
    /// module a pin stays inside its padding.
    pub fn pin(&mut self, id: &NodeId, at: Vec2) {
        let at = if self.parent_module(id).is_some() {
            Vec2::new(at.x.max(0.0), at.y.max(0.0))
        } else {
            at
        };
        if let Some(n) = self.node_mut(id) {
            n.pinned = Some(at);
        }
    }

    /// Give a node back to the automatic layout. Returns whether it was
    /// pinned.
    pub fn unpin(&mut self, id: &NodeId) -> bool {
        self.node_mut(id).is_some_and(|n| n.pinned.take().is_some())
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
        union(self.layout().rects.values().copied())
            .unwrap_or(Rect::new(Vec2::default(), Vec2::new(100.0, 60.0)))
    }

    /// Lay out `nodes` in a container whose inside starts at `(0, 0)`.
    /// Returns the box of every node under them, theirs and everything
    /// inside, in that container's coordinates, and the inside's size.
    fn lay_out(&self, nodes: &[Node], in_module: bool) -> (Vec<(NodeId, Rect)>, Vec2) {
        let blocks: Vec<Block> = nodes.iter().map(|n| self.block(n)).collect();
        // Each node's box at this level, once placed.
        let mut placed: Vec<Option<Rect>> = vec![None; nodes.len()];
        let taken =
            |placed: &[Option<Rect>]| -> Vec<Rect> { placed.iter().flatten().copied().collect() };

        // 1. Pinned nodes, where they were dropped.
        for (i, n) in nodes.iter().enumerate() {
            if let Some(pin) = n.pinned {
                let pin = if in_module {
                    Vec2::new(pin.x.max(0.0), pin.y.max(0.0))
                } else {
                    pin
                };
                let at = clear_of(pin, blocks[i].size, &taken(&placed));
                placed[i] = Some(Rect::new(at, at + blocks[i].size));
            }
        }
        // 2. Nodes the lab doesn't edit (a shader, a clock), in a column
        //    at the left: they feed what's to their right.
        let mut y = 0.0;
        let mut left = 0.0_f64;
        for (i, n) in nodes.iter().enumerate() {
            if placed[i].is_some() || n.other().is_none() {
                continue;
            }
            let at = clear_of(Vec2::new(0.0, y), blocks[i].size, &taken(&placed));
            placed[i] = Some(Rect::new(at, at + blocks[i].size));
            y = at.y + blocks[i].size.y + NODE_GAP;
            left = left.max(at.x + blocks[i].size.x + OUTPUT_GAP);
        }
        // 3. Fixtures and modules, stacked.
        let mut y = 0.0;
        for (i, n) in nodes.iter().enumerate() {
            if placed[i].is_some() || n.is_card() {
                continue;
            }
            let at = clear_of(Vec2::new(left, y), blocks[i].size, &taken(&placed));
            placed[i] = Some(Rect::new(at, at + blocks[i].size));
            y = at.y + blocks[i].size.y + NODE_GAP;
        }
        // 4. Outputs, beside what feeds them.
        let right = taken(&placed)
            .iter()
            .map(|r| r.max.x)
            .fold(0.0_f64, f64::max);
        let mut unfed_y = 0.0;
        for (i, n) in nodes.iter().enumerate() {
            if placed[i].is_some() {
                continue;
            }
            let want = match self.feeder(nodes, &blocks, &placed, &n.id) {
                Some((block, fixture)) => Vec2::new(block.max.x + OUTPUT_GAP, fixture.min.y),
                None => Vec2::new(right + OUTPUT_GAP, unfed_y),
            };
            let at = clear_of(want, blocks[i].size, &taken(&placed));
            placed[i] = Some(Rect::new(at, at + blocks[i].size));
            if self.feeder(nodes, &blocks, &placed, &n.id).is_none() {
                unfed_y = at.y + blocks[i].size.y + NODE_GAP;
            }
        }

        let mut out = Vec::new();
        let mut size = Vec2::default();
        for (i, b) in blocks.into_iter().enumerate() {
            let Some(r) = placed[i] else { continue };
            size = Vec2::new(size.x.max(r.max.x), size.y.max(r.max.y));
            out.push((nodes[i].id.clone(), r));
            out.extend(
                b.inner
                    .into_iter()
                    .map(|(id, x)| (id, Rect::new(x.min + r.min, x.max + r.min))),
            );
        }
        (out, size)
    }

    /// One node as a block: its size, and the boxes inside it relative to
    /// its top-left.
    fn block(&self, n: &Node) -> Block {
        match &n.kind {
            NodeKind::Fixture(f) => Block {
                size: f.size,
                inner: Vec::new(),
            },
            NodeKind::Output(_) | NodeKind::Other(_) => Block {
                size: card_size(n),
                inner: Vec::new(),
            },
            NodeKind::Module { children } => {
                let (rects, inside) = if children.is_empty() {
                    (Vec::new(), EMPTY_MODULE)
                } else {
                    self.lay_out(children, true)
                };
                let pad = Vec2::new(MODULE_PAD, MODULE_PAD_TOP);
                Block {
                    size: inside + Vec2::new(2.0 * MODULE_PAD, MODULE_PAD_TOP + MODULE_PAD),
                    inner: rects
                        .into_iter()
                        .map(|(id, r)| (id, Rect::new(r.min + pad, r.max + pad)))
                        .collect(),
                }
            }
        }
    }

    /// What an output at this level sits beside: the placed block holding
    /// the first fixture (tree order) with a rule to it, and that fixture's
    /// own box. `None` if nothing placed here feeds it.
    fn feeder(
        &self,
        nodes: &[Node],
        blocks: &[Block],
        placed: &[Option<Rect>],
        output: &NodeId,
    ) -> Option<(Rect, Rect)> {
        for (i, n) in nodes.iter().enumerate() {
            if n.output().is_some() {
                continue;
            }
            let Some(r) = placed[i] else { continue };
            let mut all = Vec::new();
            crate::node::walk(std::slice::from_ref(n), &mut all);
            let Some(f) = all.into_iter().find(|x| {
                x.fixture().is_some_and(|f| {
                    f.rules
                        .iter()
                        .any(|rule| matches!(rule, PatchRule::Auto { output: o } if o == output))
                })
            }) else {
                continue;
            };
            let fixture = if f.id == n.id {
                r
            } else {
                let inner = blocks[i].inner.iter().find(|(id, _)| *id == f.id)?.1;
                Rect::new(inner.min + r.min, inner.max + r.min)
            };
            return Some((r, fixture));
        }
        None
    }
}

/// A node laid out on its own, before it is placed.
struct Block {
    size: Vec2,
    /// Every box inside it, relative to its top-left.
    inner: Vec<(NodeId, Rect)>,
}

/// The first place at or below `want` where a box of `size` overlaps
/// nothing `taken`. It steps down past whatever is in the way, a
/// [`NODE_GAP`] below it. Only a real overlap moves it: a node dropped
/// close to another, or a hair above where it was, leaves the rest alone.
fn clear_of(want: Vec2, size: Vec2, taken: &[Rect]) -> Vec2 {
    let mut at = want;
    loop {
        let r = Rect::new(at, at + size);
        let blocking = taken
            .iter()
            .filter(|t| {
                r.min.x < t.max.x && t.min.x < r.max.x && r.min.y < t.max.y && t.min.y < r.max.y
            })
            .map(|t| t.max.y)
            .fold(None, |a: Option<f64>, b| Some(a.map_or(b, |a| a.max(b))));
        match blocking {
            Some(bottom) => at.y = bottom + NODE_GAP,
            None => return at,
        }
    }
}

/// A card's size: a header and one row per port (a node the lab doesn't
/// edit has one row, saying what it is).
fn card_size(n: &Node) -> Vec2 {
    let ports = n.output().map_or(1, |o| o.ports.len());
    Vec2::new(
        CARD_WIDTH,
        CARD_HEADER + CARD_ROW * ports as f64 + CARD_FOOT,
    )
}
