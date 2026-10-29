//! A card's own geometry, in card-local units: its width, its title header,
//! and one row per port. Core declares every node's size (Q2): the layout
//! places a node at that size, and the page draws it at that size.
//!
//! A card zooms with the canvas, like a fixture does: its units are world
//! units.

use crate::domain::geom::{Rect, Vec2};
use crate::domain::node::{Node, NodeId, NodeKind};
use crate::domain::project::Project;

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

/// A node's size as core declares it: a fixture's is its box; a card's is
/// a header and one row per port (any other card has one row, saying what
/// it is, and a render source's its picture under it); a module's is
/// nothing, since it wraps its children.
pub fn node_size(n: &Node) -> Vec2 {
    match &n.kind {
        NodeKind::Fixture(f) => f.size,
        NodeKind::Module { .. } => Vec2::default(),
        NodeKind::Output(o) => card_size(o.ports.len()),
        // A render source's card shows its picture under its row.
        NodeKind::Source(_) => card_size(1) + Vec2::new(0.0, SOURCE_PICTURE),
        NodeKind::Other(_) => card_size(1),
    }
}

/// How tall a render source's picture is on its card, under its row.
pub const SOURCE_PICTURE: f64 = 30.0;

/// A card with `rows` rows.
pub fn card_size(rows: usize) -> Vec2 {
    Vec2::new(CARD_WIDTH, CARD_HEADER + CARD_ROW * rows as f64 + CARD_FOOT)
}

/// Row `i` on a card `width` wide, in card-local units. A card is
/// [`CARD_WIDTH`] on the canvas; in the stack it is the column's width,
/// and its rows span it.
pub fn card_row(i: usize, width: f64) -> Rect {
    let top = CARD_HEADER + i as f64 * CARD_ROW;
    Rect::new(Vec2::new(0.0, top), Vec2::new(width, top + CARD_ROW))
}

impl Project {
    /// One port's row on its output's card, in world units.
    pub fn port_rect(&self, output: &NodeId, key: &str) -> Option<Rect> {
        let card = self.node_rect(output)?;
        let origin = card.min;
        let i = self
            .output(output)?
            .ports
            .iter()
            .position(|p| p.key == key)?;
        let r = card_row(i, card.width());
        Some(Rect::new(r.min + origin, r.max + origin))
    }
}
