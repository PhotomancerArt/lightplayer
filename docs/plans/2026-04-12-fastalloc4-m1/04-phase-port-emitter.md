# Phase 4: Port Forward Emitter

## Scope

Port the old `lpvm-native` forward emitter (`src/isa/rv32/emit.rs`) to the FA
crate as `rv32/emit.rs`. Adapt for FA VInst types (VReg u16, VRegSlice,
SymbolId).

## Code Organization

`rv32/emit.rs` — new file, contains `EmitContext` and emission logic.

## Reference

Source: `lp-shader/lpvm-native/src/isa/rv32/emit.rs` (~1400 lines)

Key adaptations:
- `lpir::VReg` (u32) → `VReg` (u16)
- `Vec<VReg>` for call args/rets → `VRegSlice` + vreg_pool lookup
- `SymbolRef` → `SymbolId` + `ModuleSymbols` name lookup
- `Allocation` (global vreg→preg) → `AllocOutput` (per-operand)

## Implementation

Create `rv32/emit.rs`:

```rust
//! Forward emitter: VInst + AllocOutput → machine code bytes.
//!
//! Ported from lpvm-native/src/isa/rv32/emit.rs, adapted for FA crate VInst types.

use alloc::vec::Vec;
use alloc::string::String;

use crate::abi::{FrameLayout, FuncAbi};
use crate::fa_alloc::{Alloc, AllocOutput, AllocError};
use crate::rv32::encode::*;
use crate::rv32::gpr::{self, FP_REG, PReg, SP};
use crate::vinst::{IcmpCond, LabelId, ModuleSymbols, SymbolId, VInst, VReg, VRegSlice};

/// Byte offset in `.text` where a relocation applies.
#[derive(Clone, Debug)]
pub struct NativeReloc {
    pub offset: usize,
    pub symbol: String,
}

/// Machine code for one function plus relocations and debug info.
#[derive(Clone, Debug)]
pub struct EmittedCode {
    /// RISC-V machine code bytes.
    pub code: Vec<u8>,
    /// Relocations for auipc+jalr call pairs.
    pub relocs: Vec<NativeReloc>,
    /// Debug line table: (code_offset, optional_src_op).
    pub debug_lines: Vec<(u32, Option<u32>)>,
}

/// Emit context for building machine code.
pub struct EmitContext<'a> {
    code: Vec<u8>,
    relocs: Vec<NativeReloc>,
    debug_lines: Vec<(u32, Option<u32>)>,
    frame: FrameLayout,
    symbols: &'a ModuleSymbols,
    vreg_pool: &'a [VReg],
    label_offsets: Vec<Option<usize>>,
    branch_fixups: Vec<BranchFixup>,
    jal_fixups: Vec<JalFixup>,
}

#[derive(Clone, Copy, Debug)]
struct BranchFixup {
    instr_offset: usize,
    target: LabelId,
    rs1: u32,
    rs2: u32,
    is_beq: bool,
}

#[derive(Clone, Copy, Debug)]
struct JalFixup {
    instr_offset: usize,
    target: LabelId,
    rd: u32,
}

impl<'a> EmitContext<'a> {
    /// Create new emit context.
    pub fn new(
        frame: FrameLayout,
        symbols: &'a ModuleSymbols,
        vreg_pool: &'a [VReg],
    ) -> Self {
        Self {
            code: Vec::new(),
            relocs: Vec::new(),
            debug_lines: Vec::new(),
            frame,
            symbols,
            vreg_pool,
            label_offsets: Vec::new(),
            branch_fixups: Vec::new(),
            jal_fixups: Vec::new(),
        }
    }

    /// Push a 32-bit instruction word.
    fn push_u32(&mut self, w: u32, src_op: Option<u32>) {
        let offset = self.code.len() as u32;
        self.code.extend_from_slice(&w.to_le_bytes());
        if let Some(op) = src_op {
            self.debug_lines.push((offset, Some(op)));
        }
    }

    /// Temporary registers for spill handling.
    const TEMP0: PReg = 5; // t0
    const TEMP1: PReg = 6; // t1
    const TEMP2: PReg = 7; // t2

    /// Get allocation for a specific operand.
    fn operand_alloc(output: &AllocOutput, inst_idx: usize, operand_idx: usize) -> Alloc {
        output.operand_alloc(inst_idx as u16, operand_idx as u16)
    }

    /// Use a vreg: return its physical register, loading from spill if needed.
    fn use_vreg(
        &mut self,
        output: &AllocOutput,
        inst_idx: usize,
        operand_idx: usize,
        temp: PReg,
        src_op: Option<u32>,
    ) -> Result<PReg, AllocError> {
        let alloc = Self::operand_alloc(output, inst_idx, operand_idx);

        match alloc {
            Alloc::Reg(preg) => Ok(preg),
            Alloc::Stack(slot) => {
                // Load from spill slot into temp
                let offset = self.frame.spill_offset_from_fp(slot as u32)
                    .ok_or(AllocError::NotImplemented)?;
                let temp_u32 = temp as u32;
                let fp_u32 = FP_REG as u32;
                self.push_u32(encode_lw(temp_u32, fp_u32, offset), src_op);
                Ok(temp)
            }
            Alloc::None => Err(AllocError::NotImplemented),
        }
    }

    /// Def a vreg: return the physical register to write to.
    fn def_vreg(
        &mut self,
        output: &AllocOutput,
        inst_idx: usize,
        operand_idx: usize,
        temp: PReg,
    ) -> Result<PReg, AllocError> {
        let alloc = Self::operand_alloc(output, inst_idx, operand_idx);

        match alloc {
            Alloc::Reg(preg) => Ok(preg),
            Alloc::Stack(_) => Ok(temp), // Caller must store after
            Alloc::None => Err(AllocError::NotImplemented),
        }
    }

    /// Store a spilled vreg after it was written to a temp.
    fn store_def_vreg(
        &mut self,
        output: &AllocOutput,
        inst_idx: usize,
        operand_idx: usize,
        temp: PReg,
        src_op: Option<u32>,
    ) -> Result<(), AllocError> {
        let alloc = Self::operand_alloc(output, inst_idx, operand_idx);

        if let Alloc::Stack(slot) = alloc {
            let offset = self.frame.spill_offset_from_fp(slot as u32)
                .ok_or(AllocError::NotImplemented)?;
            let temp_u32 = temp as u32;
            let fp_u32 = FP_REG as u32;
            self.push_u32(encode_sw(temp_u32, fp_u32, offset), src_op);
        }
        Ok(())
    }

    /// Emit prologue.
    pub fn emit_prologue(&mut self, is_sret: bool) -> Result<(), AllocError> {
        let sp = SP as u32;
        let frame_size = self.frame.total_size as i32;

        // addi sp, sp, -frame_size
        self.push_u32(encode_addi(sp, sp, -frame_size), None);

        // Save FP if needed
        if let Some(fp_off) = self.frame.fp_offset_from_sp {
            self.push_u32(encode_sw(gpr::S0 as u32, sp, fp_off), None);
        }

        // Save RA if needed
        if let Some(ra_off) = self.frame.ra_offset_from_sp {
            self.push_u32(encode_sw(gpr::RA as u32, sp, ra_off), None);
        }

        // Save callee-saved registers
        for &(preg, off) in &self.frame.callee_save_offsets {
            self.push_u32(encode_sw(preg as u32, sp, off), None);
        }

        // Set up FP
        if self.frame.save_fp {
            self.push_u32(encode_addi(gpr::S0 as u32, sp, frame_size), None);
        }

        // For sret: save sret pointer (a0) to s1
        if is_sret {
            self.push_u32(encode_addi(gpr::S1 as u32, gpr::A0 as u32, 0), None);
        }

        Ok(())
    }

    /// Emit epilogue.
    pub fn emit_epilogue(&mut self) {
        let sp = SP as u32;
        let frame_size = self.frame.total_size as i32;

        // Restore callee-saved (in reverse order)
        for &(preg, off) in self.frame.callee_save_offsets.iter().rev() {
            self.push_u32(encode_lw(preg as u32, sp, off), None);
        }

        // Restore RA if needed
        if let Some(ra_off) = self.frame.ra_offset_from_sp {
            self.push_u32(encode_lw(gpr::RA as u32, sp, ra_off), None);
        }

        // Restore FP if needed
        if let Some(fp_off) = self.frame.fp_offset_from_sp {
            self.push_u32(encode_lw(gpr::S0 as u32, sp, fp_off), None);
        }

        // Restore SP
        self.push_u32(encode_addi(sp, sp, frame_size), None);

        // Return
        self.push_u32(encode_ret(), None);
    }

    /// Ensure label slot exists.
    fn ensure_label_slot(&mut self, id: LabelId) {
        let i = id as usize;
        if i >= self.label_offsets.len() {
            self.label_offsets.resize(i + 1, None);
        }
    }

    /// Record label position.
    fn record_label(&mut self, id: LabelId) -> Result<(), AllocError> {
        self.ensure_label_slot(id);
        if self.label_offsets[id as usize].is_some() {
            return Err(AllocError::NotImplemented);
        }
        self.label_offsets[id as usize] = Some(self.code.len());
        Ok(())
    }

    /// Emit a single VInst.
    fn emit_vinst(
        &mut self,
        vinst: &VInst,
        output: &AllocOutput,
        inst_idx: usize,
    ) -> Result<(), AllocError> {
        // TODO: This is a placeholder for the full implementation.
        // The full implementation needs to:
        // 1. Match on vinst variant
        // 2. For each operand, use use_vreg/def_vreg
        // 3. Emit the appropriate instruction via encode_*
        // 4. Handle edits from the edit list
        //
        // For M1, we just emit a placeholder to get it compiling.
        Ok(())
    }

    /// Resolve branch fixups.
    fn resolve_branch_fixups(&mut self) -> Result<(), AllocError> {
        // TODO: Implement branch fixup resolution
        Ok(())
    }

    /// Finish emission and return the emitted code.
    pub fn finish(mut self) -> Result<EmittedCode, AllocError> {
        self.resolve_branch_fixups()?;
        Ok(EmittedCode {
            code: self.code,
            relocs: self.relocs,
            debug_lines: self.debug_lines,
        })
    }
}

/// Emit a function to machine code.
pub fn emit_function(
    vinsts: &[VInst],
    vreg_pool: &[VReg],
    output: &AllocOutput,
    frame: FrameLayout,
    symbols: &ModuleSymbols,
    is_sret: bool,
) -> Result<EmittedCode, AllocError> {
    let mut ctx = EmitContext::new(frame, symbols, vreg_pool);

    // Emit prologue
    ctx.emit_prologue(is_sret)?;

    // TODO: Apply edits and emit instructions
    // For M1, this is a stub — full implementation in later phases

    // Emit epilogue
    ctx.emit_epilogue();

    ctx.finish()
}
```

