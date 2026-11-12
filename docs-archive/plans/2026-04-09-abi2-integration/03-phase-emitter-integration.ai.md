## Scope of Phase

Update the RV32 emitter to use `FuncAbi` and `FrameLayout` for prologue generation and sret handling in `VInst::Ret`.

## Code Organization Reminders

- Update `emit_function` signature to take `abi: &FuncAbi` and `is_leaf: bool`
- Add `emit_prologue` helper at bottom of file
- Add `emit_epilogue` helper at bottom of file
- Modify `VInst::Ret` handler to check `abi.is_sret()`
- Keep existing direct-return path intact

## Implementation Details

### Changes to `isa/rv32/emit.rs`

#### 1. Update `emit_function` signature

```rust
pub fn emit_function(
    &self,
    vinsts: &[VInst],
    alloc: &Allocation,
    abi: &FuncAbi,      // NEW
    is_leaf: bool,      // NEW - for frame layout RA save decision
) -> Result<CodeBlob, String> {
    let mut blob = CodeBlob::new();
    
    // Compute which callee-saved registers are used
    let used_callee_saved = compute_used_callee_saved(alloc, abi);
    
    // Compute frame layout
    let spill_count = alloc.spill_count() as u32;
    let frame = FrameLayout::compute(
        abi,
        spill_count,
        used_callee_saved,
        &[], // lpir slots - none for now
        is_leaf,
    );
    
    // Emit prologue
    emit_prologue(&mut blob, abi, &frame)?;
    
    // Emit body
    for vinst in vinsts {
        match vinst {
            VInst::Ret { vals } => {
                emit_ret(&mut blob, vals, alloc, abi, &frame)?;
            }
            // ... other vinst handlers ...
        }
    }
    
    Ok(blob)
}
```

#### 2. Add helper: compute_used_callee_saved

```rust
fn compute_used_callee_saved(alloc: &Allocation, abi: &FuncAbi) -> PregSet {
    let mut used = PregSet::EMPTY;
    
    // Check all assigned registers in the allocation
    for vreg in alloc.assigned_vregs() {
        if let Some(Assignment::Reg(preg)) = alloc.get(vreg) {
            // Check if this preg is in the callee_saved set
            if abi.callee_saved().contains(preg) {
                used.insert(preg);
            }
        }
    }
    
    // For sret, s1 is always "used" (reserved for preservation, needs save/restore)
    if abi.is_sret() {
        used.insert(crate::isa::rv32::abi2::S1);
    }
    
    used
}
```

#### 3. Add helper: emit_prologue

```rust
fn emit_prologue(
    blob: &mut CodeBlob,
    abi: &FuncAbi,
    frame: &FrameLayout,
) -> Result<(), String> {
    use crate::isa::rv32::abi2::{S1, A0, SP};
    use crate::isa::rv32::inst::*;
    
    // 1. Allocate stack frame
    // addi sp, sp, -frame.total_size
    let frame_size = frame.total_size as i32;
    emit_addi(blob, SP.hw, SP.hw, -frame_size)?;
    
    // 2. Save return address (if non-leaf)
    if let Some(ra_offset) = frame.ra_offset_from_sp {
        // sw ra, ra_offset(sp)
        emit_sw(blob, RA.hw, SP.hw, ra_offset)?;
    }
    
    // 3. Save frame pointer
    if let Some(fp_offset) = frame.fp_offset_from_sp {
        // sw fp, fp_offset(sp)
        emit_sw(blob, S0.hw, SP.hw, fp_offset)?;
    }
    
    // 4. Set up frame pointer
    // mv fp, sp
    emit_addi(blob, S0.hw, SP.hw, 0)?;
    
    // 5. Save callee-saved registers
    for (preg, offset) in &frame.callee_save_offsets {
        emit_sw(blob, preg.hw, SP.hw, *offset)?;
    }
    
    // 6. Sret: preserve a0 in s1
    if abi.is_sret() {
        // mv s1, a0 (sret pointer comes in a0, preserve in s1)
        emit_addi(blob, S1.hw, A0.hw, 0)?;
    }
    
    Ok(())
}
```

