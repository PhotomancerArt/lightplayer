//! Put a part into the tree.

use alloc::collections::BTreeSet;

use crate::edit::edit_error::EditError;
use crate::edit::free_id::note_taken;
use crate::edit::part_place::{part_list_mut, subtree_ids};
use crate::part::{Mapping, Part, PartId};

/// Put `part` at place `index` among the parts of `parent` (a group), or of
/// the fixture when `parent` is `None`. **Order is wiring order**: its
/// segments come after those of the parts before it.
///
/// Refuses an id another part already has (its own, or one under it).
/// An id used before is allowed: that is how a removed part comes back
/// (undo) and finds the overrides it left (D12). A new part takes a minted
/// id ([`Mapping::new_part`]); one of the minted form moves the counter
/// past it.
pub fn insert_part(
    m: &mut Mapping,
    parent: Option<&PartId>,
    index: usize,
    part: Part,
) -> Result<(), EditError> {
    let ids = subtree_ids(&part);
    let mut seen = BTreeSet::new();
    for id in &ids {
        if m.part(id).is_some() || !seen.insert(id) {
            return Err(EditError::IdTaken(id.clone()));
        }
    }
    let list = part_list_mut(m, parent)?;
    if index > list.len() {
        return Err(EditError::IndexOutOfRange {
            index,
            len: list.len(),
        });
    }
    list.insert(index, part);
    for id in &ids {
        note_taken(m, id);
    }
    Ok(())
}
