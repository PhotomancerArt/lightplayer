## Phase 3: Implement New Emitter Path

### Scope

Implement `emit_function_bytes_fast()` in `isa/rv32/emit.rs` that consumes
`FastAllocation` instead of `Allocation`. Preprocesses edits into lookup maps,
walks VInsts with index, emits Before/After edits, reads operand pregs from
the flat array.

### Code Organization Reminders

- Place the new `emit_function_bytes_fast()` function near the existing
  `emit_function_bytes()` for comparison
- Place helper functions (edit lowering, operand reading) at the bottom
- Keep edit emission logic grouped together

### Implementation Details

**In `isa/rv32/emit.rs`:**

```rust
/// Emit machine code using the new FastAllocation format.
/// This replaces the old Allocation + use_vreg/def_vreg approach with
/// per-instruction operand assignments and explicit edit splicing.
pub fn emit_function_bytes_fast(
    func: &lpir::IrFunction,
    vinsts: &[VInst],
    fast_alloc: &FastAllocation,
    func_abi: &FuncAbi,
    module_abi: &ModuleAbi,
    options: EmitOptions,
) -> Result<EmittedFunction, NativeError> {
    let mut ctx = EmitContext::new(options.debug_info);
    
    // Preprocess edits into lookup maps
    let mut before_edits: alloc::collections::BTreeMap<usize, Vec<&Edit>> = 
        alloc::collections::BTreeMap::new();
    let mut after_edits: alloc::collections::BTreeMap<usize, Vec<&Edit>> = 
        alloc::collections::BTreeMap::new();
    
    for (pos, edit) in &fast_alloc.edits {
        match pos {
            EditPos::Before(i) => before_edits.entry(*i).or_default().push(edit),
            EditPos::After(i) => after_edits.entry(*i).or_default().push(edit),
        }
    }
    
    // Compute frame layout (same as old path, but using fast_alloc fields)
    let frame = FrameLayout::compute(
        func_abi,
        fast_alloc.num_spill_slots,
        max_caller_outgoing_stack_bytes(vinsts),
        func.total_param_slots() as u32,
    );
    ctx.frame = frame;
    
    // Emit prologue
    emit_prologue(&mut ctx, func_abi, fast_alloc.num_spill_slots)?;
    
    // Walk instructions
    for (i, inst) in vinsts.iter().enumerate() {
        ctx.set_src_op(inst.src_op());
        
        // Emit Before edits
        if let Some(edits) = before_edits.get(&i) {
            for edit in edits {
                emit_edit(&mut ctx, edit, &frame)?;
            }
        }
        
        // Get base offset for this instruction's operands
        let base = fast_alloc.operand_base[i];
        let mut offset = 0;
        
        // Read use operand registers
        let mut use_regs: Vec<PhysReg> = Vec::new();
        for _ in inst.uses() {
            use_regs.push(fast_alloc.operand_allocs[base + offset]);
            offset += 1;
        }
        
        // Read def operand registers
        let mut def_regs: Vec<PhysReg> = Vec::new();
        for _ in inst.defs() {
            def_regs.push(fast_alloc.operand_allocs[base + offset]);
            offset += 1;
        }
        
        // Emit the instruction
        emit_vinst_fast(&mut ctx, inst, &use_regs, &def_regs, &frame)?;
        
        // Emit After edits
        if let Some(edits) = after_edits.get(&i) {
            for edit in edits {
                emit_edit(&mut ctx, edit, &frame)?;
            }
        }
    }
    
    // Apply fixups and return
    ctx.apply_fixups()?;
    Ok(ctx.into_emitted_function())
}

/// Emit a single edit (lower to machine instructions).
fn emit_edit(
    ctx: &mut EmitContext,
    edit: &Edit,
    frame: &FrameLayout,
) -> Result<(), NativeError> {
    use crate::isa::rv32::inst::{encode_sw, encode_lw, encode_addi, iconst32_sequence};
    use crate::abi::S0;
    
    match edit {
        Edit::Move { from, to } => {
            match (from, to) {
                // Reg -> Reg: addi (copy)
                (Location::Reg(src), Location::Reg(dst)) => {
                    if src != dst {
                        ctx.push_u32(encode_addi(*dst as u32, *src as u32, 0));
                    }
                }
                
                // Reg -> Stack: sw
                (Location::Reg(reg), Location::Stack(slot)) => {
                    let offset = spill_slot_offset(*slot, frame)?;
                    ctx.push_u32(encode_sw(*reg as u32, S0.hw as u32, offset));
                }
                
                // Stack -> Reg: lw
                (Location::Stack(slot), Location::Reg(reg)) => {
                    let offset = spill_slot_offset(*slot, frame)?;
                    ctx.push_u32(encode_lw(*reg as u32, S0.hw as u32, offset));
                }
                
                // Imm -> Reg: iconst32_sequence
                (Location::Imm(val), Location::Reg(reg)) => {
                    iconst32_sequence(*reg as u32, *val, |w| ctx.push_u32(w));
                }
                
                _ => {
                    // TODO: handle other cases (Stack -> Stack, etc.)
                    return Err(NativeError::Unimplemented);
                }
            }
        }
    }
    
    Ok(())
}

/// Compute byte offset for a spill slot from frame pointer (s0).
fn spill_slot_offset(slot: u32, frame: &FrameLayout) -> Result<i32, NativeError> {
    // Spill slots are at negative offsets from s0
    // First slot is at -4, second at -8, etc.
    let offset = -((slot + 1) as i32 * 4);
    if !(-2048..=2047).contains(&offset) {
        return Err(NativeError::OffsetOutOfRange(offset));
    }
    Ok(offset)
}

/// Emit a VInst using pre-resolved operand registers.
fn emit_vinst_fast(
    ctx: &mut EmitContext,
    inst: &VInst,
    use_regs: &[PhysReg],
    def_regs: &[PhysReg],
    frame: &FrameLayout,
) -> Result<(), NativeError> {
    use crate::isa::rv32::inst::*;
    
    match inst {
        VInst::IConst32 { .. } => {
            // Already handled by edit generation
            // The def_reg has the target register, value is in edit
            // Actually - need to handle this. The old code generates
            // iconst32_sequence here. With FastAllocation, we should have
            // a Move edit for this... but we don't for IConst32.
            // 
            // For now, emit directly using the first (and only) def reg
            if let Some(dst) = def_regs.first() {
                // This is a bit awkward - we need the const value
                // The edit list approach assumes values are moved, not computed
                // 
                // Alternative: IConst32 generates a Move(Imm, Reg) edit
                // in the adapter. Let's do that.
                unimplemented!("IConst32 needs special handling - see design notes");
            }
        }
        
        VInst::Add32 { .. } => {
            // use_regs[0] = src1, use_regs[1] = src2
            // def_regs[0] = dst
            if use_regs.len() >= 2 && !def_regs.is_empty() {
                ctx.push_u32(encode_add(
                    def_regs[0] as u32,
                    use_regs[0] as u32,
                    use_regs[1] as u32,
                ));
            }
        }
        
        VInst::Call { target, callee_uses_sret, .. } => {
            // Call instruction emission
            // Args are in use_regs (already assigned to ABI registers by adapter)
            // Rets are in def_regs
            
            // Emit auipc + jalr pair
            let offset = ctx.code.len();
            ctx.push_u32(0); // auipc placeholder
            ctx.push_u32(0); // jalr placeholder
            ctx.relocs.push(NativeReloc {
                offset,
                symbol: target.name.clone(),
            });
            ctx.jal_fixups.push(JalFixup {
                instr_offset: offset,
                target: 0, // Not a label, symbol-based
                rd: 1,    // ra
            });
        }
        
        // ... other VInst variants
        
        _ => {
            return Err(NativeError::Unimplemented);
        }
    }
    
    Ok(())
}
```

