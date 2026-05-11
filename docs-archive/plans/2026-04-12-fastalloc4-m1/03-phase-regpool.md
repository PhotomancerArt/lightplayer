# Phase 3: Extract RegPool

## Scope

Create `fa_alloc/pool.rs` with the LRU register pool utility from the old
`walk.rs`. This is a clean utility module that will be used by the real
allocator in M2.

## Code Organization

`fa_alloc/pool.rs` — new file, placed after mod-level items but before tests.

## Implementation

Create `fa_alloc/pool.rs`:

```rust
//! Physical register pool with LRU eviction.

use crate::rv32::gpr::{self, ALLOC_POOL, PReg};
use crate::vinst::VReg;
use alloc::vec::Vec;

/// Physical register pool with LRU eviction.
pub struct RegPool {
    /// Which vreg occupies each PReg (None = free).
    preg_vreg: [Option<VReg>; 32],
    /// LRU order: index 0 = least recently used. Only allocatable regs.
    lru: Vec<PReg>,
}

impl RegPool {
    pub fn new() -> Self {
        let lru: Vec<PReg> = ALLOC_POOL.iter().copied().collect();
        Self {
            preg_vreg: [None; 32],
            lru,
        }
    }

    /// Find the PReg currently holding this vreg, if any.
    pub fn home(&self, vreg: VReg) -> Option<PReg> {
        for (i, v) in self.preg_vreg.iter().enumerate() {
            if *v == Some(vreg) {
                return Some(i as PReg);
            }
        }
        None
    }

    /// Allocate a free register for vreg. Returns the PReg and any evicted vreg.
    /// If no free reg, evicts the LRU and returns (evicted_vreg, preg).
    pub fn alloc(&mut self, vreg: VReg) -> (PReg, Option<VReg>) {
        // Try to find a free allocatable reg (prefer LRU order)
        for (i, &preg) in self.lru.iter().enumerate() {
            if self.preg_vreg[preg as usize].is_none() {
                self.preg_vreg[preg as usize] = Some(vreg);
                // Move to end (most recently used)
                self.lru.remove(i);
                self.lru.push(preg);
                return (preg, None);
            }
        }
        // Evict LRU (index 0)
        let victim_preg = self.lru.remove(0);
        let victim_vreg = self.preg_vreg[victim_preg as usize];
        self.preg_vreg[victim_preg as usize] = Some(vreg);
        self.lru.push(victim_preg);
        (victim_preg, victim_vreg)
    }

    /// Allocate a specific PReg for vreg. Evicts current occupant if any.
    /// Returns the evicted vreg (if any).
    pub fn alloc_fixed(&mut self, preg: PReg, vreg: VReg) -> Option<VReg> {
        let evicted = self.preg_vreg[preg as usize];
        self.preg_vreg[preg as usize] = Some(vreg);
        self.touch(preg);
        evicted
    }

    /// Free a PReg (vreg is no longer in a register).
    pub fn free(&mut self, preg: PReg) {
        self.preg_vreg[preg as usize] = None;
    }

    /// Mark PReg as most recently used.
    pub fn touch(&mut self, preg: PReg) {
        if let Some(pos) = self.lru.iter().position(|&p| p == preg) {
            self.lru.remove(pos);
            self.lru.push(preg);
        }
    }

    /// Count occupied allocatable registers.
    pub fn occupied_count(&self) -> usize {
        ALLOC_POOL
            .iter()
            .filter(|&&p| self.preg_vreg[p as usize].is_some())
            .count()
    }

    /// Iterate over occupied (preg, vreg) pairs for allocatable registers.
    pub fn iter_occupied(&self) -> impl Iterator<Item = (PReg, VReg)> + '_ {
        ALLOC_POOL
            .iter()
            .copied()
            .filter_map(|p| self.preg_vreg[p as usize].map(|v| (p, v)))
    }

    /// Get a snapshot of current occupied (preg, vreg) pairs.
    pub fn snapshot_occupied(&self) -> Vec<(PReg, VReg)> {
        self.iter_occupied().collect()
    }

    /// Clear allocatable registers only (preserves precolored mappings).
    pub fn clear(&mut self) {
        for p in ALLOC_POOL.iter() {
            self.preg_vreg[*p as usize] = None;
        }
        self.lru.clear();
        self.lru.extend(ALLOC_POOL.iter().copied());
    }

    /// Clear ALL registers including precolored ones outside ALLOC_POOL.
    pub fn clear_all(&mut self) {
        self.preg_vreg = [None; 32];
        self.lru.clear();
        self.lru.extend(ALLOC_POOL.iter().copied());
    }

    /// Iterate ALL occupied registers, including precolored ones
    /// outside ALLOC_POOL (e.g. a0 for vmctx).
    pub fn iter_all_occupied(&self) -> impl Iterator<Item = (PReg, VReg)> + '_ {
        self.preg_vreg
            .iter()
            .enumerate()
            .filter_map(|(i, v)| v.map(|vreg| (i as PReg, vreg)))
    }

    /// Seed the pool with vreg assignments from saved state.
    /// Clears existing state first, then populates with saved assignments.
    pub fn seed(&mut self, assignments: &[(PReg, VReg)]) {
        self.clear();
        for &(preg, vreg) in assignments {
            self.preg_vreg[preg as usize] = Some(vreg);
            self.touch(preg);
        }
    }
}

impl Default for RegPool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vinst::VReg;

    #[test]
    fn pool_alloc_and_free() {
        let mut pool = RegPool::new();
        let (preg1, evicted) = pool.alloc(VReg(0));
        assert!(evicted.is_none());
        assert_eq!(pool.home(VReg(0)), Some(preg1));

        pool.free(preg1);
        assert!(pool.home(VReg(0)).is_none());
    }

    #[test]
    fn pool_lru_eviction() {
        let mut pool = RegPool::new();

        // Fill all allocatable registers
        for i in 0..ALLOC_POOL.len() {
            let (preg, evicted) = pool.alloc(VReg(i as u16));
            assert!(evicted.is_none(), "should not evict on {}th alloc", i);
        }

        // Next alloc should evict LRU (first one allocated)
        let (preg, evicted) = pool.alloc(VReg(100));
        assert!(evicted.is_some());
        assert_eq!(evicted, Some(VReg(0)));

        // Evicted vreg no longer has a home
        assert!(pool.home(VReg(0)).is_none());
        // New vreg is in the evicted preg
        assert_eq!(pool.home(VReg(100)), Some(preg));
    }

    #[test]
    fn pool_alloc_fixed() {
        let mut pool = RegPool::new();

        // Allocate specific register
        let target = ALLOC_POOL[0];
        let evicted = pool.alloc_fixed(target, VReg(0));
        assert!(evicted.is_none());
        assert_eq!(pool.home(VReg(0)), Some(target));

        // Allocate same register to different vreg
        let evicted = pool.alloc_fixed(target, VReg(1));
        assert_eq!(evicted, Some(VReg(0)));
        assert_eq!(pool.home(VReg(1)), Some(target));
        assert!(pool.home(VReg(0)).is_none());
    }

    #[test]
    fn pool_touch_mru() {
        let mut pool = RegPool::new();

        // Allocate two registers
        let (preg1, _) = pool.alloc(VReg(0));
        let (preg2, _) = pool.alloc(VReg(1));

        // Touch first one, making it MRU
        pool.touch(preg1);

        // Allocate until eviction
        for i in 2..ALLOC_POOL.len() {
            pool.alloc(VReg(i as u16));
        }

        // Next eviction should be preg2 (LRU), not preg1 (MRU)
        let (_, evicted) = pool.alloc(VReg(100));
        assert_eq!(evicted, Some(VReg(1))); // preg2's vreg
    }
}
```

## Add to fa_alloc/mod.rs

Add to `fa_alloc/mod.rs`:

```rust
pub mod pool;

pub use pool::RegPool;
```

## Code Organization Reminders

- Place the `pub mod pool;` declaration with the other `pub mod` declarations.
- The tests are at the bottom of the file in `#[cfg(test)] mod tests`.

## Implementation

1. Create `fa_alloc/pool.rs` with the RegPool code above
2. Add `pub mod pool;` and `pub use pool::RegPool;` to `fa_alloc/mod.rs`

## Validation

```bash
cargo check -p lpvm-native 2>&1 | head -30
```

Expected: RegPool should compile. Other errors about missing modules remain.

```bash
cargo test -p lpvm-native pool::
```

Expected: RegPool unit tests pass.

## Temporary Code

None.
