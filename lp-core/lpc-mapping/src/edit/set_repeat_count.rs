//! How many copies a repeat makes.

use alloc::collections::BTreeSet;

use crate::MAX_REPEAT_COUNT;
use crate::edit::copy_levels::copy_levels;
use crate::edit::edit_error::EditError;
use crate::edit::part_place::{part_mut, subtree_ids};
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

/// How many overrides naming copies of the repeat `id` that `count` copies
/// would not make: what counting it down to `count` leaves kept and gone
/// (0 for a part that isn't a repeat). An editor says it, and an agent is
/// told it before it counts down.
#[must_use]
pub fn overrides_past_count(m: &Mapping, id: &PartId, count: u32) -> usize {
    let Some(part) = m.part(id) else {
        return 0;
    };
    let PartKind::Repeat(repeat) = &part.kind else {
        return 0;
    };
    let inside: BTreeSet<PartId> = subtree_ids(&repeat.part).into_iter().collect();
    let levels = copy_levels(&m.parts);
    let around = levels[id].len();
    m.overrides
        .keys()
        .filter(|key| inside.contains(&key.part))
        .filter(|key| {
            // The repeat's place in the copies of a node under it.
            let level = levels[&key.part].len() - around - 1;
            key.copies.get(level).is_some_and(|k| *k >= count)
        })
        .count()
}

/// A repeat makes 1 to [`MAX_REPEAT_COUNT`] copies.
pub(crate) fn check_count(count: u32) -> Result<(), EditError> {
    if (1..=MAX_REPEAT_COUNT).contains(&count) {
        Ok(())
    } else {
        Err(EditError::BadCount(count))
    }
}