**Design note on IConst32:** The old emitter generates `iconst32_sequence`
directly when encountering `IConst32`. With the edit list approach, we have
options:

1. Add a `Move { Imm, Reg }` edit for `IConst32` in the adapter
2. Handle `IConst32` specially in `emit_vinst_fast` by looking up the const
   value (need to pass it through somehow)

For M1, let's go with option 1: the adapter generates a `Before` edit for
`IConst32` that moves the immediate into the target register.

### Update Adapter for IConst32

In `regalloc/adapter.rs`, add special handling for `IConst32`:

```rust
// In the operand filling loop:
for (i, inst) in vinsts.iter().enumerate() {
    // ... handle uses and defs as before ...
    
    // Special: IConst32 needs a Move(Imm, Reg) edit
    if let VInst::IConst32 { dst, val, .. } = inst {
        // The def already has a preg assigned
        let dst_preg = operand_allocs[operand_base[i] + inst.uses().count()];
        edits.push((
            EditPos::Before(i),
            Edit::Move {
                from: Location::Imm(*val),
                to: Location::Reg(dst_preg),
            },
        ));
    }
}
```

### Tests

No unit tests in this phase — integration tests in Phase 4 will validate.

### Validate

```bash
cargo check -p lpvm-native
```

Compilation only — filetests come in Phase 4.
