## Phase 2: Implement Backward-Walk Core Algorithm

### Scope

Implement `process_instruction` that handles defs, uses, call clobbers, and
IConst32 during the backward walk. This is the algorithmic heart of fastalloc.

### Code Organization Reminders

- Place `process_instruction` method in `FastAllocState` impl block
- Place helper methods (process_def, process_use, process_call) below it
- Keep the backward walk loop in `FastAllocator::allocate`

### Implementation Details

**Add to `FastAllocState` in `fastalloc.rs`:**

```rust
impl FastAllocState {
    /// Process a single instruction during backward walk.
    fn process_instruction(&mut self, pos: usize, inst: &VInst) -> Result<(), NativeError> {
        // Step 1: Handle defs (late) - they die going backward
        for d in inst.defs() {
            self.process_def(pos, d, inst)?;
        }
        
        // Step 2: Handle call clobbers (middle)
        if inst.is_call() {
            self.process_call_clobbers(pos)?;
        }
        
        // Step 3: Handle uses (early) - they become live
        // Collect uses first since we need to process in order
        let uses: Vec<VReg> = inst.uses().collect();
        for u in uses.iter().rev() {
            self.process_use(pos, *u)?;
        }
        
        // Step 4: Handle fixed constraints for call args/rets
        if let VInst::Call { args, rets, .. } = inst {
            self.process_call_constraints(pos, args, rets)?;
        }
        
        Ok(())
    }
    
    /// Process a def: free register, remove from live.
    /// If spilled, add store-after edit.
    fn process_def(&mut self, pos: usize, v: VReg, inst: &VInst) -> Result<(), NativeError> {
        // Special case: IConst32 doesn't need a home
        if let VInst::IConst32 { dst, .. } = inst {
            if *dst == v {
                // IConst32 def: no register home, no spill slot needed
                // Each use will generate imm->reg move directly
                self.live.remove(&v);
                return Ok(());
            }
        }
        
        if let Some(preg) = self.vreg_home[v.0 as usize] {
            // Was in a register - free it
            self.preg_occupant[preg as usize] = None;
            self.touch_lru(preg); // mark as recently freed
            
            // If it has a spill slot, it was live across a region
            // Need to store back to spill slot after the instruction
            if let Some(slot) = self.vreg_spill_slot[v.0 as usize] {
                self.edits.push((EditPos::After(pos), Edit::Move {
                    from: Location::Reg(preg),
                    to: Location::Stack(slot),
                }));
            }
        }
        
        self.vreg_home[v.0 as usize] = None;
        self.live.remove(&v);
        
        Ok(())
    }
    
    /// Process a use: ensure in register, add to live.
    fn process_use(&mut self, pos: usize, v: VReg) -> Result<(), NativeError> {
        // Check if vreg has a spill slot assigned (was evicted earlier)
        let needs_reload = self.vreg_spill_slot[v.0 as usize].is_some()
            && self.vreg_home[v.0 as usize].is_none();
        
        if needs_reload {
            // Need to load from spill
            self.load_from_spill(v, pos)?;
        } else if self.vreg_home[v.0 as usize].is_none() {
            // Not in register and not spilled - need to allocate one
            // This shouldn't happen for most vregs (except maybe special cases)
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
            
            self.vreg_home[v.0 as usize] = Some(preg);
            self.preg_occupant[preg as usize] = Some(v);
        }
        
        // Mark as live and touch LRU
        if let Some(preg) = self.vreg_home[v.0 as usize] {
            self.touch_lru(preg);
        }
        self.live.insert(v);
        
        Ok(())
    }
    
    /// Evict all live values in caller-saved registers to spill slots.
    fn process_call_clobbers(&mut self, pos: usize) -> Result<(), NativeError> {
        use crate::isa::rv32::abi::caller_saved_int;
        
        // Find all live vregs in caller-saved registers
        let to_evict: Vec<(VReg, PhysReg)> = self.live
            .iter()
            .filter_map(|&v| {
                self.vreg_home[v.0 as usize].and_then(|preg| {
                    let p = crate::abi::PReg::int(preg);
                    if caller_saved_int().contains(p) {
                        Some((v, preg))
                    } else {
                        None
                    }
                })
            })
            .collect();
        
        // Evict each one
        for (v, _preg) in to_evict {
            self.evict_to_spill(v, pos, true)?;
        }
        
        Ok(())
    }
    
    /// Ensure call args are in a0-a7 and rets can be captured from a0-a1.
    fn process_call_constraints(
        &mut self,
        _pos: usize,
        _args: &[VReg],
        _rets: &[VReg],
    ) -> Result<(), NativeError> {
        // TODO: For now, args/rets are handled by the normal use/def processing
        // The ABI moves happen in the emitter. This could be optimized to
        // prefer keeping args in arg regs directly.
        Ok(())
    }
}
```

