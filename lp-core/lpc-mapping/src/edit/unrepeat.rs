//! Undo a repeat: keep only the part it repeats.

use alloc::collections::BTreeSet;
use alloc::vec::Vec;

use crate::edit::copy_levels::{copy_levels, rekey_overrides};
use crate::edit::edit_error::EditError;
use crate::edit::part_place::{part_mut, subtree_ids};
use crate::part::{Mapping, Part, PartId, PartKind, TreeId};
use crate::patching::Start;

/// Put the part the repeat `id` holds where the repeat is: copy 0 stays,
/// the other copies go. Returns how many overrides it leaves naming nodes
/// that are gone.
///
/// Copy 0's ids are the part's own (F6), so overrides on it stay put (when
/// repeats are round the repeat, each such id loses the repeat's copy 0:
/// `x@0@3` → `x@3`). Overrides on copies 1 on, and on the repeat's own
/// node, are **kept** (D12): the patcher reports each as gone. Where
/// dropping the repeat's copy number would make one of them name another
/// node (a repeat inside a repeat: `x@2` would become the outer repeat's copy
/// 2), the edit is refused ([`EditError::WouldRetarget`]).
///
/// The part takes the repeat's place in the wiring. For each field its own
/// start keeps, it takes the step the repeat gave copy 0 (the repeat's flow
/// for copy 0 when that sets the field, else the repeat's own start), so
/// copy 0 lands where it did. It doesn't when the part's own start or
/// override adds to (or keeps) a step the repeat took: one step per field
/// can't say both.
pub fn unrepeat(m: &mut Mapping, id: &PartId) -> Result<usize, EditError> {
    let part = m
        .part(id)
        .ok_or_else(|| EditError::NoSuchPart(id.clone()))?;
    let PartKind::Repeat(repeat) = &part.kind else {
        return Err(EditError::NotARepeat(id.clone()));
    };
    let inside: BTreeSet<PartId> = subtree_ids(&repeat.part).into_iter().collect();
    let levels = copy_levels(&m.parts);
    let around = levels[id].len();
    // The repeat's place in the copies of a node under it.
    let level_of = |key: &TreeId| levels[&key.part].len() - around - 1;

    let mut gone = 0;
    let mut stranded = Vec::new();
    for key in m.overrides.keys() {
        if key.part == *id {
            gone += 1;
        } else if inside.contains(&key.part)
            && key.copies.get(level_of(key)).is_some_and(|k| *k >= 1)
        {
            if key.copies.len() < levels[&key.part].len() {
                stranded.push(key.clone());
            } else {
                gone += 1;
            }
        }
    }
    if !stranded.is_empty() {
        return Err(EditError::WouldRetarget {
            part: id.clone(),
            overrides: stranded,
        });
    }

    rekey_overrides(m, |key| {
        if !inside.contains(&key.part) {
            return None;
        }
        let at = level_of(key);
        (key.copies.get(at) == Some(&0)).then(|| {
            let mut copies = key.copies.clone();
            copies.remove(at);
            TreeId {
                copies,
                ..key.clone()
            }
        })
    });

    let slot = part_mut(m, id).expect("found above");
    let placeholder = Part::new(id.clone(), PartKind::Group { parts: Vec::new() });
    let Part {
        kind,
        start,
        flow,
        lamp,
        ..
    } = core::mem::replace(slot, placeholder);
    let PartKind::Repeat(repeat) = kind else {
        unreachable!("checked above");
    };
    let mut held = *repeat.part;
    // What the repeat gave copy 0: its flow's step for copy 0 when that
    // sets the field (a flow renders only Keep and Set), else its own start.
    let copy0 = flow.render(0);
    held.start = Start {
        output: keep_or(held.start.output, || keep_or(copy0.output, || start.output)),
        port: keep_or(held.start.port, || keep_or(copy0.port, || start.port)),
        channel: keep_or(held.start.channel, || {
            keep_or(copy0.channel, || start.channel)
        }),
    };
    held.lamp = held.lamp.over(&lamp);
    *slot = held;
    Ok(gone)
}

/// `own` unless it is Keep (the default), else `taken`.
fn keep_or<T: Default + PartialEq>(own: T, taken: impl FnOnce() -> T) -> T {
    if own == T::default() { taken() } else { own }
}
