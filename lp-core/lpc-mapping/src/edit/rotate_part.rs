//! Turn a part's own geometry about a point: what expanding a repeat does
//! to each copy, so a copy that becomes a real part keeps its lamps where
//! the repeat put them.
//!
//! Every shape that can carry the turn in its own numbers does (the old
//! Map tab's rule, its `expand_repeat`): a path's, a polygon's and
//! points' positions turn; a circle's centre turns and its start angle moves
//! by the turn; a repeat inside turns its centre and its part (a turn
//! conjugated by another is the same turn about the moved centre). A grid
//! has no angle, and a filled polygon's lattice is anchored to its outline's
//! bounding box, so neither can carry a turn: they become points at the
//! lamps the repeat rendered for them, exactly.

use alloc::vec::Vec;

use crate::Rotation2d;
use crate::part::{Part, PartId, PartKind, PathCommand, Points, Shape};

/// Turn `part` and everything under it by `rotation` (`degrees`, the same
/// turn). `rendered` gives the lamps the repeat rendered for a grid or
/// filled polygon in this copy, by its authored id.
pub(crate) fn rotate_part(
    part: &mut Part,
    rotation: &Rotation2d,
    degrees: f32,
    rendered: &dyn Fn(&PartId) -> Vec<[f32; 2]>,
) {
    match &mut part.kind {
        PartKind::Shape(Shape::Path(path)) => {
            for command in &mut path.commands {
                match command {
                    PathCommand::Move(point) | PathCommand::Line(point) => {
                        *point = rotation.apply(*point);
                    }
                }
            }
        }
        PartKind::Shape(Shape::Polygon(polygon)) => {
            for point in &mut polygon.points {
                *point = rotation.apply(*point);
            }
        }
        PartKind::Shape(Shape::Points(points)) => {
            for point in &mut points.at {
                *point = rotation.apply(*point);
            }
        }
        PartKind::Shape(Shape::Circle(circle)) => {
            circle.center = rotation.apply(circle.center);
            circle.start_angle_deg += degrees;
        }
        PartKind::Shape(Shape::Grid(_) | Shape::FilledPolygon(_)) => {
            part.kind = PartKind::Shape(Shape::Points(Points {
                at: rendered(&part.id),
            }));
        }
        PartKind::Group { parts } => {
            for child in parts {
                rotate_part(child, rotation, degrees, rendered);
            }
        }
        PartKind::Repeat(repeat) => {
            repeat.center = rotation.apply(repeat.center);
            rotate_part(&mut repeat.part, rotation, degrees, rendered);
        }
    }
}
