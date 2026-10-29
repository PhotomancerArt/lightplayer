//! Make a new part: mint its id and put it into the tree.

use alloc::collections::BTreeSet;

use crate::edit::edit_error::EditError;
use crate::edit::insert_part::insert_part;
use crate::part::{Mapping, Part, PartId, PartKind};

/// Make a new part of `kind` at place `index` among the parts of `parent`
/// (a group), or of the fixture when `parent` is `None`, and return its id.
///
/// The id is minted from its kind's word ([`crate::part::IdCounters`]),
/// never one a part has **nor one a kept override still names** (D12, the
/// "used before" rule of [`crate::edit::rename_part`]):
/// [`Mapping::new_part`] skips only ids a part has, so a part minted that
/// way could take the id of a removed part whose overrides were kept, and
/// quietly pick them up. Every editor makes new parts through here.
pub fn add_part(
    m: &mut Mapping,
    parent: Option<&PartId>,
    index: usize,
    kind: PartKind,
) -> Result<PartId, EditError> {
    let taken: BTreeSet<PartId> = m
        .all_parts()
        .iter()
        .map(|p| p.id.clone())
        .chain(m.overrides.keys().map(|key| key.part.clone()))
        .collect();
    // Minting moves the counter; a refused insert puts it back.
    let next = m.next.clone();
    let id = m
        .next
        .mint(kind.word(), |candidate| taken.contains(candidate));
    match insert_part(m, parent, index, Part::new(id.clone(), kind)) {
        Ok(()) => Ok(id),
        Err(error) => {
            m.next = next;
            Err(error)
        }
    }
}