## Code Organization Reminders

- Place the emission logic in the `EmitContext` impl block.
- Keep `use_vreg`/`def_vreg`/`store_def_vreg` as the primary spill handling pattern.
- Place prologue/epilogue methods next to each other.
- Place helper methods (ensure_label_slot, record_label) at the bottom.

## Implementation

1. Create `rv32/emit.rs` with the skeleton above
2. The `emit_vinst` method is a stub for M1 — full VInst matching in later work
3. Branch fixups are stubbed — full implementation when we have real branches

## Add to rv32/mod.rs

In `rv32/mod.rs`, add:

```rust
pub mod emit;

pub use emit::{EmittedCode, NativeReloc, emit_function};
```

Remove any `pub mod inst;` or `pub mod rv32_emit;` references.

## Update rv32.rs

In `rv32.rs` (the parent module), update the module declarations:

```rust
// Remove or comment out:
// pub mod inst;
// pub mod rv32_emit;

// Add:
pub mod emit;
```

## Validation

```bash
cargo check -p lpvm-native 2>&1 | head -50
```

Expected: The emitter skeleton should compile. Errors about `emit_vinst` not
handling all VInst variants are expected (it's stubbed).

## Temporary Code

- `emit_vinst` is a stub returning `Ok(())` — full implementation later
- Branch fixups are stubbed
- The actual instruction emission is incomplete

All of these are expected for M1 and will be completed in subsequent work.
