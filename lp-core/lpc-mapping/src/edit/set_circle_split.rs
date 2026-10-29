//! One segment per ring, or one for the whole circle.

use crate::edit::add_ring::circle_mut;
use crate::edit::edit_error::EditError;
use crate::part::{CircleSplit, Mapping, PartId};

/// Make the circle `id` render one segment per ring (`circle1/r3`), or one
/// for all its rings (at the circle's own id). Overrides stay as they are,
/// as a path's split leaves them ([`crate::edit::set_path_split`]): one on
/// a ring is gone while the circle is one segment, reported, and found
/// again when it splits into rings; one on the circle's own node is on its
/// one segment when whole, and on the part above its rings when split.
pub fn set_circle_split(m: &mut Mapping, id: &PartId, split: CircleSplit) -> Result<(), EditError> {
    circle_mut(m, id)?.split = split;
    Ok(())
}
