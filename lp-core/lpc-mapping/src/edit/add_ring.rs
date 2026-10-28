//! Add a ring to a circle.

use alloc::format;
use alloc::string::String;

use crate::edit::edit_error::EditError;
use crate::edit::part_place::part_mut;
use crate::part::{Circle, Mapping, PartId, PartKind, SegmentKey, Shape};

/// Add a ring of `count` lamps at `radius` to the circle `id`, and return
/// its key. The rings are listed outside in, so it goes where its radius
/// puts it: after every ring at least as large, before the first smaller
/// one (a ring smaller than all of them goes last, a larger one first).
/// The key is the circle's next (`r<n>`), never one a ring of it had
/// before, even one since removed ([`Circle::add_ring`]): an override left
/// on a removed ring never lands on the new one. A ring of radius 0 is the
/// centre and holds one lamp.
pub fn add_ring(
    m: &mut Mapping,
    id: &PartId,
    radius: f32,
    count: u32,
) -> Result<SegmentKey, EditError> {
    check_ring(radius, count).map_err(EditError::BadRing)?;
    let circle = circle_mut(m, id)?;
    let at = circle
        .rings
        .iter()
        .position(|ring| ring.radius < radius)
        .unwrap_or(circle.rings.len());
    let key = circle.add_ring(radius, count);
    let ring = circle.rings.pop().expect("just added");
    circle.rings.insert(at, ring);
    Ok(key)
}

/// The circle `id`, to edit in place.
pub(crate) fn circle_mut<'a>(m: &'a mut Mapping, id: &PartId) -> Result<&'a mut Circle, EditError> {
    let part = part_mut(m, id).ok_or_else(|| EditError::NoSuchPart(id.clone()))?;
    match &mut part.kind {
        PartKind::Shape(Shape::Circle(circle)) => Ok(circle),
        _ => Err(EditError::NotACircle(id.clone())),
    }
}

/// A ring can be drawn: a radius of 0 or more, at least one lamp, and one
/// lamp exactly at the centre.
fn check_ring(radius: f32, count: u32) -> Result<(), String> {
    if !radius.is_finite() || radius < 0.0 {
        return Err(format!("a ring's radius is 0 or more, not {radius}"));
    }
    if count == 0 {
        return Err(String::from("a ring holds at least one lamp"));
    }
    if radius == 0.0 && count != 1 {
        return Err(format!(
            "a ring of radius 0 is the centre and holds one lamp, not {count}"
        ));
    }
    Ok(())
}
