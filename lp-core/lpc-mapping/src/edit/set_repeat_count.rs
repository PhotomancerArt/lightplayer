//! How many copies a repeat makes.

use crate::MAX_REPEAT_COUNT;
use crate::edit::edit_error::EditError;
use crate::edit::part_place::part_mut;
use crate::part::{Mapping, PartId, PartKind};

/// Make the repeat `id` make `count` copies (1 to [`MAX_REPEAT_COUNT`]).
///
/// Overrides are never moved. Counting down, the overrides of the copies it
/// drops are **kept** (D12): the patcher reports each as gone, and counting
/// back up finds them again on the same copies.
pub fn set_repeat_count(m: &mut Mapping, id: &PartId, count: u32) -> Result<(), EditError> {
    check_count(count)?;
    let part = part_mut(m, id).ok_or_else(|| EditError::NoSuchPart(id.clone()))?;
    match &mut part.kind {
        PartKind::Repeat(repeat) => {
            repeat.count = count;
            Ok(())
        }
        _ => Err(EditError::NotARepeat(id.clone())),
    }
}

/// A repeat makes 1 to [`MAX_REPEAT_COUNT`] copies.
pub(crate) fn check_count(count: u32) -> Result<(), EditError> {
    if (1..=MAX_REPEAT_COUNT).contains(&count) {
        Ok(())
    } else {
        Err(EditError::BadCount(count))
    }
}
