//! Where a shape's own lamp order starts: its "drawn from" (names.html).
//!
//! Layout, not patching: a circle drawn from the inside takes its rings
//! inner first; a path drawn from its last point starts lamp 0 there (SVG's
//! "reverse path direction"). A segment's Direction on the line is another
//! thing, an override ([`crate::edit::set_override_field`]).

use crate::RingOrder;
use crate::edit::edit_error::EditError;
use crate::edit::part_place::part_mut;
use crate::part::{Mapping, PartId, PartKind, Shape};

/// Where a shape is drawn from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrawnFrom {
    /// A circle's outer ring first (its rings as listed).
    Outside,
    /// A circle's inner ring first.
    Inside,
    /// A path's lamp 0 at its first point.
    FirstPoint,
    /// A path's lamp 0 at its last point.
    LastPoint,
}

/// Make the circle or path `id` drawn from `from`. Ring keys and overrides
/// don't change, so an override on a ring stays on that ring wherever it
/// now comes. Refuses a circle's word on anything but a circle, and a
/// path's on anything but a path.
pub fn set_drawn_from(m: &mut Mapping, id: &PartId, from: DrawnFrom) -> Result<(), EditError> {
    let part = part_mut(m, id).ok_or_else(|| EditError::NoSuchPart(id.clone()))?;
    match (&mut part.kind, from) {
        (PartKind::Shape(Shape::Circle(circle)), DrawnFrom::Outside) => {
            circle.drawn_from = RingOrder::OuterFirst;
        }
        (PartKind::Shape(Shape::Circle(circle)), DrawnFrom::Inside) => {
            circle.drawn_from = RingOrder::InnerFirst;
        }
        (PartKind::Shape(Shape::Path(path)), DrawnFrom::FirstPoint) => path.reversed = false,
        (PartKind::Shape(Shape::Path(path)), DrawnFrom::LastPoint) => path.reversed = true,
        (_, DrawnFrom::Outside | DrawnFrom::Inside) => {
            return Err(EditError::NotACircle(id.clone()));
        }
        (_, DrawnFrom::FirstPoint | DrawnFrom::LastPoint) => {
            return Err(EditError::NotAPath(id.clone()));
        }
    }
    Ok(())
}
