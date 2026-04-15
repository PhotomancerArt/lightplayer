//! Fixed-size bitset of [`crate::vinst::VReg`] for liveness (no heap).

use crate::config::MAX_VREGS;
use crate::vinst::VReg;

/// `MAX_VREGS / 64` u64 words.
pub const VREG_WORDS: usize = MAX_VREGS / 64;

/// Bitset over virtual registers `0..MAX_VREGS`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RegSet(pub [u64; VREG_WORDS]);

impl Default for RegSet {
    fn default() -> Self {
        Self::new()
    }
}

impl RegSet {
    #[must_use]
    pub const fn new() -> Self {
        Self([0; VREG_WORDS])
    }

    fn bit_index(v: VReg) -> Option<(usize, u64)> {
        let i = v.0 as usize;
        if i >= MAX_VREGS {
            return None;
        }
        let word = i / 64;
        let bit = 1u64 << (i % 64);
        Some((word, bit))
    }

    pub fn insert(&mut self, vreg: VReg) {
        if let Some((w, b)) = Self::bit_index(vreg) {
            self.0[w] |= b;
        }
    }

    pub fn remove(&mut self, vreg: VReg) {
        if let Some((w, b)) = Self::bit_index(vreg) {
            self.0[w] &= !b;
        }
    }

    pub fn contains(&self, vreg: VReg) -> bool {
        Self::bit_index(vreg).is_some_and(|(w, b)| (self.0[w] & b) != 0)
    }

    #[must_use]
    pub fn union(self, other: &RegSet) -> RegSet {
        let mut out = self;
        for i in 0..VREG_WORDS {
            out.0[i] |= other.0[i];
        }
        out
    }

    pub fn is_empty(&self) -> bool {
        self.0.iter().all(|w| *w == 0)
    }

    /// Iterate set bits as [`VReg`] (ascending index).
    pub fn iter(self) -> impl Iterator<Item = VReg> {
        (0u16..(MAX_VREGS as u16))
            .filter(move |i| self.contains(VReg(*i)))
            .map(VReg)
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
}
