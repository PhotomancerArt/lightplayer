//! Which way round a circle's rings run: clockwise or counter-clockwise.

use crate::RingDir;
use crate::edit::add_ring::circle_mut;
use crate::edit::edit_error::EditError;
use crate::part::{Mapping, PartId};

/// Make every ring of the circle `id` run `dir` from its lamp 0. Layout,
/// like "drawn from" ([`crate::edit::set_drawn_from`]): the lamps take new
/// places round each ring; ring keys and overrides don't change. One ring
/// running the other way on the line is its Direction, an override
/// ([`crate::edit::set_override_field`]).
pub fn set_circle_dir(m: &mut Mapping, id: &PartId, dir: RingDir) -> Result<(), EditError> {
    circle_mut(m, id)?.dir = dir;
    Ok(())
}
