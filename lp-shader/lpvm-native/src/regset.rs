//! Bitset of [`crate::vinst::VReg`] for liveness.
//!
//! Vregs below [`MAX_VREGS`] live in a fixed inline array (no heap — the
//! overwhelmingly common case); higher ids spill into a heap-backed overflow
//! tail that grows on demand. The overflow tail exists because nothing in the
//! pipeline caps vreg ids at `MAX_VREGS`: `TempVRegs::mint` and large frontend
//! functions both mint past 256, and a set that silently dropped those ids
//! made the regalloc walk discard loop-carried defs (wrong codegen — see
//! `tests/regalloc_high_vreg_loop.rs`).

use crate::config::MAX_VREGS;
use crate::vinst::VReg;
use alloc::vec::Vec;

/// `MAX_VREGS / 64` u64 words held inline.
pub const VREG_WORDS: usize = MAX_VREGS / 64;

/// Bitset over virtual registers. Ids `0..MAX_VREGS` are stored inline;
/// higher ids allocate overflow words as needed. Membership is exact for
/// every possible [`VReg`] — inserts are never silently dropped.
#[derive(Clone, Debug)]
pub struct RegSet {
    inline: [u64; VREG_WORDS],
    /// Words for vregs `>= MAX_VREGS`; word `i` covers ids
    /// `MAX_VREGS + 64*i .. MAX_VREGS + 64*(i+1)`. May carry trailing zero
    /// words after `remove`, so comparisons must ignore length.
    overflow: Vec<u64>,
}

impl Default for RegSet {
    fn default() -> Self {
        Self::new()
    }
}

/// Semantic equality: two sets are equal iff they contain the same vregs,
/// regardless of how many (zero) overflow words each has materialized.
impl PartialEq for RegSet {
    fn eq(&self, other: &Self) -> bool {
        let words = self.overflow.len().max(other.overflow.len());
        self.inline == other.inline
            && (0..words).all(|i| {
                self.overflow.get(i).copied().unwrap_or(0)
                    == other.overflow.get(i).copied().unwrap_or(0)
            })
    }
}

impl Eq for RegSet {}

impl RegSet {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            inline: [0; VREG_WORDS],
            overflow: Vec::new(),
        }
    }

    /// Global word index and bit mask for `v`.
    fn bit_index(v: VReg) -> (usize, u64) {
        let i = v.0 as usize;
        (i / 64, 1u64 << (i % 64))
    }

    /// The `word`-th 64-bit word of the set (inline then overflow), zero when
    /// beyond what has been materialized.
    fn word(&self, word: usize) -> u64 {
        if word < VREG_WORDS {
            self.inline[word]
        } else {
            self.overflow.get(word - VREG_WORDS).copied().unwrap_or(0)
        }
    }

    /// Total words currently materialized (inline + overflow).
    fn word_count(&self) -> usize {
        VREG_WORDS + self.overflow.len()
    }

    pub fn insert(&mut self, vreg: VReg) {
        let (w, b) = Self::bit_index(vreg);
        if w < VREG_WORDS {
            self.inline[w] |= b;
        } else {
            let idx = w - VREG_WORDS;
            if idx >= self.overflow.len() {
                self.overflow.resize(idx + 1, 0);
            }
            self.overflow[idx] |= b;
        }
    }

    pub fn remove(&mut self, vreg: VReg) {
        let (w, b) = Self::bit_index(vreg);
        if w < VREG_WORDS {
            self.inline[w] &= !b;
        } else if let Some(word) = self.overflow.get_mut(w - VREG_WORDS) {
            *word &= !b;
        }
    }

    pub fn contains(&self, vreg: VReg) -> bool {
        let (w, b) = Self::bit_index(vreg);
        (self.word(w) & b) != 0
    }

    #[must_use]
    pub fn union(self, other: &RegSet) -> RegSet {
        let mut out = self;
        for i in 0..VREG_WORDS {
            out.inline[i] |= other.inline[i];
        }
        if out.overflow.len() < other.overflow.len() {
            out.overflow.resize(other.overflow.len(), 0);
        }
        for (i, word) in other.overflow.iter().enumerate() {
            out.overflow[i] |= word;
        }
        out
    }

    pub fn is_empty(&self) -> bool {
        self.inline.iter().all(|w| *w == 0) && self.overflow.iter().all(|w| *w == 0)
    }

    /// Iterate set bits as [`VReg`] (ascending index).
    pub fn iter(&self) -> impl Iterator<Item = VReg> + '_ {
        (0..self.word_count() * 64)
            .filter(move |i| (self.word(i / 64) & (1u64 << (i % 64))) != 0)
            .map(|i| VReg(i as u16))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_remove_roundtrip() {
        let mut s = RegSet::new();
        let v = VReg(5);
        assert!(!s.contains(v));
        s.insert(v);
        assert!(s.contains(v));
        s.remove(v);
        assert!(!s.contains(v));
    }

    /// The regression this file exists to prevent: ids at and past `MAX_VREGS`
    /// were silently ignored on insert/query, which made the regalloc walk
    /// drop loop-carried vregs >= 256 (discarded defs, wrong codegen).
    #[test]
    fn vregs_at_and_past_max_vregs_are_tracked_exactly() {
        let mut s = RegSet::new();
        for id in [
            (MAX_VREGS - 1) as u16,
            MAX_VREGS as u16,
            (MAX_VREGS + 63) as u16,
            1000,
            u16::MAX,
        ] {
            let v = VReg(id);
            assert!(!s.contains(v), "v{id} must start absent");
            s.insert(v);
            assert!(s.contains(v), "v{id} must be present after insert");
        }
        assert_eq!(
            s.iter().map(|v| v.0).collect::<alloc::vec::Vec<_>>(),
            alloc::vec![
                (MAX_VREGS - 1) as u16,
                MAX_VREGS as u16,
                (MAX_VREGS + 63) as u16,
                1000,
                u16::MAX
            ]
        );
        for id in [MAX_VREGS as u16, 1000] {
            s.remove(VReg(id));
            assert!(!s.contains(VReg(id)), "v{id} must be absent after remove");
        }
        assert!(s.contains(VReg((MAX_VREGS + 63) as u16)));
    }

    /// Equality is over membership, not overflow-vector length: a set that
    /// materialized and then cleared an overflow word equals one that never
    /// touched the overflow.
    #[test]
    fn equality_ignores_materialized_zero_overflow_words() {
        let mut a = RegSet::new();
        a.insert(VReg(300));
        a.remove(VReg(300));
        let b = RegSet::new();
        assert_eq!(a, b);
        assert_eq!(b, a);
    }

    #[test]
    fn union_carries_overflow_from_both_sides() {
        let mut a = RegSet::new();
        a.insert(VReg(3));
        a.insert(VReg(700));
        let mut b = RegSet::new();
        b.insert(VReg(300));
        let u = a.clone().union(&b);
        let u2 = b.clone().union(&a);
        for v in [VReg(3), VReg(300), VReg(700)] {
            assert!(u.contains(v));
            assert!(u2.contains(v));
        }
        assert_eq!(u, u2);
        assert!(!u.is_empty());
    }
}
