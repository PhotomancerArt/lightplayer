//! Where a port's pixels sit on its output's card.
//!
//! Each port row on a card shows its strip as the board has it: one square
//! per pixel, in data order, under the port's name. A long strip wraps onto
//! more lines, as many as make the squares biggest, so a 64-pixel port is
//! as easy to read (and to click) as a 30-pixel one.
//!
//! The grid lives in the model, not the page, because a pixel is something
//! you select (D47): its square is its area, so what's outlined is what a
//! click hits, and a patch edge lands on the square where its span starts
//! (D45). In world units, so it zooms with the card.

use crate::geom::{Rect, Vec2};
use crate::layout::{CARD_ROW, PIXEL_BOTTOM, PIXEL_PAD_X, PIXEL_TOP};
use crate::node::NodeId;
use crate::project::Project;

/// The most lines a port's pixels wrap onto.
pub const MAX_PIXEL_LINES: u32 = 4;

/// One port's pixels as squares on its card.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PixelGrid {
    /// The top-left of pixel 0's square, in world units.
    pub origin: Vec2,
    /// Each square's side.
    pub side: f64,
    /// Squares per line.
    pub cols: u32,
    /// How many pixels the port has.
    pub count: u32,
}

impl PixelGrid {
    /// The grid for `count` pixels in a port row's rectangle.
    pub fn in_row(row: Rect, count: u32) -> PixelGrid {
        let count = count.max(1);
        let area = Rect::new(
            Vec2::new(row.min.x + PIXEL_PAD_X, row.min.y + PIXEL_TOP),
            Vec2::new(row.max.x - PIXEL_PAD_X, row.min.y + CARD_ROW - PIXEL_BOTTOM),
        );
        // Fewer lines win a tie: a strip reads best in one run.
        let (cols, side) = (1..=MAX_PIXEL_LINES)
            .map(|lines| {
                let cols = count.div_ceil(lines);
                let side = (area.width() / cols as f64).min(area.height() / lines as f64);
                (cols, side)
            })
            .fold(
                (count, 0.0),
                |best, c| if c.1 > best.1 + 1e-9 { c } else { best },
            );
        PixelGrid {
            origin: area.min,
            side,
            cols,
            count,
        }
    }

    /// Pixel `i`'s square, or `None` past the end.
    pub fn rect(&self, i: u32) -> Option<Rect> {
        if i >= self.count {
            return None;
        }
        let at = self.origin
            + Vec2::new(
                (i % self.cols) as f64 * self.side,
                (i / self.cols) as f64 * self.side,
            );
        Some(Rect::new(at, at + Vec2::new(self.side, self.side)))
    }
}

impl Project {
    /// One port's pixel grid, in world units.
    pub fn pixel_grid(&self, output: &NodeId, key: &str) -> Option<PixelGrid> {
        let row = self.port_rect(output, key)?;
        let pixels = self.output(output)?.port(key)?.pixels;
        Some(PixelGrid::in_row(row, pixels))
    }

    /// One pixel's square, in world units.
    pub fn pixel_rect(&self, output: &NodeId, key: &str, index: u32) -> Option<Rect> {
        self.pixel_grid(output, key)?.rect(index)
    }
}
