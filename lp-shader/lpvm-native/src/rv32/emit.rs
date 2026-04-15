//! Forward emitter: VInst + AllocOutput → machine code bytes.
//!
//! Ported from lpvm-native/src/isa/rv32c/emit.rs, adapted for FA crate VInst types.

use alloc::string::String;
use alloc::vec::Vec;

use crate::abi::FrameLayout;
use crate::alloc::{Alloc, AllocError, AllocOutput, Edit, EditPoint};
use crate::rv32::encode::*;
use crate::rv32::gpr::{ARG_REGS, FP_REG, PReg, RA_REG, RET_REGS, SP_REG};
use crate::vinst::{IcmpCond, LabelId, ModuleSymbols, VInst, VReg};

/// Callee sret buffer pointer (saved from incoming a0).
const S1_REG: PReg = 9;

#[inline]
fn branch_offset_valid(imm: i32) -> bool {
    imm % 2 == 0 && (-4096..=4094).contains(&imm)
}

#[inline]
fn jal_offset_valid(imm: i32) -> bool {
    imm % 2 == 0 && imm >= -(1 << 20) && imm <= (1 << 20) - 2
}

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
    /// Kept for API parity with [`emit_function`] (e.g. future pool-indexed lowering).
    #[allow(dead_code)]
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
    /// Create new emit context with pre-sized buffers.
    /// `expected_insts` is a hint for code buffer sizing (typically vinsts.len()).
    pub fn new(
        frame: FrameLayout,
        symbols: &'a ModuleSymbols,
        vreg_pool: &'a [VReg],
        expected_insts: usize,
    ) -> Self {
        // Estimate ~4 bytes per instruction + prologue/epilogue (~64 bytes)
        let code_cap = expected_insts.saturating_mul(4).saturating_add(64);
        Self {
            code: Vec::with_capacity(code_cap),
            relocs: Vec::new(),
            debug_lines: Vec::with_capacity(expected_insts),
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

    fn is_dead_def(output: &AllocOutput, inst_idx: usize, def_op_idx: usize) -> bool {
        matches!(
            Self::operand_alloc(output, inst_idx, def_op_idx),
            Alloc::None
        )
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
                let offset = self
                    .frame
                    .spill_offset_from_fp(slot as u32)
                    .ok_or(crate::emit_err!())?;
                let temp_u32 = temp as u32;
                let fp_u32 = FP_REG as u32;
                self.push_u32(encode_lw(temp_u32, fp_u32, offset), src_op);
                Ok(temp)
            }
            Alloc::None => Err(crate::emit_err!()),
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
            Alloc::None => Err(crate::emit_err!()),
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
            let offset = self
                .frame
                .spill_offset_from_fp(slot as u32)
                .ok_or(crate::emit_err!())?;
            let temp_u32 = temp as u32;
            let fp_u32 = FP_REG as u32;
            self.push_u32(encode_sw(temp_u32, fp_u32, offset), src_op);
        }
        Ok(())
    }

    /// Emit an allocator edit (reload/spill/reg move) as concrete instructions.
    fn emit_edit(&mut self, edit: &Edit, src_op: Option<u32>) -> Result<(), AllocError> {
        match edit {
            Edit::Move { from, to } => match (*from, *to) {
                (Alloc::None, _) | (_, Alloc::None) => return Err(crate::emit_err!()),
                (Alloc::Reg(src), Alloc::Reg(dst)) => {
                    if src != dst {
                        self.push_u32(encode_addi(dst as u32, src as u32, 0), src_op);
                    }
                }
                (Alloc::Stack(slot), Alloc::Reg(dst)) => {
                    let offset = self
                        .frame
                        .spill_offset_from_fp(slot as u32)
                        .ok_or(crate::emit_err!())?;
                    self.push_u32(encode_lw(dst as u32, FP_REG as u32, offset), src_op);
                }
                (Alloc::Reg(src), Alloc::Stack(slot)) => {
                    let offset = self
                        .frame
                        .spill_offset_from_fp(slot as u32)
                        .ok_or(crate::emit_err!())?;
                    self.push_u32(encode_sw(src as u32, FP_REG as u32, offset), src_op);
                }
                (Alloc::Stack(s_from), Alloc::Stack(s_to)) => {
                    let o_from = self
                        .frame
                        .spill_offset_from_fp(s_from as u32)
                        .ok_or(crate::emit_err!())?;
                    let o_to = self
                        .frame
                        .spill_offset_from_fp(s_to as u32)
                        .ok_or(crate::emit_err!())?;
                    let t = Self::TEMP0 as u32;
                    self.push_u32(encode_lw(t, FP_REG as u32, o_from), src_op);
                    self.push_u32(encode_sw(t, FP_REG as u32, o_to), src_op);
                }
            },
            Edit::LoadIncomingArg { fp_offset, to } => match *to {
                Alloc::Reg(dst) => {
                    self.push_u32(encode_lw(dst as u32, FP_REG as u32, *fp_offset), src_op);
                }
                Alloc::Stack(slot) => {
                    let spill_off = self
                        .frame
                        .spill_offset_from_fp(slot as u32)
                        .ok_or(crate::emit_err!())?;
                    let t = Self::TEMP0 as u32;
                    self.push_u32(encode_lw(t, FP_REG as u32, *fp_offset), src_op);
                    self.push_u32(encode_sw(t, FP_REG as u32, spill_off), src_op);
                }
                Alloc::None => return Err(crate::emit_err!()),
            },
        }
        Ok(())
    }

    fn label_offset_get(&self, id: LabelId) -> Option<usize> {
        self.label_offsets.get(id as usize).copied().flatten()
    }

    /// Emit prologue.
    pub fn emit_prologue(&mut self, is_sret: bool) -> Result<(), AllocError> {
        let sp = SP_REG as u32;
        let frame_size = self.frame.total_size as i32;

        // addi sp, sp, -frame_size (skip if frame_size is 0)
        if frame_size != 0 {
            self.push_u32(encode_addi(sp, sp, -frame_size), None);
        }

        // Save FP if needed
        if let Some(fp_off) = self.frame.fp_offset_from_sp {
            self.push_u32(encode_sw(FP_REG as u32, sp, fp_off), None);
        }

        // Save RA if needed
        if let Some(ra_off) = self.frame.ra_offset_from_sp {
            self.push_u32(encode_sw(RA_REG as u32, sp, ra_off), None);
        }

        // Save callee-saved registers
        let callee_saves: Vec<_> = self.frame.callee_save_offsets.clone();
        for (preg, off) in callee_saves {
            self.push_u32(encode_sw(preg.hw as u32, sp, off), None);
        }

        // Set up FP
        if self.frame.save_fp {
            self.push_u32(encode_addi(FP_REG as u32, sp, frame_size), None);
        }

        // sret: preserve incoming struct-return pointer (a0) in s1 for Ret stores / callee
        if is_sret {
            self.push_u32(encode_addi(S1_REG as u32, ARG_REGS[0] as u32, 0), None);
        }

        Ok(())
    }

    /// Emit epilogue.
    pub fn emit_epilogue(&mut self) {
        let sp = SP_REG as u32;
        let frame_size = self.frame.total_size as i32;

        // Restore callee-saved (in reverse order)
        let callee_saves: Vec<_> = self
            .frame
            .callee_save_offsets
            .iter()
            .rev()
            .cloned()
            .collect();
        for (preg, off) in callee_saves {
            self.push_u32(encode_lw(preg.hw as u32, sp, off), None);
        }

        // Restore RA if needed
        if let Some(ra_off) = self.frame.ra_offset_from_sp {
            self.push_u32(encode_lw(RA_REG as u32, sp, ra_off), None);
        }

        // Restore FP if needed
        if let Some(fp_off) = self.frame.fp_offset_from_sp {
            self.push_u32(encode_lw(FP_REG as u32, sp, fp_off), None);
        }

        // Restore SP (skip if frame_size is 0)
        if frame_size != 0 {
            self.push_u32(encode_addi(sp, sp, frame_size), None);
        }

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
            return Err(crate::emit_err!());
        }
        self.label_offsets[id as usize] = Some(self.code.len());
        Ok(())
    }

    fn emit_icmp(
        &mut self,
        output: &AllocOutput,
        inst_idx: usize,
        cond: IcmpCond,
        src_op: Option<u32>,
    ) -> Result<(), AllocError> {
        if Self::is_dead_def(output, inst_idx, 0) {
            return Ok(());
        }
        let rs_l = self.use_vreg(output, inst_idx, 1, Self::TEMP0, src_op)? as u32;
        let rs_r = self.use_vreg(output, inst_idx, 2, Self::TEMP1, src_op)? as u32;
        let rd = self.def_vreg(output, inst_idx, 0, Self::TEMP0)? as u32;
        let scratch = Self::TEMP2 as u32;

        match cond {
            IcmpCond::LtS => self.push_u32(encode_slt(rd, rs_l, rs_r), src_op),
            IcmpCond::LtU => self.push_u32(encode_sltu(rd, rs_l, rs_r), src_op),
            IcmpCond::GtS => self.push_u32(encode_slt(rd, rs_r, rs_l), src_op),
            IcmpCond::GtU => self.push_u32(encode_sltu(rd, rs_r, rs_l), src_op),
            IcmpCond::Eq => {
                self.push_u32(encode_xor(scratch, rs_l, rs_r), src_op);
                self.push_u32(encode_sltiu(rd, scratch, 1), src_op);
            }
            IcmpCond::Ne => {
                self.push_u32(encode_xor(scratch, rs_l, rs_r), src_op);
                self.push_u32(encode_sltu(rd, 0, scratch), src_op);
            }
            IcmpCond::LeS => {
                self.push_u32(encode_slt(scratch, rs_r, rs_l), src_op);
                self.push_u32(encode_xori(rd, scratch, 1), src_op);
            }
            IcmpCond::LeU => {
                self.push_u32(encode_sltu(scratch, rs_r, rs_l), src_op);
                self.push_u32(encode_xori(rd, scratch, 1), src_op);
            }
            IcmpCond::GeS => {
                self.push_u32(encode_slt(scratch, rs_l, rs_r), src_op);
                self.push_u32(encode_xori(rd, scratch, 1), src_op);
            }
            IcmpCond::GeU => {
                self.push_u32(encode_sltu(scratch, rs_l, rs_r), src_op);
                self.push_u32(encode_xori(rd, scratch, 1), src_op);
            }
        }

        self.store_def_vreg(output, inst_idx, 0, Self::TEMP0, src_op)
    }

    fn emit_ieq_imm(
        &mut self,
        output: &AllocOutput,
        inst_idx: usize,
        imm: i32,
        src_op: Option<u32>,
    ) -> Result<(), AllocError> {
        if Self::is_dead_def(output, inst_idx, 0) {
            return Ok(());
        }
        let mut rs = self.use_vreg(output, inst_idx, 1, Self::TEMP0, src_op)? as u32;
        let rd = self.def_vreg(output, inst_idx, 0, Self::TEMP0)? as u32;
        const IMM12: core::ops::RangeInclusive<i32> = -2048_i32..=2047_i32;

        if IMM12.contains(&imm) {
            let scratch = Self::TEMP2 as u32;
            self.push_u32(encode_xori(scratch, rs, imm), src_op);
            self.push_u32(encode_sltiu(rd, scratch, 1), src_op);
        } else {
            if rs == Self::TEMP1 as u32 {
                self.push_u32(encode_addi(Self::TEMP0 as u32, rs, 0), src_op);
                rs = Self::TEMP0 as u32;
            }
            for w in iconst32_sequence(Self::TEMP1 as u32, imm) {
                self.push_u32(w, src_op);
            }
            self.push_u32(
                encode_xor(Self::TEMP2 as u32, rs, Self::TEMP1 as u32),
                src_op,
            );
            self.push_u32(encode_sltiu(rd, Self::TEMP2 as u32, 1), src_op);
        }
        self.store_def_vreg(output, inst_idx, 0, Self::TEMP0, src_op)
    }

    fn emit_select32(
        &mut self,
        output: &AllocOutput,
        inst_idx: usize,
        src_op: Option<u32>,
    ) -> Result<(), AllocError> {
        if Self::is_dead_def(output, inst_idx, 0) {
            return Ok(());
        }
        let p_true = self.use_vreg(output, inst_idx, 2, Self::TEMP0, src_op)? as u32;
        let p_false = self.use_vreg(output, inst_idx, 3, Self::TEMP1, src_op)? as u32;
        let p_cond = self.use_vreg(output, inst_idx, 1, Self::TEMP2, src_op)? as u32;

        // Negate boolean condition (0/1) into a full bitmask (0x0/0xFFFFFFFF).
        // Without this, `and` with 1 zeroes all but the LSB, making select
        // always return the false value for Q32 (and most multi-bit) operands.
        self.push_u32(encode_sub(Self::TEMP2 as u32, 0, p_cond), src_op);
        self.push_u32(encode_sub(Self::TEMP0 as u32, p_true, p_false), src_op);
        self.push_u32(
            encode_and(Self::TEMP0 as u32, Self::TEMP0 as u32, Self::TEMP2 as u32),
            src_op,
        );

        let rd = self.def_vreg(output, inst_idx, 0, Self::TEMP0)? as u32;
        self.push_u32(encode_add(rd, Self::TEMP0 as u32, p_false), src_op);
        self.store_def_vreg(output, inst_idx, 0, Self::TEMP0, src_op)
    }

    /// Emit a single VInst.
    fn emit_vinst(
        &mut self,
        vinst: &VInst,
        output: &AllocOutput,
        inst_idx: usize,
        is_sret: bool,
    ) -> Result<(), AllocError> {
        let src_op = vinst.src_op();
        match vinst {
            VInst::Add32 { .. } => {
                if Self::is_dead_def(output, inst_idx, 0) {
                    return Ok(());
                }
                let rs1 = self.use_vreg(output, inst_idx, 1, Self::TEMP0, src_op)? as u32;
                let rs2 = self.use_vreg(output, inst_idx, 2, Self::TEMP1, src_op)? as u32;
                let rd = self.def_vreg(output, inst_idx, 0, Self::TEMP0)? as u32;
                self.push_u32(encode_add(rd, rs1, rs2), src_op);
                self.store_def_vreg(output, inst_idx, 0, Self::TEMP0, src_op)?;
            }
            VInst::Sub32 { .. } => {
                if Self::is_dead_def(output, inst_idx, 0) {
                    return Ok(());
                }
                let rs1 = self.use_vreg(output, inst_idx, 1, Self::TEMP0, src_op)? as u32;
                let rs2 = self.use_vreg(output, inst_idx, 2, Self::TEMP1, src_op)? as u32;
                let rd = self.def_vreg(output, inst_idx, 0, Self::TEMP0)? as u32;
                self.push_u32(encode_sub(rd, rs1, rs2), src_op);
                self.store_def_vreg(output, inst_idx, 0, Self::TEMP0, src_op)?;
            }
            VInst::Neg32 { .. } => {
                if Self::is_dead_def(output, inst_idx, 0) {
                    return Ok(());
                }
                let rs = self.use_vreg(output, inst_idx, 1, Self::TEMP0, src_op)? as u32;
                let rd = self.def_vreg(output, inst_idx, 0, Self::TEMP0)? as u32;
                self.push_u32(encode_sub(rd, 0, rs), src_op);
                self.store_def_vreg(output, inst_idx, 0, Self::TEMP0, src_op)?;
            }
            VInst::Mul32 { .. } => {
                if Self::is_dead_def(output, inst_idx, 0) {
                    return Ok(());
                }
                let rs1 = self.use_vreg(output, inst_idx, 1, Self::TEMP0, src_op)? as u32;
                let rs2 = self.use_vreg(output, inst_idx, 2, Self::TEMP1, src_op)? as u32;
                let rd = self.def_vreg(output, inst_idx, 0, Self::TEMP0)? as u32;
                self.push_u32(encode_mul(rd, rs1, rs2), src_op);
                self.store_def_vreg(output, inst_idx, 0, Self::TEMP0, src_op)?;
            }
            VInst::And32 { .. } => {
                if Self::is_dead_def(output, inst_idx, 0) {
                    return Ok(());
                }
                let rs1 = self.use_vreg(output, inst_idx, 1, Self::TEMP0, src_op)? as u32;
                let rs2 = self.use_vreg(output, inst_idx, 2, Self::TEMP1, src_op)? as u32;
                let rd = self.def_vreg(output, inst_idx, 0, Self::TEMP0)? as u32;
                self.push_u32(encode_and(rd, rs1, rs2), src_op);
                self.store_def_vreg(output, inst_idx, 0, Self::TEMP0, src_op)?;
            }
            VInst::Or32 { .. } => {
                if Self::is_dead_def(output, inst_idx, 0) {
                    return Ok(());
                }
                let rs1 = self.use_vreg(output, inst_idx, 1, Self::TEMP0, src_op)? as u32;
                let rs2 = self.use_vreg(output, inst_idx, 2, Self::TEMP1, src_op)? as u32;
                let rd = self.def_vreg(output, inst_idx, 0, Self::TEMP0)? as u32;
                self.push_u32(encode_or(rd, rs1, rs2), src_op);
                self.store_def_vreg(output, inst_idx, 0, Self::TEMP0, src_op)?;
            }
            VInst::Xor32 { .. } => {
                if Self::is_dead_def(output, inst_idx, 0) {
                    return Ok(());
                }
                let rs1 = self.use_vreg(output, inst_idx, 1, Self::TEMP0, src_op)? as u32;
                let rs2 = self.use_vreg(output, inst_idx, 2, Self::TEMP1, src_op)? as u32;
                let rd = self.def_vreg(output, inst_idx, 0, Self::TEMP0)? as u32;
                self.push_u32(encode_xor(rd, rs1, rs2), src_op);
                self.store_def_vreg(output, inst_idx, 0, Self::TEMP0, src_op)?;
            }
            VInst::Bnot32 { .. } => {
                if Self::is_dead_def(output, inst_idx, 0) {
                    return Ok(());
                }
                let rs = self.use_vreg(output, inst_idx, 1, Self::TEMP0, src_op)? as u32;
                let rd = self.def_vreg(output, inst_idx, 0, Self::TEMP0)? as u32;
                self.push_u32(encode_xori(rd, rs, -1), src_op);
                self.store_def_vreg(output, inst_idx, 0, Self::TEMP0, src_op)?;
            }
            VInst::Shl32 { .. } => {
                if Self::is_dead_def(output, inst_idx, 0) {
                    return Ok(());
                }
                let rs1 = self.use_vreg(output, inst_idx, 1, Self::TEMP0, src_op)? as u32;
                let rs2 = self.use_vreg(output, inst_idx, 2, Self::TEMP1, src_op)? as u32;
                let rd = self.def_vreg(output, inst_idx, 0, Self::TEMP0)? as u32;
                self.push_u32(encode_sll(rd, rs1, rs2), src_op);
                self.store_def_vreg(output, inst_idx, 0, Self::TEMP0, src_op)?;
            }
            VInst::ShrS32 { .. } => {
                if Self::is_dead_def(output, inst_idx, 0) {
                    return Ok(());
                }
                let rs1 = self.use_vreg(output, inst_idx, 1, Self::TEMP0, src_op)? as u32;
                let rs2 = self.use_vreg(output, inst_idx, 2, Self::TEMP1, src_op)? as u32;
                let rd = self.def_vreg(output, inst_idx, 0, Self::TEMP0)? as u32;
                self.push_u32(encode_sra(rd, rs1, rs2), src_op);
                self.store_def_vreg(output, inst_idx, 0, Self::TEMP0, src_op)?;
            }
            VInst::ShrU32 { .. } => {
                if Self::is_dead_def(output, inst_idx, 0) {
                    return Ok(());
                }
                let rs1 = self.use_vreg(output, inst_idx, 1, Self::TEMP0, src_op)? as u32;
                let rs2 = self.use_vreg(output, inst_idx, 2, Self::TEMP1, src_op)? as u32;
                let rd = self.def_vreg(output, inst_idx, 0, Self::TEMP0)? as u32;
                self.push_u32(encode_srl(rd, rs1, rs2), src_op);
                self.store_def_vreg(output, inst_idx, 0, Self::TEMP0, src_op)?;
            }
            VInst::DivS32 { .. } => {
                if Self::is_dead_def(output, inst_idx, 0) {
                    return Ok(());
                }
                let rs1 = self.use_vreg(output, inst_idx, 1, Self::TEMP0, src_op)? as u32;
                let rs2 = self.use_vreg(output, inst_idx, 2, Self::TEMP1, src_op)? as u32;
                let rd = self.def_vreg(output, inst_idx, 0, Self::TEMP0)? as u32;
                self.push_u32(encode_div(rd, rs1, rs2), src_op);
                self.store_def_vreg(output, inst_idx, 0, Self::TEMP0, src_op)?;
            }
            VInst::DivU32 { .. } => {
                if Self::is_dead_def(output, inst_idx, 0) {
                    return Ok(());
                }
                let rs1 = self.use_vreg(output, inst_idx, 1, Self::TEMP0, src_op)? as u32;
                let rs2 = self.use_vreg(output, inst_idx, 2, Self::TEMP1, src_op)? as u32;
                let rd = self.def_vreg(output, inst_idx, 0, Self::TEMP0)? as u32;
                self.push_u32(encode_divu(rd, rs1, rs2), src_op);
                self.store_def_vreg(output, inst_idx, 0, Self::TEMP0, src_op)?;
            }
            VInst::RemS32 { .. } => {
                if Self::is_dead_def(output, inst_idx, 0) {
                    return Ok(());
                }
                let rs1 = self.use_vreg(output, inst_idx, 1, Self::TEMP0, src_op)? as u32;
                let rs2 = self.use_vreg(output, inst_idx, 2, Self::TEMP1, src_op)? as u32;
                let rd = self.def_vreg(output, inst_idx, 0, Self::TEMP0)? as u32;
                self.push_u32(encode_rem(rd, rs1, rs2), src_op);
                self.store_def_vreg(output, inst_idx, 0, Self::TEMP0, src_op)?;
            }
            VInst::RemU32 { .. } => {
                if Self::is_dead_def(output, inst_idx, 0) {
                    return Ok(());
                }
                let rs1 = self.use_vreg(output, inst_idx, 1, Self::TEMP0, src_op)? as u32;
                let rs2 = self.use_vreg(output, inst_idx, 2, Self::TEMP1, src_op)? as u32;
                let rd = self.def_vreg(output, inst_idx, 0, Self::TEMP0)? as u32;
                self.push_u32(encode_remu(rd, rs1, rs2), src_op);
                self.store_def_vreg(output, inst_idx, 0, Self::TEMP0, src_op)?;
            }
            VInst::Icmp32 { cond, .. } => {
                self.emit_icmp(output, inst_idx, *cond, src_op)?;
            }
            VInst::IeqImm32 { imm, .. } => {
                self.emit_ieq_imm(output, inst_idx, *imm, src_op)?;
            }
            VInst::Select32 { .. } => {
                self.emit_select32(output, inst_idx, src_op)?;
            }
            VInst::Br { target, .. } => {
                let instr_off = self.code.len();
                if let Some(tgt) = self.label_offset_get(*target) {
                    let imm = tgt as i32 - instr_off as i32;
                    if !jal_offset_valid(imm) {
                        return Err(crate::emit_err!());
                    }
                    self.push_u32(encode_jal(0, imm), src_op);
                } else {
                    self.push_u32(0, src_op);
                    self.jal_fixups.push(JalFixup {
                        instr_offset: instr_off,
                        target: *target,
                        rd: 0,
                    });
                }
            }
            VInst::BrIf {
                cond: _,
                target,
                invert,
                ..
            } => {
                let rs1 = self.use_vreg(output, inst_idx, 0, Self::TEMP0, src_op)? as u32;
                let instr_off = self.code.len();
                if let Some(tgt) = self.label_offset_get(*target) {
                    let imm = tgt as i32 - instr_off as i32;
                    if !branch_offset_valid(imm) {
                        return Err(crate::emit_err!());
                    }
                    let w = if *invert {
                        encode_beq(rs1, 0, imm)
                    } else {
                        encode_bne(rs1, 0, imm)
                    };
                    self.push_u32(w, src_op);
                } else {
                    self.push_u32(0, src_op);
                    self.branch_fixups.push(BranchFixup {
                        instr_offset: instr_off,
                        target: *target,
                        rs1,
                        rs2: 0,
                        is_beq: *invert,
                    });
                }
            }
            VInst::Mov32 { .. } => {
                if Self::is_dead_def(output, inst_idx, 0) {
                    return Ok(());
                }
                let rs = self.use_vreg(output, inst_idx, 1, Self::TEMP0, src_op)? as u32;
                let def_alloc = Self::operand_alloc(output, inst_idx, 0);
                if let Alloc::Stack(slot) = def_alloc {
                    // Store source register directly to spill slot,
                    // avoiding the addi-to-temp + sw roundtrip.
                    let offset = self
                        .frame
                        .spill_offset_from_fp(slot as u32)
                        .ok_or(crate::emit_err!())?;
                    self.push_u32(encode_sw(rs, FP_REG as u32, offset), src_op);
                } else {
                    let rd = self.def_vreg(output, inst_idx, 0, Self::TEMP0)? as u32;
                    if rd != rs {
                        self.push_u32(encode_addi(rd, rs, 0), src_op);
                    }
                }
            }
            VInst::Load32 { offset, .. } => {
                if Self::is_dead_def(output, inst_idx, 0) {
                    return Ok(());
                }
                let rs1 = self.use_vreg(output, inst_idx, 1, Self::TEMP1, src_op)? as u32;
                let rd = self.def_vreg(output, inst_idx, 0, Self::TEMP0)? as u32;
                self.push_u32(encode_lw(rd, rs1, *offset), src_op);
                self.store_def_vreg(output, inst_idx, 0, Self::TEMP0, src_op)?;
            }
            VInst::Store32 { offset, .. } => {
                let rs2 = self.use_vreg(output, inst_idx, 0, Self::TEMP0, src_op)? as u32;
                let rs1 = self.use_vreg(output, inst_idx, 1, Self::TEMP1, src_op)? as u32;
                self.push_u32(encode_sw(rs2, rs1, *offset), src_op);
            }
            VInst::IConst32 { val, .. } => {
                if Self::is_dead_def(output, inst_idx, 0) {
                    return Ok(());
                }
                let rd = self.def_vreg(output, inst_idx, 0, Self::TEMP0)? as u32;
                for w in iconst32_sequence(rd, *val) {
                    self.push_u32(w, src_op);
                }
                self.store_def_vreg(output, inst_idx, 0, Self::TEMP0, src_op)?;
            }
            VInst::SlotAddr { slot, .. } => {
                if Self::is_dead_def(output, inst_idx, 0) {
                    return Ok(());
                }
                let off = self
                    .frame
                    .lpir_offset_from_sp(*slot)
                    .ok_or(crate::emit_err!(
                        "lpir slot {} not in frame layout (have: {:?})",
                        slot,
                        self.frame.lpir_slot_offsets
                    ))?;
                let sp_reg = SP_REG as u32;
                let rd = self.def_vreg(output, inst_idx, 0, Self::TEMP0)? as u32;
                if (-2048..2048).contains(&off) {
                    self.push_u32(encode_addi(rd, sp_reg, off), src_op);
                } else {
                    let t_off = Self::TEMP1 as u32;
                    let t_alt = Self::TEMP2 as u32;
                    let scratch = if rd == t_off { t_alt } else { t_off };
                    for w in iconst32_sequence(scratch, off) {
                        self.push_u32(w, src_op);
                    }
                    self.push_u32(encode_add(rd, sp_reg, scratch), src_op);
                }
                self.store_def_vreg(output, inst_idx, 0, Self::TEMP0, src_op)?;
            }
            VInst::MemcpyWords { size, .. } => {
                let t_data = Self::TEMP0 as u32;
                let p_src = Self::TEMP1 as u32;
                let p_dst = Self::TEMP2 as u32;
                let r_src = self.use_vreg(output, inst_idx, 1, Self::TEMP1, src_op)? as u32;
                let r_dst = self.use_vreg(output, inst_idx, 0, Self::TEMP2, src_op)? as u32;
                if r_src != p_src {
                    self.push_u32(encode_addi(p_src, r_src, 0), src_op);
                }
                if r_dst != p_dst {
                    self.push_u32(encode_addi(p_dst, r_dst, 0), src_op);
                }
                let mut remaining = *size as i32;
                while remaining > 0 {
                    let mut local_off = 0i32;
                    while local_off + 4 <= remaining && local_off <= 2047 - 3 {
                        self.push_u32(encode_lw(t_data, p_src, local_off), src_op);
                        self.push_u32(encode_sw(t_data, p_dst, local_off), src_op);
                        local_off += 4;
                    }
                    if local_off == 0 {
                        return Err(crate::emit_err!());
                    }
                    if local_off < remaining {
                        self.push_u32(encode_addi(p_src, p_src, local_off), src_op);
                        self.push_u32(encode_addi(p_dst, p_dst, local_off), src_op);
                    }
                    remaining -= local_off;
                }
            }
            VInst::Call {
                target,
                args,
                rets,
                callee_uses_sret,
                ..
            } => {
                let cap = if *callee_uses_sret {
                    ARG_REGS.len() - 1
                } else {
                    ARG_REGS.len()
                };

                // Store overflow args to the outgoing stack area at [SP + offset].
                for i in cap..args.len() {
                    let operand_idx = rets.len() + i;
                    let alloc = Self::operand_alloc(output, inst_idx, operand_idx);
                    let stack_off = ((i - cap) * 4) as i32;
                    match alloc {
                        Alloc::Reg(src) => {
                            self.push_u32(encode_sw(src as u32, SP_REG as u32, stack_off), src_op);
                        }
                        Alloc::Stack(slot) => {
                            let spill_off = self
                                .frame
                                .spill_offset_from_fp(slot as u32)
                                .ok_or(crate::emit_err!())?;
                            let t = Self::TEMP0 as u32;
                            self.push_u32(encode_lw(t, FP_REG as u32, spill_off), src_op);
                            self.push_u32(encode_sw(t, SP_REG as u32, stack_off), src_op);
                        }
                        Alloc::None => {}
                    }
                }

                if *callee_uses_sret {
                    let sret_off = self
                        .frame
                        .sret_slot_offset_from_fp()
                        .ok_or(crate::emit_err!())?;
                    self.push_u32(
                        encode_addi(ARG_REGS[0] as u32, FP_REG as u32, sret_off),
                        src_op,
                    );
                }

                let auipc_off = self.code.len();
                let ra = RA_REG as u32;
                self.push_u32(encode_auipc(ra, 0), src_op);
                self.push_u32(encode_jalr(ra, ra, 0), src_op);
                self.relocs.push(NativeReloc {
                    offset: auipc_off,
                    symbol: String::from(self.symbols.name(*target)),
                });

                if *callee_uses_sret {
                    let sret_off = self
                        .frame
                        .sret_slot_offset_from_fp()
                        .ok_or(crate::emit_err!())?;
                    for i in 0..rets.len() {
                        let alloc = Self::operand_alloc(output, inst_idx, i);
                        let buf_off = sret_off + (i as i32) * 4;
                        match alloc {
                            Alloc::Reg(dst) => {
                                self.push_u32(
                                    encode_lw(dst as u32, FP_REG as u32, buf_off),
                                    src_op,
                                );
                            }
                            Alloc::Stack(slot) => {
                                let spill_off = self
                                    .frame
                                    .spill_offset_from_fp(slot as u32)
                                    .ok_or(crate::emit_err!())?;
                                let t = Self::TEMP0 as u32;
                                self.push_u32(encode_lw(t, FP_REG as u32, buf_off), src_op);
                                self.push_u32(encode_sw(t, FP_REG as u32, spill_off), src_op);
                            }
                            Alloc::None => {}
                        }
                    }
                }
            }
            VInst::Ret { vals, .. } => {
                let n = vals.len();
                let base_reg = S1_REG as u32;
                for i in 0..n {
                    let src = self.use_vreg(output, inst_idx, i, Self::TEMP0, src_op)? as u32;
                    if is_sret {
                        let offset = (i * 4) as i32;
                        self.push_u32(encode_sw(src, base_reg, offset), src_op);
                    } else if i < RET_REGS.len() {
                        let dst = RET_REGS[i] as u32;
                        if src != dst {
                            self.push_u32(encode_addi(dst, src, 0), src_op);
                        }
                    } else {
                        return Err(crate::emit_err!());
                    }
                }
            }
            VInst::Label(id, _) => {
                self.record_label(*id)?;
            }
        }
        Ok(())
    }

    /// Patch [`BranchFixup`] / [`JalFixup`] placeholders after all labels are known.
    fn resolve_branch_fixups(&mut self) -> Result<(), AllocError> {
        for f in &self.branch_fixups {
            let target = self
                .label_offsets
                .get(f.target as usize)
                .copied()
                .flatten()
                .ok_or(crate::emit_err!())?;
            let pc_rel = target as i32 - f.instr_offset as i32;
            if !branch_offset_valid(pc_rel) {
                return Err(crate::emit_err!());
            }
            let w = if f.is_beq {
                encode_beq(f.rs1, f.rs2, pc_rel)
            } else {
                encode_bne(f.rs1, f.rs2, pc_rel)
            };
            self.code[f.instr_offset..f.instr_offset + 4].copy_from_slice(&w.to_le_bytes());
        }
        for f in &self.jal_fixups {
            let target = self
                .label_offsets
                .get(f.target as usize)
                .copied()
                .flatten()
                .ok_or(crate::emit_err!())?;
            let pc_rel = target as i32 - f.instr_offset as i32;
            if !jal_offset_valid(pc_rel) {
                return Err(crate::emit_err!());
            }
            let w = encode_jal(f.rd, pc_rel);
            self.code[f.instr_offset..f.instr_offset + 4].copy_from_slice(&w.to_le_bytes());
        }
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
    log::debug!(
        "[native-fa] emit_function: starting with {} vinsts, {} edits",
        vinsts.len(),
        output.edits.len()
    );
    let mut ctx = EmitContext::new(frame, symbols, vreg_pool, vinsts.len());

    log::debug!("[native-fa] emit_function: emitting prologue...");
    ctx.emit_prologue(is_sret)?;

    let mut edit_cursor = 0usize;

    if vinsts.is_empty() {
        log::debug!(
            "[native-fa] emit_function: empty vinsts, processing {} edits",
            output.edits.len()
        );
        while edit_cursor < output.edits.len() {
            let (point, edit) = &output.edits[edit_cursor];
            if *point != EditPoint::Before(0) {
                break;
            }
            ctx.emit_edit(edit, None)?;
            edit_cursor += 1;
        }
    } else {
        log::debug!(
            "[native-fa] emit_function: emitting {} vinsts...",
            vinsts.len()
        );
        for (inst_idx, vinst) in vinsts.iter().enumerate() {
            if inst_idx % 100 == 0 {
                log::debug!(
                    "[native-fa] emit_function: vinst {}/{}",
                    inst_idx,
                    vinsts.len()
                );
            }
            let src_op = vinst.src_op();
            while edit_cursor < output.edits.len() {
                let (point, edit) = &output.edits[edit_cursor];
                if *point != EditPoint::Before(inst_idx as u16) {
                    break;
                }
                ctx.emit_edit(edit, src_op)?;
                edit_cursor += 1;
            }
            ctx.emit_vinst(vinst, output, inst_idx, is_sret)?;
            while edit_cursor < output.edits.len() {
                let (point, edit) = &output.edits[edit_cursor];
                if *point != EditPoint::After(inst_idx as u16) {
                    break;
                }
                ctx.emit_edit(edit, src_op)?;
                edit_cursor += 1;
            }
        }
    }

    if edit_cursor != output.edits.len() {
        return Err(crate::emit_err!());
    }

    log::debug!("[native-fa] emit_function: emitting epilogue and finish...");
    ctx.emit_epilogue();

    let result = ctx.finish()?;
    log::debug!(
        "[native-fa] emit_function: complete, {} bytes emitted",
        result.code.len()
    );
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::debug::vinst;
    use crate::rv32::abi;
    use lps_shared::{LpsFnSig, LpsType};

    #[test]
    fn emit_iconst_ret_non_empty_machine_code() {
        let input = "i0 = IConst32 42\nRet i0";
        let (vinsts, symbols, pool) = vinst::parse(input).unwrap();
        let mut lowered = crate::lower::LoweredFunction {
            vinsts,
            vreg_pool: pool,
            symbols,
            loop_regions: Vec::new(),
            region_tree: crate::region::RegionTree::new(),
            lpir_slots: Vec::new(),
        };
        let root = lowered.region_tree.push(crate::region::Region::Linear {
            start: 0,
            end: lowered.vinsts.len() as u16,
        });
        lowered.region_tree.root = root;

        let abi = abi::func_abi_rv32(
            &LpsFnSig {
                name: alloc::string::String::from("t"),
                return_type: LpsType::Int,
                parameters: vec![],
            },
            0,
        );

        let result = crate::emit::emit_lowered(&lowered, &abi).expect("emit_lowered");
        // With frame pointer omission for leaf functions, simple IConst+Ret
        // compiles to ~5 instructions (20 bytes). Ensure we got valid code.
        assert!(
            result.code.len() >= 12,
            "expected at least 3 instructions, got {} bytes",
            result.code.len()
        );
    }
}