#### 4. Add helper: emit_epilogue

```rust
fn emit_epilogue(
    blob: &mut CodeBlob,
    frame: &FrameLayout,
) -> Result<(), String> {
    use crate::isa::rv32::abi2::{SP, S0};
    use crate::isa::rv32::inst::*;
    
    // 1. Restore callee-saved registers (reverse order)
    for (preg, offset) in frame.callee_save_offsets.iter().rev() {
        emit_lw(blob, preg.hw, SP.hw, *offset)?;
    }
    
    // 2. Restore frame pointer
    if let Some(fp_offset) = frame.fp_offset_from_sp {
        emit_lw(blob, S0.hw, SP.hw, fp_offset)?;
    }
    
    // 3. Restore return address
    if let Some(ra_offset) = frame.ra_offset_from_sp {
        emit_lw(blob, RA.hw, SP.hw, ra_offset)?;
    }
    
    // 4. Deallocate stack frame
    let frame_size = frame.total_size as i32;
    emit_addi(blob, SP.hw, SP.hw, frame_size)?;
    
    Ok(())
}
```

#### 5. Modify VInst::Ret handler

```rust
fn emit_ret(
    blob: &mut CodeBlob,
    vals: &[VReg],
    alloc: &Allocation,
    abi: &FuncAbi,
    frame: &FrameLayout,
) -> Result<(), String> {
    use crate::isa::rv32::abi2::{S1, A0, A1};
    use crate::isa::rv32::inst::*;
    
    if abi.is_sret() {
        // Store return values to sret buffer at s1
        for (i, vreg) in vals.iter().enumerate() {
            let src = alloc.get(*vreg)
                .and_then(|a| a.preg())
                .ok_or("Unassigned vreg in sret Ret")?;
            
            let offset = (i * 4) as i32;
            // sw src, offset(s1)
            emit_sw(blob, src.hw, S1.hw, offset)?;
        }
        
        // Epilogue
        emit_epilogue(blob, frame)?;
        
        // Return (buffer address already in a0 from caller, or we reload s1 to a0)
        // For safety, reload s1 to a0
        emit_addi(blob, A0.hw, S1.hw, 0)?;
        emit_jalr(blob, 0, RA.hw, 0)?; // ret
        
    } else {
        // Direct return - move to a0-a1
        for (i, vreg) in vals.iter().enumerate() {
            let src = alloc.get(*vreg)
                .and_then(|a| a.preg())
                .ok_or("Unassigned vreg in direct Ret")?;
            
            let dst_reg = match i {
                0 => A0.hw,
                1 => A1.hw,
                _ => return Err(format!("Too many return values: {}", vals.len())),
            };
            
            emit_addi(blob, dst_reg, src.hw, 0)?; // mv
        }
        
        // Epilogue
        emit_epilogue(blob, frame)?;
        
        // Return
        emit_jalr(blob, 0, RA.hw, 0)?; // ret
    }
    
    Ok(())
}
```

### Testing Strategy

Since we don't have actual emission tests that run generated code yet, validate by:

1. **Compilation** - code builds without errors
2. **Existing tests** - all 82+ tests still pass (no regressions)
3. **Code inspection** - disassemble output and verify pattern

Add a basic unit test:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn emit_leaf_prologue_no_ra_save() {
        // Create simple function with leaf=true
        // Verify no RA save in prologue
    }
    
    #[test]
    fn emit_sret_prologue_preserves_a0() {
        // Create sret function
        // Verify mv s1, a0 emitted
    }
}
```

## Validate

```bash
# Build check
cargo check -p lpvm-native

# All tests pass
cargo test -p lpvm-native

# No warnings
cargo check -p lpvm-native --tests

# Format
cargo +nightly fmt -p lpvm-native
```
