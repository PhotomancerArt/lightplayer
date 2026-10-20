//! A grid: the product's rectilinear lamp matrix (`map2d` `grid`), a whole
//! object (plan M4). Columns and rows of lamps one pitch apart, wired row
//! by row from a start corner, every row the same way (raster) or each the
//! other way from the last (snake, a panel's usual wiring).
//!
//! The lab takes the product's shape as it is (plan D9): the fields are the
//! product's, and so is where every lamp lands (`resolve::grid_lamps`).

use crate::domain::geom::Vec2;

#[derive(Debug, Clone, PartialEq)]
pub struct Grid {
    /// Where the top-left lamp sits, before the start corner reorders them.
    pub origin: Vec2,
    pub cols: u32,
    pub rows: u32,
    /// Lamp to lamp, both ways.
    pub pitch: f64,
    pub routing: Routing,
    pub start_corner: StartCorner,
}

/// How a lattice's rows are wired: a grid's, and a filled polygon's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Routing {
    /// Each row runs back the other way (a panel's usual wiring).
    #[default]
    Snake,
    /// Every row runs the same way.
    Raster,
}

/// Which corner lamp 0 is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StartCorner {
    #[default]
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl StartCorner {
    /// Does the walk start from the bottom row?
    pub fn from_bottom(self) -> bool {
        matches!(self, StartCorner::BottomLeft | StartCorner::BottomRight)
    }

    /// Does the walk start from the right column?
    pub fn from_right(self) -> bool {
        matches!(self, StartCorner::TopRight | StartCorner::BottomRight)
    }
}

impl Grid {
    /// The box the grid's lamps fill: every lamp's cell, half a pitch round
    /// it, so a grid is outlined like the panel it is.
    pub fn outline(&self) -> Vec<Vec2> {
        let half = self.pitch / 2.0;
        let min = Vec2::new(self.origin.x - half, self.origin.y - half);
        let max = Vec2::new(
            self.origin.x + (self.cols.max(1) - 1) as f64 * self.pitch + half,
            self.origin.y + (self.rows.max(1) - 1) as f64 * self.pitch + half,
        );
        vec![min, Vec2::new(max.x, min.y), max, Vec2::new(min.x, max.y)]
    }
}
