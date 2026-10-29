//! Move a part to another place in the tree.

use alloc::collections::BTreeSet;
use alloc::vec::Vec;

use crate::edit::copy_levels::copy_levels;
use crate::edit::edit_error::EditError;
use crate::edit::part_place::{part_list_mut, place_of, subtree_ids};
use crate::part::{Mapping, PartId, PartKind, TreeId};

/// Move the part `id` (and everything under it) to place `index` among the
/// parts of `parent` (a group), or of the fixture when `parent` is `None`.
/// `index` counts the destination's parts as they are once the part has
/// been taken out. **Order is wiring order**: moving a part moves where its
/// segments come, and the cursor carries on from whatever now comes before.
///
/// Ids are flat, so a move renames nothing and its overrides go with it
/// (grouping is never part of an id). One thing a move can't carry: when it
/// changes the repeats round the part, an override on one of their copies
/// (`x@2`) would name a copy of a different repeat, so the move is refused
/// ([`EditError::WouldRetarget`]). Overrides on the part's own copy go with
/// it.
///
/// Refuses the part a repeat holds, and a move into the part itself or a
/// part under it.
pub fn move_part(
    m: &mut Mapping,
    id: &PartId,
    parent: Option<&PartId>,
    index: usize,
) -> Result<(), EditError> {
    let place = place_of(m, id).ok_or_else(|| EditError::NoSuchPart(id.clone()))?;
    if place.held {
        return Err(EditError::HeldByRepeat(id.clone()));
    }
    let moved: BTreeSet<PartId> = subtree_ids(m.part(id).expect("placed above"))
        .into_iter()
        .collect();
    if let Some(parent) = parent {
        if moved.contains(parent) {
            return Err(EditError::IntoItself(id.clone()));
        }
        match m.part(parent) {
            None => return Err(EditError::NoSuchPart(parent.clone())),
            Some(p) if !matches!(p.kind, PartKind::Group { .. }) => {
                return Err(EditError::NotAGroup(parent.clone()));
            }
            Some(_) => {}
        }
    }
    let same_list = place.parent.as_ref() == parent;
    let len = part_list_mut(m, parent)?.len() - usize::from(same_list);
    if index > len {
        return Err(EditError::IndexOutOfRange { index, len });
    }

    // The repeats round the part, before and after: a group is no repeat,
    // so after the move they are the ones round its new parent.
    let levels = copy_levels(&m.parts);
    let before = &levels[id];
    let after = parent.map(|p| levels[p].clone()).unwrap_or_default();
    if *before != after {
        let stranded: Vec<TreeId> = m
            .overrides
            .keys()
            .filter(|key| {
                moved.contains(&key.part) && {
                    let inside = levels[&key.part].len() - before.len();
                    key.copies.len() > inside
                }
            })
            .cloned()
            .collect();
        if !stranded.is_empty() {
            return Err(EditError::WouldRetarget {
                part: id.clone(),
                overrides: stranded,
            });
        }
    }

    let part = part_list_mut(m, place.parent.as_ref())?.remove(place.index);
    part_list_mut(m, parent)?.insert(index, part);
    Ok(())
}
