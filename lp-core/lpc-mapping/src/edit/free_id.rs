//! Whether a part may take an id: never one another part has, and never one
//! used before (D6), so nothing that pointed at an old part quietly points
//! at a new one.
//!
//! "Used before" is what the mapping can still see: an id of the minted
//! form (`path3`) whose number its counter has passed, or an id an override
//! still names (the overrides of a removed part are kept, D12). A slug a
//! removed part had and nothing names any more is free again: nothing can
//! point at it.

use crate::edit::edit_error::EditError;
use crate::part::{Mapping, PART_KIND_WORDS, PartId};

/// May a part take `id`?
pub(crate) fn check_free(m: &Mapping, id: &PartId) -> Result<(), EditError> {
    if m.part(id).is_some() {
        return Err(EditError::IdTaken(id.clone()));
    }
    let minted_before = minted_form(id).is_some_and(|(word, n)| n >= 1 && n < m.next.next(word));
    if minted_before || m.overrides.keys().any(|key| &key.part == id) {
        return Err(EditError::IdUsedBefore(id.clone()));
    }
    Ok(())
}

/// A part took `id`: when it has the minted form, move the counter past
/// it, so it is never minted for another part.
pub(crate) fn note_taken(m: &mut Mapping, id: &PartId) {
    if let Some((word, n)) = minted_form(id) {
        m.next.raise(word, n.saturating_add(1));
    }
}

/// `path3` → (`path`, 3): a kind's word and a number, the form
/// [`crate::part::IdCounters::mint`] makes.
fn minted_form(id: &PartId) -> Option<(&'static str, u32)> {
    let text = id.as_str();
    let stem = text.trim_end_matches(|c: char| c.is_ascii_digit());
    if stem.len() == text.len() {
        return None;
    }
    let word = PART_KIND_WORDS.iter().find(|w| **w == stem)?;
    let n = text[stem.len()..].parse().ok()?;
    Some((word, n))
}
