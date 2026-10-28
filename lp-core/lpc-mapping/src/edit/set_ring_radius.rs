//! How far one ring of a circle is from its centre.

use crate::edit::add_ring::{check_ring, circle_mut};
use crate::edit::edit_error::EditError;
use crate::part::{Mapping, PartId, SegmentKey};

/// Put the ring `key` of the circle `id` at `radius`. Its lamps move with
/// it; its key doesn't change, so its overrides stay on it. The rings are
/// listed outside in, so it goes where its new radius puts it, as a ring
/// added there would ([`crate::edit::add_ring`]): after every other ring at
/// least as large, before the first smaller one.
///
/// Refuses a radius below 0, and a radius of 0 (the centre) for a ring of
/// more than one lamp.
pub fn set_ring_radius(
    m: &mut Mapping,
    id: &PartId,
    key: &SegmentKey,
    radius: f32,
) -> Result<(), EditError> {
    let circle = circle_mut(m, id)?;
    let index = circle
        .rings
        .iter()
        .position(|ring| &ring.key == key)
        .ok_or_else(|| EditError::NoSuchRing {
            part: id.clone(),
            key: key.clone(),
        })?;
    check_ring(radius, circle.rings[index].count).map_err(EditError::BadRing)?;
    let mut ring = circle.rings.remove(index);
    ring.radius = radius;
    let at = circle
        .rings
        .iter()
        .position(|other| other.radius < radius)
        .unwrap_or(circle.rings.len());
    circle.rings.insert(at, ring);
    Ok(())
}
