//! Give a part another id.

use crate::edit::copy_levels::rekey_overrides;
use crate::edit::edit_error::EditError;
use crate::edit::free_id::{check_free, note_taken};
use crate::edit::part_place::part_mut;
use crate::part::{Mapping, PartId, TreeId};

/// Give the part `id` the id `new`, and every override on a node of it the
/// new id with the same key and copies (`circle1/r3@2` → `display/r3@2`),
/// so every lamp lands where it did. Starts and flows name ports and
/// outputs, never parts, so nothing else changes.
///
/// Refuses an id another part has, and one used before: minted already, or
/// still named by an override (a removed part's, kept by D12), which the
/// renamed part would otherwise pick up.
pub fn rename_part(m: &mut Mapping, id: &PartId, new: PartId) -> Result<(), EditError> {
    if m.part(id).is_none() {
        return Err(EditError::NoSuchPart(id.clone()));
    }
    if *id == new {
        return Ok(());
    }
    check_free(m, &new)?;
    part_mut(m, id).expect("found above").id = new.clone();
    note_taken(m, &new);
    rekey_overrides(m, |key| {
        (key.part == *id).then(|| TreeId {
            part: new.clone(),
            ..key.clone()
        })
    });
    Ok(())
}
