//! The next number for each kind of part: how new ids are minted and why
//! they are never reused.
//!
//! A new part's id is its kind's word and a number (`path1`, `circle2`). The
//! counters are kept with the mapping, so deleting `path3` never lets the
//! next path be `path3` again: whatever pointed at the old one (an override)
//! must never quietly point at the new one (D6, D12).

use alloc::format;

use crate::part::part_id::PartId;
use crate::sorted_set::{WordNumbers, find_word, set_word};

/// The words a part's id is minted from, one per kind.
pub const PART_KIND_WORDS: [&str; 8] = [
    "path", "polygon", "circle", "grid", "filled", "points", "group", "repeat",
];

/// The next number per kind: `{"path": 4, "circle": 2}`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IdCounters {
    /// By kind word, sorted (eight words at most: a sorted `Vec`, which the
    /// device links in a fraction of a map's flash).
    next: WordNumbers,
}

impl IdCounters {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The number the next `word` part would try first (1 when none was
    /// ever minted).
    #[must_use]
    pub fn next(&self, word: &str) -> u32 {
        find_word(&self.next, word)
            .map_or(1, |at| self.next[at].1)
            .max(1)
    }

    /// Every counter, by kind word.
    pub fn iter(&self) -> impl Iterator<Item = (&str, u32)> {
        self.next.iter().map(|(word, n)| (word.as_str(), *n))
    }

    /// Set a kind's next number (a reader restoring saved counters). It
    /// never goes down: a lower number than the one held is ignored.
    pub fn raise(&mut self, word: &str, next: u32) {
        let held = self.next(word);
        set_word(&mut self.next, word, held.max(next));
    }

    /// Mint the next id for a kind of part, skipping any id `taken` says the
    /// mapping already has (so a hand-written `path3` is never minted again).
    /// The counter only ever goes up.
    ///
    /// # Panics
    ///
    /// When `word` is not a kind word ([`PART_KIND_WORDS`]): every caller
    /// mints from a [`crate::part::PartKind`]'s own word.
    pub fn mint(&mut self, word: &str, taken: impl Fn(&PartId) -> bool) -> PartId {
        assert!(
            PART_KIND_WORDS.contains(&word),
            "{word:?} is not a part kind"
        );
        let mut n = self.next(word);
        let id = loop {
            let id = PartId::new(&format!("{word}{n}")).expect("a kind word and a number is an id");
            n = n.saturating_add(1);
            if !taken(&id) {
                break id;
            }
        };
        set_word(&mut self.next, word, n);
        id
    }
}
