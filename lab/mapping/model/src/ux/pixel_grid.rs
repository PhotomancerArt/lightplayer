//! Where a port's pixels sit on its output's card.
//!
//! Each port row on a card shows its strip as the board has it: one square
//! per pixel, in data order, under the port's name. A long strip wraps onto
//! more lines, as many as make the squares biggest, so a 64-pixel port is
//! as easy to read (and to click) as a 30-pixel one.
//!
//! The pixels are grouped into runs, one per object (`port_runs.rs`). When
//! a port has only a few runs, each gets its own line, the way lp2014 listed
//! its channel groups: every run starts at the left, so the edge that lands
//! on it never crosses another. With many runs (a long chain), they pack
//! along the lines with one empty square between them.
//!
//! The grid lives in the model, not the page, because a pixel is something
//! you select (D47): its square is its area, so what's outlined is what a
//! click hits, and a patch edge lands on the square where its span starts
//! (D45). In world units, so it zooms with the card.

use crate::domain::geom::{Rect, Vec2};
use crate::ux::card_geometry::{CARD_ROW, PIXEL_BOTTOM, PIXEL_PAD_X, PIXEL_TOP};
use crate::domain::node::NodeId;
use crate::domain::project::Project;

/// The most lines a port's pixels wrap onto.
pub const MAX_PIXEL_LINES: u32 = 4;

/// One port's pixels as squares on its card.
#[derive(Debug, Clone, PartialEq)]
pub struct PixelGrid {
    /// The top-left of the first square, in world units.
    pub origin: Vec2,
    /// Each square's side.
    pub side: f64,
    /// Squares per line.
    pub cols: u32,
    /// How many pixels the port has.
    pub count: u32,
    /// Where each pixel's square sits: (column, line).
    cells: Vec<(u32, u32)>,
}

/// How far a run's outline stands off its squares, as a share of a square.
pub const RUN_OUTLINE_PAD: f64 = 0.2;

/// Lines sit this many squares apart, so the outlines of runs on
/// neighbouring lines never touch.
pub const LINE_PITCH: f64 = 1.0 + 2.0 * RUN_OUTLINE_PAD + 0.15;

impl PixelGrid {
    /// The grid for `count` pixels, in runs starting at `run_starts` (the
    /// first pixel of every run after the first), in a port row's rectangle.
    pub fn in_row(row: Rect, count: u32, run_starts: Vec<u32>) -> PixelGrid {
        let count = count.max(1);
        let area = Rect::new(
            Vec2::new(row.min.x + PIXEL_PAD_X, row.min.y + PIXEL_TOP),
            Vec2::new(row.max.x - PIXEL_PAD_X, row.min.y + CARD_ROW - PIXEL_BOTTOM),
        );
        let mut starts = vec![0];
        starts.extend(run_starts.into_iter().filter(|s| *s > 0 && *s < count));
        let runs: Vec<u32> = starts
            .iter()
            .zip(starts.iter().skip(1).chain([count].iter()))
            .map(|(a, b)| b - a)
            .collect();

        let size = |cols: u32, cells: &[(u32, u32)]| {
            let lines = cells.iter().map(|c| c.1).max().unwrap_or(0) + 1;
            let tall = (lines - 1) as f64 * LINE_PITCH + 1.0;
            (area.width() / cols as f64).min(area.height() / tall)
        };
        // Packing onto 1..=MAX lines: the biggest squares win, fewer lines
        // on a tie.
        let slots = count + runs.len() as u32 - 1;
        let (mut cols, mut cells) = (1..=MAX_PIXEL_LINES)
            .map(|lines| {
                let cols = slots.div_ceil(lines).max(1);
                (cols, pack(&runs, cols))
            })
            .fold(None::<(u32, Vec<(u32, u32)>)>, |best, c| match best {
                Some(b) if size(c.0, &c.1) <= size(b.0, &b.1) + 1e-9 => Some(b),
                _ => Some(c),
            })
            .expect("at least one line");
        // One run per line reads best (every run starts at the left, so no
        // edge crosses another run): take it when there are few runs and
        // its squares are at least half the size packing gets.
        if runs.len() > 1 && runs.len() as u32 <= MAX_PIXEL_LINES {
            let per_line_cols = runs.iter().copied().max().unwrap_or(1);
            let mut per_line = Vec::new();
            for (line, len) in runs.iter().enumerate() {
                per_line.extend((0..*len).map(|c| (c, line as u32)));
            }
            if size(per_line_cols, &per_line) >= 0.5 * size(cols, &cells) {
                cols = per_line_cols;
                cells = per_line;
            }
        }
        PixelGrid {
            origin: area.min,
            side: size(cols, &cells),
            cols,
            count,
            cells,
        }
    }

    /// Pixel `i`'s square, or `None` past the end.
    pub fn rect(&self, i: u32) -> Option<Rect> {
        let (col, line) = *self.cells.get(i as usize)?;
        let at =
            self.origin + Vec2::new(col as f64 * self.side, line as f64 * self.side * LINE_PITCH);
        Some(Rect::new(at, at + Vec2::new(self.side, self.side)))
    }

    /// The outline around a run of pixels: one box per line it covers,
    /// standing off the squares by [`RUN_OUTLINE_PAD`].
    pub fn run_outline(&self, pixels: std::ops::Range<u32>) -> Vec<Rect> {
        let pad = self.side * RUN_OUTLINE_PAD;
        let mut lines: Vec<Rect> = Vec::new();
        for i in pixels {
            let Some(r) = self.rect(i) else { break };
            match lines.last_mut() {
                Some(last) if (last.min.y - (r.min.y - pad)).abs() < 1e-9 => {
                    last.max.x = last.max.x.max(r.max.x + pad);
                }
                _ => lines.push(Rect::new(
                    r.min - Vec2::new(pad, pad),
                    r.max + Vec2::new(pad, pad),
                )),
            }
        }
        lines
    }
}

impl Project {
    /// One port's pixel grid, in world units.
    pub fn pixel_grid(&self, output: &NodeId, key: &str) -> Option<PixelGrid> {
        let row = self.port_rect(output, key)?;
        self.output(output)?.port(key)?;
        let starts = self
            .port_runs(output, key)
            .iter()
            .skip(1)
            .map(|r| r.pixels.start)
            .collect();
        Some(PixelGrid::in_row(
            row,
            self.port_pixels(output, key),
            starts,
        ))
    }

    /// One pixel's square, in world units.
    pub fn pixel_rect(&self, output: &NodeId, key: &str, index: u32) -> Option<Rect> {
        self.pixel_grid(output, key)?.rect(index)
    }
}

/// Runs packed along lines `cols` squares wide: one empty square before
/// each run that doesn't start a line.
fn pack(runs: &[u32], cols: u32) -> Vec<(u32, u32)> {
    let mut cells = Vec::new();
    let (mut col, mut line) = (0u32, 0u32);
    for (i, len) in runs.iter().enumerate() {
        if i > 0 && col > 0 {
            col += 1;
        }
        for _ in 0..*len {
            if col >= cols {
                col = 0;
                line += 1;
            }
            cells.push((col, line));
            col += 1;
        }
    }
    cells
}
