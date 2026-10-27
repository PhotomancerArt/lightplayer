//! Wrap a part in a new repeat.

use alloc::collections::BTreeSet;
use alloc::vec::Vec;

use crate::edit::copy_levels::{copy_levels, rekey_overrides};
use crate::edit::edit_error::EditError;
use crate::edit::part_place::{part_mut, subtree_ids};
use crate::edit::set_repeat_count::check_count;
use crate::part::{Mapping, Part, PartId, PartKind, Repeat, TreeId};
use crate::patching::{ChannelOp, OutputOp, Override, PortOp, Start};

/// Put a new repeat of `count` copies about `center` where the part `id` is,
/// holding it. The repeat gets a minted id (`repeat<n>`), which it returns.
///
/// **Copy 0 is where the part was.** The part keeps its id (F6), so it is
/// copy 0 and every override on it, or on a node under it, still names copy
/// 0 (when repeats are round the part already, each such id gains the new
/// repeat's copy 0 at its place: `x@3` → `x@0@3`). The repeat takes the
/// part's place in the wiring and the part's own start, so copies 1 on
/// follow copy 0 end to end instead of all starting where the part did. Where
/// the part's own override replaced a step (with Keep or Add), the repeat
/// gets an explicit Keep for that field, so the step isn't taken twice.
pub fn wrap_in_repeat(
    m: &mut Mapping,
    id: &PartId,
    count: u32,
    center: [f32; 2],
) -> Result<PartId, EditError> {
    check_count(count)?;
    let part = m
        .part(id)
        .ok_or_else(|| EditError::NoSuchPart(id.clone()))?;
    let inside: BTreeSet<PartId> = subtree_ids(part).into_iter().collect();
    let levels = copy_levels(&m.parts);
    let around = levels[id].len();

    // Never an id a part has, nor one a kept override still names.
    let taken: BTreeSet<PartId> = m
        .all_parts()
        .iter()
        .map(|p| p.id.clone())
        .chain(m.overrides.keys().map(|key| key.part.clone()))
        .collect();
    let repeat_id = m.next.mint("repeat", |candidate| taken.contains(candidate));

    // The new repeat's node sits in the same copies as the part's own.
    let keeps: Vec<(TreeId, Override)> = m
        .overrides
        .iter()
        .filter(|(key, _)| key.part == *id && key.is_part_node())
        .filter_map(|(key, entry)| {
            let keep = Override {
                output: entry
                    .output
                    .as_ref()
                    .filter(|op| !is_set_output(op))
                    .map(|_| OutputOp::Keep),
                port: entry
                    .port
                    .as_ref()
                    .filter(|op| !is_set_port(op))
                    .map(|_| PortOp::Keep),
                channel: entry
                    .channel
                    .filter(|op| !is_set_channel(*op))
                    .map(|_| ChannelOp::Keep),
                ..Override::default()
            };
            let node = TreeId {
                part: repeat_id.clone(),
                ..key.clone()
            };
            (!keep.is_empty()).then_some((node, keep))
        })
        .collect();

    let slot = part_mut(m, id).expect("found above");
    let placeholder = Part::new(repeat_id.clone(), PartKind::Group { parts: Vec::new() });
    let mut held = core::mem::replace(slot, placeholder);
    let start = core::mem::replace(&mut held.start, Start::KEEP);
    let mut repeat = Part::new(
        repeat_id.clone(),
        PartKind::Repeat(Repeat::new(held, center, count)),
    );
    repeat.start = start;
    *slot = repeat;

    // Every id under the part gains the new repeat's copy 0 just outside
    // the repeats that are inside the part.
    rekey_overrides(m, |key| {
        if !inside.contains(&key.part) {
            return None;
        }
        let at = levels[&key.part].len() - around;
        (key.copies.len() > at).then(|| {
            let mut copies = key.copies.clone();
            copies.insert(at, 0);
            TreeId {
                copies,
                ..key.clone()
            }
        })
    });
    m.overrides.extend(keeps);
    Ok(repeat_id)
}

fn is_set_output(op: &OutputOp) -> bool {
    matches!(op, OutputOp::Set(_))
}

fn is_set_port(op: &PortOp) -> bool {
    matches!(op, PortOp::Set { .. })
}

fn is_set_channel(op: ChannelOp) -> bool {
    matches!(op, ChannelOp::Set(_))
}