**Update `FastAllocator::allocate` to use the backward walk:**

```rust
impl FastAllocator {
    pub fn allocate(
        vinsts: &[VInst],
        num_vregs: usize,
        initial_homes: &[(VReg, Option<PhysReg>)],
    ) -> Result<FastAllocation, NativeError> {
        if has_control_flow(vinsts) {
            return Err(NativeError::Unimplemented);
        }
        
        let mut state = FastAllocState::new(num_vregs, initial_homes);
        
        // Backward walk
        for (pos, inst) in vinsts.iter().enumerate().rev() {
            state.process_instruction(pos, inst)?;
        }
        
        // TODO: build_allocation (Phase 3)
        state.build_allocation(vinsts)
    }
}
```

### Notes on IConst32 handling

IConst32 is special:
- The def doesn't allocate a register or spill slot
- Each use generates a `Move { from: Imm(k), to: Reg(p) }` edit
- This happens in `process_use` - need to detect IConst32 uses

**Add IConst32 detection in process_use:**

```rust
fn process_use(&mut self, pos: usize, v: VReg, is_iconst: bool, iconst_val: Option<i32>) 
    -> Result<(), NativeError> 
{
    if is_iconst {
        // IConst32 use: generate imm->reg move
        let k = iconst_val.unwrap();
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
            from: Location::Imm(k),
            to: Location::Reg(preg),
        }));
        
        // Don't add to vreg_home - IConst32 has no persistent home
        // Don't add to live - it's rematerialized at each use
        return Ok(());
    }
    
    // ... rest of normal use processing
}
```

Actually, a cleaner approach: track IConst32 values in a separate map, check
in process_use if the vreg is an IConst32 and handle specially.

### Tests

Add tests for backward walk:

```rust
#[cfg(test)]
mod tests {
    use alloc::string::String;
    
    use lpir::{IrFunction, IrType, VReg};
    
    use super::*;
    
    #[test]
    fn backward_walk_simple_add() {
        let vinsts = alloc::vec![
            VInst::IConst32 { dst: VReg(1), val: 1, src_op: None },
            VInst::IConst32 { dst: VReg(2), val: 2, src_op: None },
            VInst::Add32 { dst: VReg(3), src1: VReg(1), src2: VReg(2), src_op: None },
        ];
        
        let initial: &[(VReg, Option<PhysReg>)] = &[];
        let result = FastAllocator::allocate(&vinsts, 4, initial);
        
        // Should succeed for straight-line code
        assert!(result.is_ok());
    }
    
    #[test]
    fn backward_walk_rejects_control_flow() {
        let vinsts = alloc::vec![
            VInst::IConst32 { dst: VReg(1), val: 1, src_op: None },
            VInst::Br { target: 0, src_op: None },
        ];
        
        let initial: &[(VReg, Option<PhysReg>)] = &[];
        let result = FastAllocator::allocate(&vinsts, 2, initial);
        
        assert!(result.is_err());
    }
}
```

### Validate

```bash
cargo check -p lpvm-native
cargo test -p lpvm-native --lib regalloc::fastalloc::tests
```

Expect tests to pass for straight-line, fail/return error for control flow.
