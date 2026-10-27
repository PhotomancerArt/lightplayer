//! Ids that look minted: a kind's word and a number (`path3`), a ring's `r`
//! and a number (`r5`). A counter must stay above every one of them, or the
//! next mint would reuse an id something may still point at (D6, D12).

use crate::part::PART_KIND_WORDS;

/// The kind word and number of an id that reads as minted (`path3` →
/// `("path", 3)`), whatever kind its part is.
pub(crate) fn minted_part_number(id: &str) -> Option<(&'static str, u32)> {
    PART_KIND_WORDS
        .iter()
        .find_map(|word| Some((*word, number_after(id, word)?)))
}

/// The number of a ring key that reads as minted (`r5` → 5).
pub(crate) fn minted_ring_number(key: &str) -> Option<u32> {
    number_after(key, "r")
}

fn number_after(text: &str, prefix: &str) -> Option<u32> {
    let digits = text.strip_prefix(prefix)?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}
