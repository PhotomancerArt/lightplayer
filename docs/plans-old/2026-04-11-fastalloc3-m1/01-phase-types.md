# Phase 1: SpillAlloc + RegPool Types

## Scope

Create the core data structures for register allocation: `SpillAlloc` for spill
slot management and `RegPool` for physical register tracking with LRU eviction.

## Code Organization Reminders

- One concept per file: `spill.rs` for SpillAlloc, RegPool goes in `walk.rs`
  (it's internal to the walk)
- Tests first, helpers at the bottom
- TODO comments for anything deferred

## Implementation Details

### `fa_alloc/spill.rs` (new file)

```rust
use crate::vinst::VReg;

/// Spill slot allocator.
///
/// Assigns frame-pointer-relative spill slots on demand. Uses u8 slot
/// indices — sufficient for shaders (< 256 spills).
pub struct SpillAlloc {
    /// Spill slot for each vreg index. None = not spilled.
    slots: Vec<Option<u8>>,
    /// Next available slot index.
    next_slot: u8,
}

impl SpillAlloc {
    pub fn new(num_vregs: usize) -> Self {
        Self {
            slots: vec![None; num_vregs],
            next_slot: 0,
        }
    }

    /// Get existing spill slot or assign a new one.
    pub fn get_or_assign(&mut self, vreg: VReg) -> u8 {
        let idx = vreg.0 as usize;
        if let Some(slot) = self.slots[idx] {
            slot
        } else {
            let slot = self.next_slot;
            self.slots[idx] = Some(slot);
            self.next_slot += 1;
            slot
        }
    }

    /// Check if vreg has a spill slot.
    pub fn has_slot(&self, vreg: VReg) -> Option<u8> {
        self.slots[vreg.0 as usize]
    }

    /// Total spill slots used.
    pub fn total_slots(&self) -> u32 {
        self.next_slot as u32
    }
}
```

### `RegPool` in `fa_alloc/walk.rs`

```rust
use crate::rv32::gpr::{self, PReg, ALLOC_POOL};
use crate::vinst::VReg;

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

    /// Allocate a free register for vreg. Returns the PReg.
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
}
```

### Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spill_assign_and_retrieve() {
        let mut s = SpillAlloc::new(4);
        assert_eq!(s.has_slot(VReg(0)), None);
        assert_eq!(s.get_or_assign(VReg(0)), 0);
        assert_eq!(s.get_or_assign(VReg(0)), 0); // same slot
        assert_eq!(s.get_or_assign(VReg(2)), 1);
        assert_eq!(s.total_slots(), 2);
    }

    #[test]
    fn regpool_alloc_free() {
        let mut pool = RegPool::new();
        let (p1, evicted) = pool.alloc(VReg(0));
        assert!(evicted.is_none());
        assert_eq!(pool.home(VReg(0)), Some(p1));

        pool.free(p1);
        assert_eq!(pool.home(VReg(0)), None);
    }

    #[test]
    fn regpool_evicts_lru() {
        let mut pool = RegPool::new();
        let n = ALLOC_POOL.len();
        // Fill all allocatable regs
        for i in 0..n {
            let (_, evicted) = pool.alloc(VReg(i as u16));
            assert!(evicted.is_none());
        }
        // Next alloc should evict
        let (_, evicted) = pool.alloc(VReg(n as u16));
        assert!(evicted.is_some());
    }

    #[test]
    fn regpool_alloc_fixed() {
        let mut pool = RegPool::new();
        let (p, _) = pool.alloc(VReg(0));
        // Force vreg 1 into p, evicting vreg 0
        let evicted = pool.alloc_fixed(p, VReg(1));
        assert_eq!(evicted, Some(VReg(0)));
        assert_eq!(pool.home(VReg(1)), Some(p));
        assert_eq!(pool.home(VReg(0)), None);
    }
}
```

## Validate

```bash
cargo test -p lpvm-native --lib -- fa_alloc
```

Should compile and pass new tests. Existing stub tests may need `#[allow]` or
minor updates if signatures change.
