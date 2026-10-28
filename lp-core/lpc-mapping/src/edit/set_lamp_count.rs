//! How many lamps a path, a polygon or one ring of a circle holds.

use alloc::format;

use crate::edit::add_ring::circle_mut;
use crate::edit::edit_error::EditError;
use crate::edit::part_place::part_mut;
use crate::part::{Mapping, PartId, PartKind, SegmentKey, Shape};

/// Give the path or polygon `id`, or its ring `ring` when `id` is a
/// circle, `count` lamps. Lamps are spread along the shape as drawn, so
/// they move; ids and overrides don't change. A repeat's copies share the
/// count of the part it holds: set it on that part.
///
/// Refuses no lamps, more than one lamp at a circle's centre, and a part
/// with no lamp count of its own (a group, a repeat, a grid…).
pub fn set_lamp_count(
    m: &mut Mapping,
    id: &PartId,
    ring: Option<&SegmentKey>,
    count: u32,
) -> Result<(), EditError> {
    if count == 0 {
        return Err(EditError::NoLamps(id.clone()));
    }
    if let Some(key) = ring {
        let circle = circle_mut(m, id)?;
        let found = circle
            .rings
            .iter_mut()
            .find(|r| &r.key == key)
            .ok_or_else(|| EditError::NoSuchRing {
                part: id.clone(),
                key: key.clone(),
            })?;
        if found.is_centre() && count != 1 {
            return Err(EditError::BadRing(format!(
                "a ring of radius 0 is the centre and holds one lamp, not {count}"
            )));
        }
        found.count = count;
        return Ok(());
    }
    let part = part_mut(m, id).ok_or_else(|| EditError::NoSuchPart(id.clone()))?;
    match &mut part.kind {
        PartKind::Shape(Shape::Path(path)) => path.count = count,
        PartKind::Shape(Shape::Polygon(polygon)) => polygon.count = count,
        _ => return Err(EditError::NoLampCount(id.clone())),
    }
    Ok(())
}
