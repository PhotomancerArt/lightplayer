# Phase 1: Backward Walk Allocator

## Scope

Create `fa_alloc/walk.rs` with the backward walk algorithm for Linear regions.

## Implementation

### File: `fa_alloc/walk.rs`

Structure:
```rust
use crate::fa_alloc::{Alloc, AllocError, AllocOutput, Edit, EditPoint};
use crate::fa_alloc::pool::RegPool;
use crate::fa_alloc::spill::SpillAlloc;
use crate::fa_alloc::trace::AllocTrace;
use crate::abi::FuncAbi;
use crate::vinst::{VInst, VReg};
use alloc::vec::Vec;

/// Walk a Linear region backward, producing AllocOutput.
pub fn walk_linear(
    vinsts: &[VInst],
    vreg_pool: &[VReg],
    func_abi: &FuncAbi,
) -> Result<AllocOutput, AllocError> {
    let mut pool = RegPool::new();
    let mut spill = SpillAlloc::new();
    let mut trace = AllocTrace::new();
    let mut edits: Vec<(EditPoint, Edit)> = Vec::new();
    
    // TODO: Calculate total operands and build inst_alloc_offsets
    // TODO: Seed pool with entry parameters
    // TODO: Walk instructions backward
    // TODO: Reverse edits
    // TODO: Return AllocOutput
    
    Err(AllocError::NotImplemented)
}

/// Per-instruction state during backward walk.
struct WalkState {
    // TODO: fields needed during walk
}

impl WalkState {
    /// Process one instruction in backward order.
    fn process_inst(
        &mut self,
        inst_idx: usize,
        inst: &VInst,
        vreg_pool: &[VReg],
        pool: &mut RegPool,
        spill: &mut SpillAlloc,
        edits: &mut Vec<(EditPoint, Edit)>,
    ) -> Result<(), AllocError> {
        // TODO: Process defs (free registers)
        // TODO: Process uses (allocate, evict if needed)
        // TODO: Record allocations
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    // TODO: Unit tests for walk components
}
```

### Steps

1. **Operand counting**: Build `inst_alloc_offsets` by counting defs+uses per VInst
   - Use `vinst.for_each_def()` and `vinst.for_each_use()` to count
   - Offsets[i] = cumulative sum of operands before instruction i

2. **Entry param seeding**: Before walk, for each param in `func_abi.precolors()`:
   - Call `pool.alloc_fixed(abi_reg, vreg)` to seed at ABI register

3. **Backward walk**: Iterate `vinsts.iter().enumerate().rev()`:
   - **Process defs**: For each def vreg:
     - Check if in pool, if so free it
     - Record final allocation (Reg or Stack based on where it ended up)
   - **Process uses**: For each use vreg:
     - If in pool: touch (MRU) and record allocation
     - If spilled: reload from slot, record allocation, add edit
     - If not allocated: allocate fresh reg, evict LRU if needed, record edit if evicted

4. **Entry move recording**: After walk, for each param:
   - If `pool.home(vreg) != abi_reg`, record `Edit::Move` at `EditPoint::Before(0)`

5. **Finalize**: Reverse edits, build `AllocOutput`

## Code Organization

- Place `walk_linear` at top (entry point)
- Place `WalkState` struct and impl below
- Place tests at bottom
- Keep related methods grouped together

## Validate

```bash
cargo check -p lpvm-native-fa
cargo test -p lpvm-native-fa fa_alloc::walk::tests
```

Should compile. Tests will fail (NotImplemented) until Phase 1 complete.
