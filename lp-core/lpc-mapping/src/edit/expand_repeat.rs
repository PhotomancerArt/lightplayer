//! Expand a repeat: every copy becomes a real part (D6).

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::format;
use alloc::vec;
use alloc::vec::Vec;

use crate::Rotation2d;
use crate::edit::copy_levels::{copy_levels, rekey_overrides};
use crate::edit::edit_error::EditError;
use crate::edit::free_id::check_free;
use crate::edit::part_place::{part_mut, subtree_ids};
use crate::edit::rotate_part::rotate_part;
use crate::part::{Mapping, Part, PartId, PartKind, Shape, TreeId, render};

/// Turn the repeat `id` into a group of its copies, each a real part, and
/// return the copies' ids, copy 0 first.
///
/// - **Ids.** Copy 0 is the part the repeat held, with its own ids. In copy
///   k every part `x` under it becomes `x-k` (a group's parts too). Refused
///   when any of those ids is taken or was used before.
/// - **Overrides** follow their nodes: `x@k` → `x-k`, keys and the copies of
///   other repeats kept (`circle2/r1@2` → `circle2-2/r1`; `z@2@3`, inner
///   copy 2 of copy 3, → `z-3@2`), copy 0's lose the repeat's place
///   (`x@0@3` → `x@3`). Overrides on copies past the count (kept, D12) are
///   rekeyed the same way and stay gone.
/// - **Patching is unchanged.** The repeat becomes a group with its id, its
///   start and its flow, holding the copies in order, so every copy takes the
///   steps it took; each copy has the held part's own start and flow, as
///   every copy had.
/// - **Lamps stay where the repeat put them**, up to float rounding: each
///   copy's geometry turns by its copy's angle (`rotate_part`); a grid or a
///   filled polygon, which can't carry a turn, becomes points at the very
///   lamps the repeat rendered.
pub fn expand_repeat(m: &mut Mapping, id: &PartId) -> Result<Vec<PartId>, EditError> {
    let part = m
        .part(id)
        .ok_or_else(|| EditError::NoSuchPart(id.clone()))?;
    let PartKind::Repeat(repeat) = &part.kind else {
        return Err(EditError::NotARepeat(id.clone()));
    };
    let held = (*repeat.part).clone();
    let center = repeat.center;
    let degrees = repeat.copy_degrees();
    let count = repeat.count;
    let inside: BTreeSet<PartId> = subtree_ids(&held).into_iter().collect();
    let levels = copy_levels(&m.parts);
    let around = levels[id].len();
    // The repeat's place in the copies of a node under it.
    let level_of = |part: &PartId| levels[part].len() - around - 1;

    // The ids copy k ≥ 1 takes: free, and unique among themselves.
    let mut fresh = BTreeSet::new();
    for k in 1..count {
        for part in &inside {
            let new = copy_id(part, k)?;
            check_free(m, &new)?;
            if !fresh.insert(new.clone()) {
                return Err(EditError::IdTaken(new));
            }
        }
    }

    // Where every override under the repeat goes.
    let mut rekeyed = BTreeMap::new();
    let mut stranded = Vec::new();
    for key in m.overrides.keys().filter(|key| inside.contains(&key.part)) {
        let at = level_of(&key.part);
        let Some(&k) = key.copies.get(at) else {
            continue;
        };
        let mut copies = key.copies.clone();
        copies.remove(at);
        let part = if k == 0 {
            key.part.clone()
        } else {
            copy_id(&key.part, k)?
        };
        // A copy past the count names a part that won't exist: it must
        // stay gone, never land on a part that has that id already.
        if k >= count && m.part(&part).is_some() {
            stranded.push(key.clone());
        }
        let new = TreeId {
            part,
            key: key.key.clone(),
            copies,
        }
        .canonical();
        rekeyed.insert(key.clone(), new);
    }
    if !stranded.is_empty() {
        return Err(EditError::WouldRetarget {
            part: id.clone(),
            overrides: stranded,
        });
    }

    // A grid or filled polygon becomes the lamps the repeat rendered for it.
    let tree = if bakes(&held) {
        Some(render(m).map_err(EditError::Render)?)
    } else {
        None
    };
    let mut copies = Vec::with_capacity(count as usize);
    copies.push(held.clone());
    for k in 1..count {
        let mut copy = held.clone();
        let rendered = |part: &PartId| -> Vec<[f32; 2]> {
            let Some(tree) = &tree else {
                return Vec::new();
            };
            let mut at = vec![0; level_of(part)];
            at.push(k);
            let segment = TreeId {
                part: part.clone(),
                key: None,
                copies: at,
            }
            .canonical();
            tree.segment(&segment)
                .map(|s| tree.lamps_of(s).to_vec())
                .unwrap_or_default()
        };
        rotate_part(
            &mut copy,
            &Rotation2d::about(center, degrees[k as usize]),
            degrees[k as usize],
            &rendered,
        );
        rename_copy(&mut copy, k);
        copies.push(copy);
    }

    let ids = copies.iter().map(|c| c.id.clone()).collect();
    part_mut(m, id).expect("found above").kind = PartKind::Group { parts: copies };
    rekey_overrides(m, |key| rekeyed.get(key).cloned());
    Ok(ids)
}

/// Copy k's id for the part `part`: `part-k`.
fn copy_id(part: &PartId, k: u32) -> Result<PartId, EditError> {
    let id = format!("{part}-{k}");
    PartId::new(&id).map_err(|reason| EditError::BadId { id, reason })
}

/// Every part of copy k takes its copy id, and its name (when it has one)
/// the same suffix.
fn rename_copy(part: &mut Part, k: u32) {
    part.id = copy_id(&part.id, k).expect("checked before anything changed");
    if !part.name.is_empty() {
        part.name = format!("{}-{k}", part.name);
    }
    for child in part.kind.parts_mut() {
        rename_copy(child, k);
    }
}

/// Does anything under `part` become points when it turns?
fn bakes(part: &Part) -> bool {
    let mut found = false;
    part.walk(&mut |p| {
        found |= matches!(
            p.kind,
            PartKind::Shape(Shape::Grid(_) | Shape::FilledPolygon(_))
        );
    });
    found
}
