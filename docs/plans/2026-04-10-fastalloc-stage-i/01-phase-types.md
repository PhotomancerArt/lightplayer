## Phase 1: Define FastAllocation Types and Infrastructure

### Scope

Add the new allocation output types to `regalloc/mod.rs`:
- `FastAllocation` struct
- `EditPos` enum (Before/After)
- `Edit` enum (Move)
- `Location` enum (Reg, Stack, Imm)

Add helper methods for computing operand counts and building the `operand_base`
array from VInsts.

### Code Organization Reminders

- Place types and entry points first in the file
- Place helper utility functions at the bottom
- Keep related functionality grouped together
- Add TODO comments for any temporary code

### Implementation Details

**In `regalloc/mod.rs`:**

```rust
/// New allocation output format for fastalloc-style register allocation.
/// Replaces the static `Allocation` vreg_to_phys map with per-instruction
/// operand assignments and explicit move edits.
pub struct FastAllocation {
    /// Flat array of PhysReg assignments for all operands.
    /// Indexed by `operand_base[inst_idx] + operand_offset`.
    pub operand_allocs: Vec<PhysReg>,
    
    /// Base offset into operand_allocs for each instruction.
    /// `operand_base[i]` is the index of the first operand for instruction i.
    /// Length equals number of instructions.
    pub operand_base: Vec<usize>,
    
    /// Move edits to splice between instructions.
    pub edits: Vec<(EditPos, Edit)>,
    
    /// Number of spill slots needed (for frame layout).
    pub num_spill_slots: u32,
    
    /// Incoming stack parameters (same as Allocation).
    pub incoming_stack_params: Vec<(VReg, i32)>,
}

/// Position for an edit relative to an instruction.
pub enum EditPos {
    Before(usize),  // before instruction at index
    After(usize),   // after instruction at index
}

/// An edit to splice into the instruction stream.
pub enum Edit {
    Move { from: Location, to: Location },
}

/// A value location for moves.
pub enum Location {
    Reg(PhysReg),
    Stack(u32),  // spill slot index
    Imm(i32),    // for rematerialization
}
```

**Helper function to compute operand base offsets:**

```rust
/// Compute the base offset for each instruction's operands.
/// Returns a vector where entry i is the starting index in operand_allocs
/// for instruction i's first operand.
pub fn compute_operand_base(vinsts: &[VInst]) -> Vec<usize> {
    let mut base = Vec::with_capacity(vinsts.len());
    let mut offset = 0usize;
    for inst in vinsts {
        base.push(offset);
        offset += inst.uses().count() + inst.defs().count();
    }
    base
}
```

**Helper to get total operand count:**

```rust
/// Total number of operands across all instructions.
pub fn total_operand_count(vinsts: &[VInst]) -> usize {
    vinsts.iter().map(|i| i.uses().count() + i.defs().count()).sum()
}
```

### Tests

Add unit tests in `regalloc/mod.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_compute_operand_base() {
        let vinsts = vec![
            VInst::IConst32 { dst: VReg(0), val: 1, src_op: None },
            VInst::Add32 { dst: VReg(1), src1: VReg(0), src2: VReg(0), src_op: None },
        ];
        let base = compute_operand_base(&vinsts);
        assert_eq!(base, vec![0, 0]); // first has 0 uses+1 def, second starts at 0
    }
    
    #[test]
    fn test_total_operand_count() {
        let vinsts = vec![
            VInst::IConst32 { dst: VReg(0), val: 1, src_op: None }, // 1 def
            VInst::Add32 { dst: VReg(1), src1: VReg(0), src2: VReg(0), src_op: None }, // 2 uses + 1 def
        ];
        assert_eq!(total_operand_count(&vinsts), 4);
    }
}
```

### Validate

```bash
cargo check -p lpvm-native
cargo test -p lpvm-native --lib regalloc::tests
```

No filetests needed yet — this phase just defines types.
