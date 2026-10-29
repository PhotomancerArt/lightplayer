//! Remove a ring from a circle.

use crate::edit::add_ring::circle_mut;
use crate::edit::edit_error::EditError;
use crate::part::{Mapping, PartId, Ring, SegmentKey};

/// Remove the ring `key` from the circle `id`, and return it. The other
/// rings keep their keys, and its key is never given to another ring. Its
/// overrides are **kept** (D12): the patcher reports each as gone.
pub fn remove_ring(m: &mut Mapping, id: &PartId, key: &SegmentKey) -> Result<Ring, EditError> {
    let circle = circle_mut(m, id)?;
    let index = circle
        .rings
        .iter()
        .position(|ring| &ring.key == key)
        .ok_or_else(|| EditError::NoSuchRing {
            part: id.clone(),
            key: key.clone(),
        })?;
    Ok(circle.rings.remove(index))
}
