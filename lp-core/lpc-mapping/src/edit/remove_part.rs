//! Take a part out of the tree.

use alloc::collections::BTreeSet;

use crate::edit::edit_error::EditError;
use crate::edit::part_place::{part_list_mut, place_of, subtree_ids};
use crate::part::{Mapping, Part, PartId};

/// What [`remove_part`] took out, and from where (so it can be put back:
/// [`crate::edit::insert_part`] with the same parent and index).
#[derive(Debug, Clone, PartialEq)]
pub struct RemovedPart {
    pub part: Part,
    /// The group it was in; `None` at the top.
    pub parent: Option<PartId>,
    pub index: usize,
    /// The overrides on it and on every node under it. They are **kept**
    /// (D12): the patcher reports each as gone, and putting the part back
    /// finds them again.
    pub kept_overrides: usize,
}

/// Take the part `id`, and everything under it, out of the tree. Its
/// overrides, and those of every node under it, stay in the mapping; the
/// patcher reports each as a [`crate::patching::Problem::GoneNode`], and no
/// other part can take an id they name ([`crate::edit::rename_part`]). A
/// minted id is never minted again.
///
/// Refuses the part a repeat holds: a repeat repeats exactly one part
/// (unrepeat it, or remove the repeat).
pub fn remove_part(m: &mut Mapping, id: &PartId) -> Result<RemovedPart, EditError> {
    let place = place_of(m, id).ok_or_else(|| EditError::NoSuchPart(id.clone()))?;
    if place.held {
        return Err(EditError::HeldByRepeat(id.clone()));
    }
    let list = part_list_mut(m, place.parent.as_ref())?;
    let part = list.remove(place.index);
    let ids: BTreeSet<PartId> = subtree_ids(&part).into_iter().collect();
    let kept_overrides = m.overrides.keys().filter(|k| ids.contains(&k.part)).count();
    Ok(RemovedPart {
        part,
        parent: place.parent,
        index: place.index,
        kept_overrides,
    })
}
