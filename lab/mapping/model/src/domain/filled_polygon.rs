//! A filled polygon: the product's shaped matrix (`map2d`
//! `filled_polygon`), a whole object (plan M4). An outline filled with a
//! lattice of lamps one pitch apart, wired row by row like a grid; a cell
//! is lit only when its centre is inside the outline.
//!
//! Its lamp count is derived, never authored: it is however many cells
//! the outline holds (`resolve::filled_polygon_lamps`, the product's walk).
//! The outline is edited whole for now: no point handles (plan M4).

use crate::domain::geom::Vec2;
use crate::domain::grid::{Routing, StartCorner};

#[derive(Debug, Clone, PartialEq)]
pub struct FilledPolygon {
    /// The outline, three or more points, closed.
    pub points: Vec<Vec2>,
    /// Lamp to lamp, both ways.
    pub pitch: f64,
    /// The lattice's turn in degrees about the outline's box centre. The
    /// outline stays put; only the lattice inside it turns.
    pub angle_deg: f64,
    /// Slides the lattice inside the outline, in the lattice's own frame.
    pub offset: Vec2,
    pub routing: Routing,
    pub start_corner: StartCorner,
}
