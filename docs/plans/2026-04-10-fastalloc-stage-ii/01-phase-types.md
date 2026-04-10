## Phase 1: Define FastAllocator Types and State

### Scope

Create `regalloc/fastalloc.rs` with the core data structures for the
backward-walk allocator:
- `FastAllocState` - the allocator's mutable state during the backward walk
- `FastAllocator` - the public API struct
- LRU tracking and eviction helpers
- Initialization from parameter homes

### Code Organization Reminders

- Place struct definitions and public API at the top of the file
- Place helper methods (LRU, eviction) at the bottom
- Keep related functionality grouped

### Implementation Details

**Create `regalloc/fastalloc.rs`:**

```rust
//! Fast backward-walk register allocator (straight-line code only).

use alloc::collections::BTreeSet;
use alloc::vec::Vec;

use lpir::VReg;

use crate::error::NativeError;
use crate::regalloc::{Edit, EditPos, FastAllocation, Location, OperandHome, PhysReg};
use crate::vinst::VInst;

/// Maximum vreg index (for empty preg_occupant entries).
const MAX_VREG: VReg = VReg(u32::MAX);

/// Allocator state during backward walk.
struct FastAllocState {
    /// Current home of each vreg: Some(preg) = in register, None = on stack.
    vreg_home: Vec<Option<PhysReg>>,
    
    /// Inverse mapping: which vreg occupies each preg.
    preg_occupant: [Option<VReg>; 32],
    
    /// Set of currently live vregs.
    live: BTreeSet<VReg>,
    
    /// Spill slot for each vreg (lazy assignment).
    vreg_spill_slot: Vec<Option<u32>>,
    /// Next available spill slot index.
    next_spill_slot: u32,
    
    /// LRU ring buffer (circular queue of allocatable registers).
    lru: Vec<PhysReg>,
    /// Index of most-recently-used entry in LRU.
    lru_head: usize,
    
    /// Output edits.
    edits: Vec<(EditPos, Edit)>,
}

impl FastAllocState {
    /// Create new state with initial homes for parameters.
    fn new(num_vregs: usize, initial_homes: &[(VReg, Option<PhysReg>)]) -> Self {
        let mut vreg_home = alloc::vec![None; num_vregs];
        let mut preg_occupant: [Option<VReg>; 32] = [None; 32];
        
        for (v, home) in initial_homes {
            vreg_home[v.0 as usize] = *home;
            if let Some(p) = home {
                preg_occupant[*p as usize] = Some(*v);
            }
        }
        
        Self {
            vreg_home,
            preg_occupant,
            live: BTreeSet::new(),
            vreg_spill_slot: alloc::vec![None; num_vregs],
            next_spill_slot: 0,
            lru: Vec::new(),  // populated with allocatable regs
            lru_head: 0,
            edits: Vec::new(),
        }
    }
    
    /// Mark a register as most-recently-used.
    fn touch_lru(&mut self, preg: PhysReg) {
        // Remove if present, add at head
        if let Some(pos) = self.lru.iter().position(|&p| p == preg) {
            self.lru.remove(pos);
        }
        self.lru.push(preg);
        self.lru_head = self.lru.len().saturating_sub(1);
    }
    
    /// Get the least-recently-used register for eviction.
    fn lru_victim(&self) -> Option<PhysReg> {
        if self.lru.is_empty() {
            None
        } else {
            Some(self.lru[0])
        }
    }
    
    /// Allocate or return existing spill slot for a vreg.
    fn spill_slot(&mut self, v: VReg) -> u32 {
        let vi = v.0 as usize;
        if self.vreg_spill_slot[vi].is_none() {
            self.vreg_spill_slot[vi] = Some(self.next_spill_slot);
            self.next_spill_slot += 1;
        }
        self.vreg_spill_slot[vi].unwrap()
    }
    
    /// Evict a vreg from its register to a spill slot.
    /// Returns the freed register.
    fn evict_to_spill(&mut self, v: VReg, pos: usize, before: bool) -> Result<PhysReg, NativeError> {
        let preg = self.vreg_home[v.0 as usize]
            .ok_or_else(|| NativeError::UnassignedVReg(v.0))?;
        let slot = self.spill_slot(v);
        
        let edit_pos = if before {
            EditPos::Before(pos)
        } else {
            EditPos::After(pos)
        };
        
        self.edits.push((edit_pos, Edit::Move {
            from: Location::Reg(preg),
            to: Location::Stack(slot),
        }));
        
        self.vreg_home[v.0 as usize] = None;
        self.preg_occupant[preg as usize] = None;
        
        Ok(preg)
    }
    
    /// Load a vreg from spill slot into a register.
    /// Allocates a register (evicting LRU if needed).
    fn load_from_spill(&mut self, v: VReg, pos: usize) -> Result<PhysReg, NativeError> {
        let slot = self.vreg_spill_slot[v.0 as usize]
            .ok_or_else(|| NativeError::UnassignedVReg(v.0))?;
        
        // Find a free register or evict
        let preg = if let Some(p) = self.find_free_reg() {
            p
        } else {
            // Evict LRU
            let victim_preg = self.lru_victim()
                .ok_or_else(|| NativeError::Unimplemented)?;
            let victim_vreg = self.preg_occupant[victim_preg as usize]
                .ok_or_else(|| NativeError::Unimplemented)?;
            self.evict_to_spill(victim_vreg, pos, true)?
        };
        
        self.edits.push((EditPos::Before(pos), Edit::Move {
            from: Location::Stack(slot),
            to: Location::Reg(preg),
        }));
        
        self.vreg_home[v.0 as usize] = Some(preg);
        self.preg_occupant[preg as usize] = Some(v);
        self.touch_lru(preg);
        
        Ok(preg)
    }
    
    /// Find any free allocatable register.
    fn find_free_reg(&self) -> Option<PhysReg> {
        // TODO: use ABI allocatable set
        for p in 1u8..=31 {
            if self.preg_occupant[p as usize].is_none() {
                return Some(p);
            }
        }
        None
    }
}

/// Check if VInst sequence has control flow (Label, Br, BrIf).
fn has_control_flow(vinsts: &[VInst]) -> bool {
    vinsts.iter().any(|inst| {
        matches!(inst, VInst::Label(..) | VInst::Br { .. } | VInst::BrIf { .. })
    })
}

/// Public fast allocator API.
pub struct FastAllocator;

impl FastAllocator {
    /// Allocate registers using backward-walk algorithm.
    /// 
    /// # Arguments
    /// * `vinsts` - The VInst sequence (must be straight-line)
    /// * `num_vregs` - Total number of vregs (for array sizing)
    /// * `initial_homes` - Initial register assignments for params
    pub fn allocate(
        vinsts: &[VInst],
        num_vregs: usize,
        initial_homes: &[(VReg, Option<PhysReg>)],
    ) -> Result<FastAllocation, NativeError> {
        if has_control_flow(vinsts) {
            return Err(NativeError::Unimplemented);
        }
        
        let mut state = FastAllocState::new(num_vregs, initial_homes);
        
        // TODO: backward walk (Phase 2)
        // for (pos, inst) in vinsts.iter().enumerate().rev() {
        //     state.process_instruction(pos, inst)?;
        // }
        
        // TODO: build allocation (Phase 3)
        // state.build_allocation(vinsts)
        
        Err(NativeError::Unimplemented)
    }
}
```

