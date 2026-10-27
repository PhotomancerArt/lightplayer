//! The drawing kinds of part (names.html "Shape"): path, polygon, circle,
//! grid, filled polygon, points.
//!
//! Every shape renders segments: a circle one per ring (or one for all,
//! when split so), a path one (or one per subpath, when split), the others
//! one each. The polygon, grid and filled polygon are today's shapes,
//! unchanged; the path is in SVG's words; the circle holds keyed rings and
//! is today's ring set too; points are new.

use crate::map2d_doc::{FilledPolygonShape, GridShape, PolygonShape};
use crate::part::circle::Circle;
use crate::part::path::Path;
use crate::part::points::Points;

/// A part you draw.
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    Path(Path),
    /// A closed outline with lamps round it (today's polygon).
    Polygon(PolygonShape),
    /// Rings round a centre.
    Circle(Circle),
    Grid(GridShape),
    FilledPolygon(FilledPolygonShape),
    /// Lamps at listed positions.
    Points(Points),
}

impl Shape {
    /// The word its ids are minted from.
    #[must_use]
    pub fn word(&self) -> &'static str {
        match self {
            Shape::Path(_) => "path",
            Shape::Polygon(_) => "polygon",
            Shape::Circle(_) => "circle",
            Shape::Grid(_) => "grid",
            Shape::FilledPolygon(_) => "filled",
            Shape::Points(_) => "points",
        }
    }
}
