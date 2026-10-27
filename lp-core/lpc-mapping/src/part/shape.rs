//! The drawing kinds of part (names.html "Shape"): path, polygon, circle,
//! ring set, grid, filled polygon.
//!
//! Every shape renders segments: a circle one per ring, a path one (or one
//! per subpath, when split), the others one each. The polygon, ring set,
//! grid and filled polygon are today's shapes, unchanged; the path is in
//! SVG's words; the circle is new.

use crate::map2d_doc::{FilledPolygonShape, GridShape, PolygonShape, RingShape};
use crate::part::circle::Circle;
use crate::part::path::Path;

/// A part you draw.
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    Path(Path),
    /// A closed outline with lamps round it (today's polygon).
    Polygon(PolygonShape),
    /// Rings round a centre, one segment each.
    Circle(Circle),
    /// Concentric rings on ONE run, one segment (today's `ring`).
    RingSet(RingShape),
    Grid(GridShape),
    FilledPolygon(FilledPolygonShape),
}

impl Shape {
    /// The word its ids are minted from.
    #[must_use]
    pub fn word(&self) -> &'static str {
        match self {
            Shape::Path(_) => "path",
            Shape::Polygon(_) => "polygon",
            Shape::Circle(_) => "circle",
            Shape::RingSet(_) => "ringset",
            Shape::Grid(_) => "grid",
            Shape::FilledPolygon(_) => "filled",
        }
    }
}
