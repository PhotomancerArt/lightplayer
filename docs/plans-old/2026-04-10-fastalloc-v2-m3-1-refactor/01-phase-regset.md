# Phase 1: MAX_VREGS Constant and RegSet Type

## Scope

Add the compile-time constant for maximum virtual registers and implement the fixed-size bitset type that replaces BTreeSet for liveness.

## Implementation

### 1. Update `config.rs`

Add the constant that controls RegSet size:

```rust
//! Compile-time configuration for lpvm-native.

/// When `true`, use linear-scan register allocation.
pub const USE_LINEAR_SCAN_REGALLOC: bool = true;

/// Maximum number of virtual registers supported by the allocator.
/// Determines RegSet size (MAX_VREGS / 64 words = 4).
/// If exceeded, lowering will panic with a clear error message.
pub const MAX_VREGS: usize = 256;

/// Number of u64 words needed for RegSet.
pub const VREG_WORDS: usize = MAX_VREGS / 64;
```

### 2. Create `regset.rs`

```rust
//! Fixed-size bitset for virtual register sets.
//!
//! Replaces BTreeSet<VReg> for liveness analysis.
//! 32 bytes fixed, zero heap allocation.

use core::fmt;
use crate::config::{MAX_VREGS, VREG_WORDS};
use crate::vinst::VReg;

/// Fixed-size bitset for up to MAX_VREGS virtual registers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RegSet(pub [u64; VREG_WORDS]);

impl RegSet {
    /// Empty set.
    pub const EMPTY: Self = Self([0; VREG_WORDS]);

    /// Create new empty set.
    pub fn new() -> Self {
        Self::EMPTY
    }

    /// Insert a vreg into the set.
    pub fn insert(&mut self, vreg: VReg) {
        let idx = vreg.0 as usize;
        assert!(idx < MAX_VREGS, "vreg {} exceeds MAX_VREGS {}", idx, MAX_VREGS);
        let word = idx / 64;
        let bit = idx % 64;
        self.0[word] |= 1u64 << bit;
    }

    /// Remove a vreg from the set.
    pub fn remove(&mut self, vreg: VReg) {
        let idx = vreg.0 as usize;
        if idx >= MAX_VREGS {
            return;
        }
        let word = idx / 64;
        let bit = idx % 64;
        self.0[word] &= !(1u64 << bit);
    }

    /// Check if set contains vreg.
    pub fn contains(&self, vreg: VReg) -> bool {
        let idx = vreg.0 as usize;
        if idx >= MAX_VREGS {
            return false;
        }
        let word = idx / 64;
        let bit = idx % 64;
        (self.0[word] >> bit) & 1 != 0
    }

    /// Union of two sets (returns new set).
    pub fn union(&self, other: &RegSet) -> RegSet {
        let mut result = RegSet::new();
        for i in 0..VREG_WORDS {
            result.0[i] = self.0[i] | other.0[i];
        }
        result
    }

    /// Intersection of two sets.
    pub fn intersect(&self, other: &RegSet) -> RegSet {
        let mut result = RegSet::new();
        for i in 0..VREG_WORDS {
            result.0[i] = self.0[i] & other.0[i];
        }
        result
    }

    /// Difference: self - other.
    pub fn difference(&self, other: &RegSet) -> RegSet {
        let mut result = RegSet::new();
        for i in 0..VREG_WORDS {
            result.0[i] = self.0[i] & !other.0[i];
        }
        result
    }

    /// Check if set is empty.
    pub fn is_empty(&self) -> bool {
        self.0.iter().all(|&w| w == 0)
    }

    /// Count of set bits.
    pub fn len(&self) -> usize {
        self.0.iter().map(|&w| w.count_ones() as usize).sum()
    }

    /// Iterate over all vregs in the set.
    pub fn iter(&self) -> impl Iterator<Item = VReg> + '_ {
        let mut vregs = alloc::vec::Vec::new();
        for word_idx in 0..VREG_WORDS {
            let word = self.0[word_idx];
            if word != 0 {
                for bit in 0..64 {
                    if (word >> bit) & 1 != 0 {
                        let vreg_idx = word_idx * 64 + bit;
                        if vreg_idx < MAX_VREGS {
                            vregs.push(VReg(vreg_idx as u16));
                        }
                    }
                }
            }
        }
        vregs.into_iter()
    }

    /// Clear all bits.
    pub fn clear(&mut self) {
        self.0 = [0; VREG_WORDS];
    }
}

impl Default for RegSet {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for RegSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let vregs: Vec<_> = self.iter().map(|v| format!("v{}", v.0)).collect();
        write!(f, "[{}]", vregs.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_regset_basic() {
        let mut set = RegSet::new();
        assert!(set.is_empty());

        set.insert(VReg(5));
        assert!(set.contains(VReg(5)));
        assert!(!set.contains(VReg(4)));

        set.remove(VReg(5));
        assert!(!set.contains(VReg(5)));
        assert!(set.is_empty());
    }

    #[test]
    fn test_regset_union() {
        let mut a = RegSet::new();
        let mut b = RegSet::new();
        a.insert(VReg(1));
        a.insert(VReg(2));
        b.insert(VReg(2));
        b.insert(VReg(3));

        let u = a.union(&b);
        assert!(u.contains(VReg(1)));
        assert!(u.contains(VReg(2)));
        assert!(u.contains(VReg(3)));
        assert_eq!(u.len(), 3);
    }

    #[test]
    fn test_regset_iter() {
        let mut set = RegSet::new();
        set.insert(VReg(10));
        set.insert(VReg(20));
        set.insert(VReg(30));

        let vregs: Vec<_> = set.iter().collect();
        assert_eq!(vregs.len(), 3);
        assert!(vregs.contains(&VReg(10)));
        assert!(vregs.contains(&VReg(20)));
        assert!(vregs.contains(&VReg(30)));
    }

    #[test]
    fn test_regset_size() {
        use core::mem::size_of;
        assert_eq!(size_of::<RegSet>(), 32); // 4 * 8 bytes
    }
}
```

### 3. Update `lib.rs`

Export the new type:

```rust
pub mod regset;
pub use regset::RegSet;
```

## Validate

```bash
cargo test -p lpvm-native --lib -- regset
```

Tests should verify:
- Basic insert/remove/contains operations
- Union, intersection, difference
- Iterator produces all vregs
- Size is exactly 32 bytes