### Tests

Add unit tests in `fastalloc.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn state_initializes_with_param_homes() {
        let initial = &[(VReg(0), Some(10)), (VReg(1), Some(11))];
        let state = FastAllocState::new(5, initial);
        
        assert_eq!(state.vreg_home[0], Some(10));
        assert_eq!(state.vreg_home[1], Some(11));
        assert_eq!(state.vreg_home[2], None);
        
        assert_eq!(state.preg_occupant[10], Some(VReg(0)));
        assert_eq!(state.preg_occupant[11], Some(VReg(1)));
    }
    
    #[test]
    fn spill_slot_allocates_lazily() {
        let initial: &[(VReg, Option<PhysReg>)] = &[];
        let mut state = FastAllocState::new(3, initial);
        
        let s0 = state.spill_slot(VReg(0));
        let s1 = state.spill_slot(VReg(1));
        let s0_again = state.spill_slot(VReg(0));
        
        assert_eq!(s0, 0);
        assert_eq!(s1, 1);
        assert_eq!(s0_again, 0); // same slot
        assert_eq!(state.next_spill_slot, 2);
    }
    
    #[test]
    fn detects_control_flow() {
        let with_label = alloc::vec![VInst::Label(0, None)];
        let with_br = alloc::vec![VInst::Br { target: 0, src_op: None }];
        let with_brif = alloc::vec![VInst::BrIf { 
            cond: VReg(0), 
            target: 0, 
            invert: false, 
            src_op: None 
        }];
        let straight = alloc::vec![VInst::Add32 { 
            dst: VReg(0), 
            src1: VReg(1), 
            src2: VReg(2), 
            src_op: None 
        }];
        
        assert!(has_control_flow(&with_label));
        assert!(has_control_flow(&with_br));
        assert!(has_control_flow(&with_brif));
        assert!(!has_control_flow(&straight));
    }
}
```

### Validate

```bash
cargo check -p lpvm-native
cargo test -p lpvm-native --lib regalloc::fastalloc::tests
```
