# Phase 3: Label Backpatching

## Scope of Phase

Implement single-pass label resolution with backpatching in the emit path.

## Code Organization Reminders

- Add backpatch state to `EmitContext`
- Keep the main emit loop simple: just call helper methods
- Backpatch logic should be contained in a few focused methods

## Implementation Details

### 1. Add backpatch state to `EmitContext`

In `isa/rv32/emit.rs`, add to `EmitContext`:

```rust
/// Pending branch fixups: (byte_offset, target_label_id)
pending_fixups: Vec<(usize, LabelId)>,

/// Resolved label positions: label_id -> byte_offset
label_offsets: alloc::vec::Vec<Option<usize>>,
```

Update `EmitContext::with_frame` to initialize these:
```rust
pending_fixups: Vec::new(),
label_offsets: Vec::new(),
```

### 2. Add backpatch methods

```rust
/// Record that a label is at the current code position.
/// Resolves any pending fixups for this label.
fn record_label(&mut self, label: LabelId) {
    let offset = self.code.len();
    
    // Ensure label_offsets is big enough
    if label as usize >= self.label_offsets.len() {
        self.label_offsets.resize(label as usize + 1, None);
    }
    self.label_offsets[label as usize] = Some(offset);
    
    // Resolve any pending fixups for this label
    self.resolve_fixups_for_label(label, offset);
}

/// Resolve all pending fixups for a label.
fn resolve_fixups_for_label(&mut self, label: LabelId, target_offset: usize) {
    // Collect fixups to resolve
    let to_resolve: Vec<(usize, LabelId)> = self
        .pending_fixups
        .iter()
        .filter(|(_, l)| *l == label)
        .copied()
        .collect();
    
    // Remove resolved fixups from pending
    self.pending_fixups.retain(|(_, l)| *l != label);
    
    // Patch each instruction
    for (instr_offset, _) in to_resolve {
        let pc_relative = target_offset as i32 - instr_offset as i32;
        
        // Re-encode the branch with correct offset
        // We need to know which instruction type - this is tricky
        // For now, assume all branches are BEQ/BNE with same encoding
        // TODO: store instruction type in fixup
        
        let instr_bytes = &mut self.code[instr_offset..instr_offset + 4];
        let mut instr = u32::from_le_bytes([
            instr_bytes[0], instr_bytes[1], instr_bytes[2], instr_bytes[3],
        ]);
        
        // Patch the immediate field
        // This depends on instruction type - need to handle carefully
        // For BEQ/BNE: B-type immediate
        // For JAL: J-type immediate
        
        // Simple approach: re-encode from scratch if we stored enough info
        // Better approach: store original rs1, rs2, and instr type in fixup
        
        // For now, placeholder - we'll refine this
        let _ = pc_relative; // suppress warning
        let _ = instr;
    }
}

/// Queue a fixup for a forward branch.
fn queue_fixup(&mut self, instr_offset: usize, target: LabelId) {
    self.pending_fixups.push((instr_offset, target));
}

/// Verify all fixups were resolved.
fn final_backpatch_check(&self) -> Result<(), NativeError> {
    if !self.pending_fixups.is_empty() {
        return Err(NativeError::UnresolvedLabels(
            self.pending_fixups.len()
        ));
    }
    Ok(())
}
```

### 3. Problem: Need to store more in fixups

The above approach has a flaw: we need to know the instruction type and operands to re-encode. Better approach: store everything needed in the fixup.

Revised fixup type:
```rust
enum BranchType {
    Beq { rs1: u32, rs2: u32 },  // branch if rs1 == rs2
    Bne { rs1: u32, rs2: u32 },  // branch if rs1 != rs2
    Jal { rd: u32 },              // jump and link
}

struct Fixup {
    instr_offset: usize,
    target: LabelId,
    branch_type: BranchType,
}
```

Or simpler: just store the pre-encoded instruction with a placeholder offset, then patch just the immediate bits.

### 4. Simpler approach: Emit placeholder, patch immediate bits

For B-type (beq/bne):
- Encode with placeholder offset (e.g., 0)
- Store: offset in code, target label, rs1, rs2, is_beq
- When resolving: re-encode with real offset, overwrite bytes

For J-type (jal):
- Same pattern, different immediate encoding

Revised:
```rust
struct BranchFixup {
    offset: usize,        // Position in code buffer
    target: LabelId,
    rs1: u32,            // For BEQ/BNE (x0 for BrIf invert cases)
    rs2: u32,            // For BEQ/BNE (x0 for BrIf)
    is_beq: bool,        // true=BEQ, false=BNE
}

struct JalFixup {
    offset: usize,
    target: LabelId,
    rd: u32,
}
```

### 5. Revised emit methods

