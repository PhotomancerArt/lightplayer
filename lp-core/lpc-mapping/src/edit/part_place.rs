//! Where a part sits in the tree: its parent and its place among the
//! parent's parts. Helpers for the edits; nothing here changes a mapping by
//! itself.

use alloc::vec::Vec;

use crate::edit::edit_error::EditError;
use crate::part::{Mapping, Part, PartId, PartKind};

/// A part's place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PartPlace {
    /// The part holding it; `None` at the top.
    pub parent: Option<PartId>,
    /// Its place among its parent's parts (0 when a repeat holds it).
    pub index: usize,
    /// A repeat holds it: it is the one part its parent repeats.
    pub held: bool,
}

/// Where the part `id` sits, when there is one.
pub(crate) fn place_of(m: &Mapping, id: &PartId) -> Option<PartPlace> {
    if let Some(index) = m.parts.iter().position(|p| &p.id == id) {
        return Some(PartPlace {
            parent: None,
            index,
            held: false,
        });
    }
    m.parts.iter().find_map(|p| place_under(p, id))
}

fn place_under(parent: &Part, id: &PartId) -> Option<PartPlace> {
    let held = matches!(parent.kind, PartKind::Repeat(_));
    if let Some(index) = parent.parts().iter().position(|p| &p.id == id) {
        return Some(PartPlace {
            parent: Some(parent.id.clone()),
            index,
            held,
        });
    }
    parent.parts().iter().find_map(|p| place_under(p, id))
}

/// The part `id`, to edit in place.
pub(crate) fn part_mut<'a>(m: &'a mut Mapping, id: &PartId) -> Option<&'a mut Part> {
    find_mut(&mut m.parts, id)
}

fn find_mut<'a>(parts: &'a mut [Part], id: &PartId) -> Option<&'a mut Part> {
    for part in parts {
        if &part.id == id {
            return Some(part);
        }
        if let Some(found) = find_mut(part.kind.parts_mut(), id) {
            return Some(found);
        }
    }
    None
}

/// The list a part can be put into: the top level, or a group's parts.
pub(crate) fn part_list_mut<'a>(
    m: &'a mut Mapping,
    parent: Option<&PartId>,
) -> Result<&'a mut Vec<Part>, EditError> {
    let Some(parent) = parent else {
        return Ok(&mut m.parts);
    };
    let part = part_mut(m, parent).ok_or_else(|| EditError::NoSuchPart(parent.clone()))?;
    match &mut part.kind {
        PartKind::Group { parts } => Ok(parts),
        _ => Err(EditError::NotAGroup(parent.clone())),
    }
}

/// The ids of a part and every part under it, depth first.
pub(crate) fn subtree_ids(part: &Part) -> Vec<PartId> {
    let mut ids = Vec::new();
    part.walk(&mut |p| ids.push(p.id.clone()));
    ids
}
