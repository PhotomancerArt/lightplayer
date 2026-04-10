## Phase 2: Build AllocationAdapter

### Scope

Create `regalloc/adapter.rs` with `AllocationAdapter` that converts the existing
`Allocation` (static vreg→phys map) to `FastAllocation` (per-instruction operand
assignments + edit list).

### Code Organization Reminders

- Place the adapter struct and main `adapt` method first
- Place helper functions (operand filling, edit generation) at the bottom
- Keep related functionality grouped together

### Implementation Details

**Create `regalloc/adapter.rs`:**

```rust
//! Adapter: converts Allocation (static map) to FastAllocation (per-instruction).

use crate::regalloc::{FastAllocation, EditPos, Edit, Location, compute_operand_base, total_operand_count};
use crate::regalloc::{Allocation, PhysReg};
use crate::abi::FuncAbi;
use crate::vinst::VInst;
use crate::isa::rv32::abi::call_clobber_hw;
use lpir::VReg;
use alloc::vec::Vec;

pub struct AllocationAdapter;

impl AllocationAdapter {
    /// Convert an Allocation (static vreg→phys map) to FastAllocation
    /// (per-instruction operand assignments + edits).
    pub fn adapt(
        alloc: &Allocation,
        vinsts: &[VInst],
        func_abi: &FuncAbi,
    ) -> FastAllocation {
        let operand_base = compute_operand_base(vinsts);
        let total_operands = total_operand_count(vinsts);
        let mut operand_allocs = Vec::with_capacity(total_operands);
        
        // Fill operand assignments from the static map
        for (i, inst) in vinsts.iter().enumerate() {
            let base = operand_base[i];
            let mut offset = 0;
            
            // Uses first
            for vreg in inst.uses() {
                let preg = get_vreg_phys(alloc, vreg);
                operand_allocs.push(preg);
                offset += 1;
            }
            
            // Then defs
            for vreg in inst.defs() {
                let preg = get_vreg_phys(alloc, vreg);
                operand_allocs.push(preg);
                offset += 1;
            }
        }
        
        // Generate edits for call save/restore
        let edits = generate_call_edits(alloc, vinsts, func_abi);
        
        FastAllocation {
            operand_allocs,
            operand_base,
            edits,
            num_spill_slots: alloc.spill_count(),
            incoming_stack_params: alloc.incoming_stack_params.clone(),
        }
    }
}

/// Get the physical register for a vreg from the Allocation.
/// Handles spilled vregs by returning a temp register (the old emitter
/// logic handles spills separately).
fn get_vreg_phys(alloc: &Allocation, vreg: VReg) -> PhysReg {
    // TODO: handle spilled vregs - for now assume all vregs have phys regs
    // This matches the current behavior where the emitter does spill loads
    alloc.vreg_to_phys[vreg.0 as usize].unwrap_or(0)
}

/// Generate call save/restore edits for each Call instruction.
/// Replicates the logic from emit.rs `regs_saved_for_call()`.
fn generate_call_edits(
    alloc: &Allocation,
    vinsts: &[VInst],
    func_abi: &FuncAbi,
) -> Vec<(EditPos, Edit)> {
    let mut edits = Vec::new();
    let clobber = call_clobber_hw(func_abi);
    let slot_base = alloc.spill_count() as u32; // call saves go after regalloc spills
    
    for (pos, inst) in vinsts.iter().enumerate() {
        if let VInst::Call { rets, .. } = inst {
            // Compute which regs to save (same logic as regs_saved_for_call)
            let saved = regs_to_save(alloc, rets, clobber);
            
            // Before: save to stack
            for (i, (_, preg)) in saved.iter().enumerate() {
                let slot = slot_base + i as u32;
                edits.push((
                    EditPos::Before(pos),
                    Edit::Move {
                        from: Location::Reg(*preg),
                        to: Location::Stack(slot),
                    },
                ));
            }
            
            // After: restore from stack (reverse order)
            // Skip restore if the preg is home of a return vreg
            let ret_homes: u32 = rets.iter()
                .filter_map(|v| alloc.vreg_to_phys[v.0 as usize])
                .map(|p| 1u32 << p)
                .sum();
            
            for (i, (_, preg)) in saved.iter().enumerate().rev() {
                if ret_homes & (1u32 << *preg) != 0 {
                    continue; // skip restore - this reg holds return value
                }
                let slot = slot_base + i as u32;
                edits.push((
                    EditPos::After(pos),
                    Edit::Move {
                        from: Location::Stack(slot),
                        to: Location::Reg(*preg),
                    },
                ));
            }
        }
    }
    
    edits
}

/// Compute which caller-saved registers need saving.
/// Mirrors `regs_saved_for_call()` in emit.rs.
fn regs_to_save(alloc: &Allocation, rets: &[VReg], clobber: u32) -> Vec<(VReg, PhysReg)> {
    let mut seen = 0u32;
    let mut out = Vec::new();
    
    for (vi, po) in alloc.vreg_to_phys.iter().enumerate() {
        let Some(p) = po else { continue };
        let v = VReg(vi as u32);
        
        if alloc.is_spilled(v) { continue; }
        if rets.contains(&v) { continue; }
        if clobber & (1u32 << *p) == 0 { continue; }
        
        let bit = 1u32 << *p;
        if seen & bit == 0 {
            seen |= bit;
            out.push((v, *p));
        }
    }
    
    out.sort_by_key(|(v, _)| v.0);
    out
}
```

**Add to `regalloc/mod.rs`:**

```rust
pub mod adapter;
pub use adapter::AllocationAdapter;
```

### Tests

Add unit tests in `regalloc/adapter.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::regalloc::GreedyAlloc;
    use lpir::{IrFunction, VReg, IrType};
    use alloc::string::String;
    
    #[test]
    fn test_adapter_produces_valid_fast_allocation() {
        let f = IrFunction {
            name: String::from("test"),
            is_entry: true,
            vmctx_vreg: VReg(0),
            param_count: 0,
            return_types: vec![],
            vreg_types: vec![IrType::I32; 3],
            slots: vec![],
            body: vec![],
            vreg_pool: vec![],
        };
        let vinsts = vec![
            VInst::IConst32 { dst: VReg(1), val: 1, src_op: None },
            VInst::IConst32 { dst: VReg(2), val: 2, src_op: None },
            VInst::Add32 { dst: VReg(3), src1: VReg(1), src2: VReg(2), src_op: None },
        ];
        
        let alloc = GreedyAlloc::new().allocate(&f, &vinsts, 0).unwrap();
        let fast_alloc = AllocationAdapter::adapt(&alloc, &vinsts, /* func_abi */);
        
        // Check that operand_allocs has the right length
        assert_eq!(fast_alloc.operand_allocs.len(), 5); // 2 defs + 2 defs + 2 uses + 1 def
        
        // Check that edits is empty (no calls)
        assert!(fast_alloc.edits.is_empty());
    }
}
```

### Validate

```bash
cargo check -p lpvm-native
cargo test -p lpvm-native --lib regalloc::adapter::tests
```