```rust
/// Emit BEQ with backpatching support.
fn emit_beq(&mut self, rs1: u32, rs2: u32, target: LabelId) -> Result<(), NativeError> {
    if let Some(target_offset) = self.get_label_offset(target) {
        // Target already known - emit directly
        let pc_relative = target_offset as i32 - self.code.len() as i32;
        self.push_u32(encode_beq(rs1, rs2, pc_relative));
    } else {
        // Forward reference - queue fixup
        let instr_offset = self.code.len();
        self.push_u32(0); // placeholder
        self.branch_fixups.push(BranchFixup {
            offset: instr_offset,
            target,
            rs1,
            rs2,
            is_beq: true,
        });
    }
    Ok(())
}

/// Get label offset if already resolved.
fn get_label_offset(&self, label: LabelId) -> Option<usize> {
    self.label_offsets.get(label as usize).copied().flatten()
}
```

### 6. Alternative: Two tiny passes

Given the complexity of storing fixup state, consider a simpler two-pass approach within emit:
- Pass 1: Iterate VInsts, emit all non-branch instructions, record label positions, emit placeholder zeros for branches
- Pass 2: Iterate fixups, patch branch instructions with correct offsets

This is cleaner and only adds one extra small loop over fixups (not all VInsts).

Let's go with the two-pass approach for simplicity.

## Revised Implementation (Two-Pass)

```rust
pub fn emit_function_bytes(
    func: &lpir::IrFunction,
    fn_sig: &lps_shared::LpsFnSig,
    float_mode: lpir::FloatMode,
    debug_info: bool,
) -> Result<EmittedFunction, NativeError> {
    let vinsts = crate::lower::lower_ops(func, float_mode)?;
    // ... existing setup ...
    
    let mut ctx = EmitContext::with_frame(frame, debug_info);
    ctx.emit_prologue(is_sret);
    
    // Pass 1: Collect label positions, emit everything
    for v in &vinsts {
        ctx.collect_or_emit(v, &alloc, is_sret)?;
    }
    
    // Pass 2: Backpatch all branches
    ctx.resolve_all_fixups()?;
    
    ctx.emit_epilogue();
    Ok(EmittedFunction {
        code: ctx.code,
        relocs: ctx.relocs,
        debug_lines: ctx.debug_lines,
    })
}

fn collect_or_emit(&mut self, inst: &VInst, alloc: &Allocation, is_sret: bool) 
    -> Result<(), NativeError> {
    match inst {
        VInst::Label(id, _) => {
            self.label_offsets.push((*id, self.code.len()));
            Ok(())
        }
        VInst::Br { target, .. } => {
            // Emit placeholder, queue fixup
            let offset = self.code.len();
            self.push_u32(0);
            self.jal_fixups.push(JalFixup { offset, target: *target, rd: 0 });
            Ok(())
        }
        VInst::BrIf { cond, target, invert, .. } => {
            // Emit beq/bne with placeholder
            let rs = self.use_vreg(alloc, *cond, Self::TEMP0)? as u32;
            let offset = self.code.len();
            self.push_u32(0);
            self.branch_fixups.push(BranchFixup {
                offset,
                target: *target,
                rs1: rs,
                rs2: 0, // compare to x0
                is_beq: *invert, // beq = branch if equal to 0 (false)
            });
            self.store_def_vreg(alloc, *cond, Self::TEMP0);
            Ok(())
        }
        _ => self.emit_vinst(inst, alloc, is_sret),
    }
}

fn resolve_all_fixups(&mut self) -> Result<(), NativeError> {
    // Build label map
    let label_map: alloc::collections::BTreeMap<_, _> = 
        self.label_offsets.iter().copied().collect();
    
    for fixup in &self.branch_fixups {
        let target_offset = label_map.get(&fixup.target)
            .ok_or(NativeError::UnresolvedLabel(fixup.target))?;
        let pc_relative = *target_offset as i32 - fixup.offset as i32;
        
        let instr = if fixup.is_beq {
            encode_beq(fixup.rs1, fixup.rs2, pc_relative)
        } else {
            encode_bne(fixup.rs1, fixup.rs2, pc_relative)
        };
        
        // Patch in place
        let bytes = instr.to_le_bytes();
        self.code[fixup.offset..fixup.offset + 4].copy_from_slice(&bytes);
    }
    
    for fixup in &self.jal_fixups {
        let target_offset = label_map.get(&fixup.target)
            .ok_or(NativeError::UnresolvedLabel(fixup.target))?;
        let pc_relative = *target_offset as i32 - fixup.offset as i32;
        let instr = encode_jal(fixup.rd, pc_relative);
        let bytes = instr.to_le_bytes();
        self.code[fixup.offset..fixup.offset + 4].copy_from_slice(&bytes);
    }
    
    Ok(())
}
```

## Tests

Add test in `emit.rs`:

```rust
#[test]
fn emit_branch_forward_reference() {
    // Label(0), BrIf(v0, target=0, invert=true)
    // Should emit: beq v0, x0, -4 (or similar)
}
```

## Validate

```bash
cargo test -p lpvm-native
```
