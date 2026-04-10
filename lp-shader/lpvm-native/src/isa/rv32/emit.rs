//! RV32 emission: machine code, relocations, ELF object (`object` crate).

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use object::write::{Object, Relocation, StandardSection, Symbol, SymbolId, SymbolSection};
use object::{
    Architecture, BinaryFormat, Endianness, FileFlags, SymbolFlags, SymbolKind, SymbolScope, elf,
};

use super::abi::{self, A0, ARG_REGS, RET_REGS, S0, S1, SP, callee_saved_int, func_abi_rv32};
use super::inst::{
    encode_add, encode_addi, encode_and, encode_auipc, encode_beq, encode_bne, encode_div,
    encode_divu, encode_jal, encode_jalr, encode_lw, encode_mul, encode_or, encode_rem,
    encode_remu, encode_ret, encode_sll, encode_slt, encode_sltiu, encode_sltu, encode_sra,
    encode_srl, encode_sub, encode_sw, encode_xor, encode_xori, iconst32_sequence,
};
use crate::abi::{FrameLayout, FuncAbi, ModuleAbi, PReg, PregSet};
use crate::config::{REG_ALLOC_ALGORITHM, RegAllocAlgorithm};
use crate::error::{LowerError, NativeError};
use crate::regalloc::{
    Allocation, AllocationAdapter, CallSaveEditParams, Edit, EditPos, FastAllocation,
    FastAllocator, GreedyAlloc, LinearScan, Location, OperandHome, PhysReg,
};
use crate::vinst::SymbolRef;
use crate::vinst::{IcmpCond, LabelId, VInst};
use lpir::VReg;
use lps_shared;

/// Byte offset in `.text` where a relocation applies (at the `auipc` of an auipc+jalr pair).
#[derive(Clone, Debug)]
pub struct NativeReloc {
    pub offset: usize,
    pub symbol: String,
}

/// Machine code for one function plus relocations and optional debug line map.
#[derive(Debug)]
pub struct EmittedFunction {
    pub code: Vec<u8>,
    pub relocs: Vec<NativeReloc>,
    /// When [`EmitContext`] was built with `debug_info`, maps each instruction's byte offset to an LPIR op index.
    pub debug_lines: Vec<(u32, Option<u32>)>,
}

/// Stack slots used around [`VInst::Call`] to preserve caller state across the RV32 ABI clobber set.
#[derive(Clone, Debug)]
pub(crate) struct CallSaveLayout {
    /// First spill slot index reserved for per-call register saves (after regalloc spills).
    slot_base: u32,
    max_per_call: u32,
    /// Bitset of clobbered registers (bit i set = register i is clobbered by call).
    clobber_hw: u32,
    /// When the caller returns via sret, `s1` holds the outer sret pointer; nested sret callees
    /// overwrite `s1`, so we spill it here before any call and reload after.
    s1_slot: Option<u32>,
}

#[derive(Debug)]
pub struct EmitContext {
    pub code: Vec<u8>,
    pub relocs: Vec<NativeReloc>,
    pub debug_lines: Vec<(u32, Option<u32>)>,
    frame: FrameLayout,
    debug_info: bool,
    current_src_op: Option<u32>,
    /// `label_offsets[id]` = byte offset of [`VInst::Label`], when recorded.
    label_offsets: Vec<Option<usize>>,
    branch_fixups: Vec<BranchFixup>,
    jal_fixups: Vec<JalFixup>,
    call_save: Option<CallSaveLayout>,
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

#[inline]
fn branch_offset_valid(imm: i32) -> bool {
    imm % 2 == 0 && (-4096..=4094).contains(&imm)
}

#[inline]
fn jal_offset_valid(imm: i32) -> bool {
    imm % 2 == 0 && imm >= -(1 << 20) && imm <= (1 << 20) - 2
}

fn call_clobber_hw(abi: &FuncAbi) -> u32 {
    let mut bits = 0u32;
    for p in abi.call_clobbers().iter() {
        bits |= 1u32 << p.hw;
    }
    bits
}

/// Compute which vregs are live (used) after a given instruction position.
/// This is a quick local scan - we only look ahead until the next Label or Br,
/// which defines a basic block boundary in our structured control flow.
#[allow(dead_code)]
fn compute_live_out(vinsts: &[VInst], pos: usize) -> alloc::collections::BTreeSet<VReg> {
    let mut live = alloc::collections::BTreeSet::new();

    // Scan forward from the next instruction
    for i in (pos + 1)..vinsts.len() {
        let inst = &vinsts[i];

        // Control flow boundaries stop the scan
        match inst {
            VInst::Label { .. } | VInst::Br { .. } | VInst::BrIf { .. } => break,
            _ => {}
        }

        // All uses are live (same-block scan until control flow; no kill tracking).
        for v in inst.uses() {
            live.insert(v);
        }
    }

    live
}

fn regs_saved_for_call(
    alloc: &Allocation,
    rets: &[VReg],
    clobber: u32,
    _vinsts: &[VInst],
    _call_pos: usize,
) -> Vec<(VReg, PhysReg)> {
    // TODO: use liveness analysis to only save registers whose vregs are live after the call.
    // For now, we save all clobbered registers that have vregs assigned (conservative but safe).
    let mut seen = 0u32;
    let mut out = Vec::new();
    for (vi, po) in alloc.vreg_to_phys.iter().enumerate() {
        let Some(p) = po else {
            continue;
        };
        let v = VReg(vi as u32);
        if alloc.is_spilled(v) {
            continue;
        }
        if rets.contains(&v) {
            continue;
        }
        if clobber & (1u32 << *p) == 0 {
            continue;
        }

        let bit = 1u32 << *p;
        if seen & bit == 0 {
            seen |= bit;
            out.push((v, *p));
        }
    }
    out.sort_by_key(|(v, _)| v.0);
    out
}

fn max_regs_saved_across_calls(vinsts: &[VInst], alloc: &Allocation, clobber: u32) -> u32 {
    let mut m = 0u32;
    for (pos, inst) in vinsts.iter().enumerate() {
        if let VInst::Call { rets, .. } = inst {
            m = m.max(
                regs_saved_for_call(alloc, rets.as_slice(), clobber, vinsts, pos).len() as u32,
            );
        }
    }
    m
}

/// Max bytes required at `SP+0` for stack-passed arguments across all calls in this function.
fn max_caller_outgoing_stack_bytes(vinsts: &[VInst]) -> u32 {
    let mut max_b = 0u32;
    for inst in vinsts {
        if let VInst::Call {
            args,
            callee_uses_sret,
            ..
        } = inst
        {
            let cap = if *callee_uses_sret {
                ARG_REGS.len() - 1
            } else {
                ARG_REGS.len()
            };
            let n_stack = args.len().saturating_sub(cap);
            max_b = max_b.max((n_stack as u32).saturating_mul(4));
        }
    }
    max_b
}

fn phys_homes_of_non_spilled(alloc: &Allocation, vregs: &[VReg]) -> u32 {
    let mut out = 0u32;
    for v in vregs {
        if !alloc.is_spilled(*v) {
            if let Some(p) = alloc.vreg_to_phys.get(v.0 as usize).copied().flatten() {
                out |= 1u32 << p;
            }
        }
    }
    out
}

/// Operand resolution for [`EmitContext::emit_vinst`] (legacy [`Allocation`] vs [`FastAllocation`]).
pub(crate) trait OperandSource {
    fn use_vreg_os(
        &mut self,
        ctx: &mut EmitContext,
        v: VReg,
        temp: PhysReg,
    ) -> Result<PhysReg, NativeError>;

    fn def_vreg_os(
        &mut self,
        ctx: &mut EmitContext,
        v: VReg,
        temp: PhysReg,
    ) -> Result<PhysReg, NativeError>;

    fn store_def_vreg_os(&mut self, ctx: &mut EmitContext, v: VReg, temp: PhysReg);
}

pub(crate) struct LegacyOperandSource<'a> {
    pub alloc: &'a Allocation,
}

impl OperandSource for LegacyOperandSource<'_> {
    fn use_vreg_os(
        &mut self,
        ctx: &mut EmitContext,
        v: VReg,
        temp: PhysReg,
    ) -> Result<PhysReg, NativeError> {
        ctx.use_vreg(self.alloc, v, temp)
    }

    fn def_vreg_os(
        &mut self,
        ctx: &mut EmitContext,
        v: VReg,
        temp: PhysReg,
    ) -> Result<PhysReg, NativeError> {
        ctx.def_vreg(self.alloc, v, temp)
    }

    fn store_def_vreg_os(&mut self, ctx: &mut EmitContext, v: VReg, temp: PhysReg) {
        ctx.store_def_vreg(self.alloc, v, temp);
    }
}

pub(crate) struct FastOperandSource<'a> {
    pub fast: &'a FastAllocation,
    pub idx: usize,
    last_def: Option<OperandHome>,
}

impl OperandSource for FastOperandSource<'_> {
    fn use_vreg_os(
        &mut self,
        ctx: &mut EmitContext,
        v: VReg,
        temp: PhysReg,
    ) -> Result<PhysReg, NativeError> {
        let h = *self
            .fast
            .operand_homes
            .get(self.idx)
            .ok_or_else(|| NativeError::UnassignedVReg(v.0))?;
        self.idx += 1;
        self.last_def = None;
        match h {
            OperandHome::Reg(p) => Ok(p),
            OperandHome::Spill(s) => Ok(ctx.load_spill(s, temp)),
            OperandHome::Remat(k) => {
                let rd = temp as u32;
                for w in iconst32_sequence(rd, k) {
                    ctx.push_u32(w);
                }
                Ok(temp)
            }
        }
    }

    fn def_vreg_os(
        &mut self,
        _ctx: &mut EmitContext,
        v: VReg,
        temp: PhysReg,
    ) -> Result<PhysReg, NativeError> {
        let h = *self
            .fast
            .operand_homes
            .get(self.idx)
            .ok_or_else(|| NativeError::UnassignedVReg(v.0))?;
        self.idx += 1;
        self.last_def = Some(h);
        match h {
            OperandHome::Reg(p) => Ok(p),
            OperandHome::Spill(_) | OperandHome::Remat(_) => Ok(temp),
        }
    }

    fn store_def_vreg_os(&mut self, ctx: &mut EmitContext, _v: VReg, temp: PhysReg) {
        if let Some(OperandHome::Spill(s)) = self.last_def.take() {
            ctx.store_spill(s, temp);
        }
    }
}

impl EmitContext {
    /// Create a new emit context with the given frame layout.
    fn with_frame(frame: FrameLayout, debug_info: bool, call_save: Option<CallSaveLayout>) -> Self {
        Self {
            code: Vec::new(),
            relocs: Vec::new(),
            debug_lines: Vec::new(),
            frame,
            debug_info,
            current_src_op: None,
            label_offsets: Vec::new(),
            branch_fixups: Vec::new(),
            jal_fixups: Vec::new(),
            call_save,
        }
    }

    /// Create a new emit context for a leaf function.
    pub fn new(is_leaf: bool, debug_info: bool) -> Self {
        // Build minimal FuncAbi for leaf frame computation
        let sig = lps_shared::LpsFnSig {
            name: String::from("__leaf"),
            return_type: lps_shared::LpsType::Void,
            parameters: vec![],
        };
        let func_abi = func_abi_rv32(&sig, 1);
        let frame = FrameLayout::compute(&func_abi, 0, PregSet::EMPTY, &[], is_leaf, 0, 0);
        Self::with_frame(frame, debug_info, None)
    }

    fn push_u32(&mut self, w: u32) {
        let offset = self.code.len() as u32;
        self.code.extend_from_slice(&w.to_le_bytes());
        if self.debug_info && self.current_src_op.is_some() {
            self.debug_lines.push((offset, self.current_src_op));
        }
    }

    /// Emit function prologue: adjust sp, save ra/s0/callee-saved regs.
    /// For sret functions, also saves the sret pointer (in a0) to s1.
    pub fn emit_prologue(&mut self, is_sret: bool, alloc: &Allocation) -> Result<(), NativeError> {
        let sp = SP.hw as u32;
        let frame_size = self.frame.total_size as i32;

        self.push_u32(encode_addi(sp, sp, -frame_size));

        if let Some(fp_off) = self.frame.fp_offset_from_sp {
            self.push_u32(encode_sw(S0.hw as u32, sp, fp_off));
        }

        if let Some(ra_off) = self.frame.ra_offset_from_sp {
            self.push_u32(encode_sw(abi::RA.hw as u32, sp, ra_off));
        }

        let callee_saves = self.frame.callee_save_offsets.clone();
        for &(preg, off) in &callee_saves {
            self.push_u32(encode_sw(preg.hw as u32, sp, off));
        }

        if self.frame.save_fp {
            self.push_u32(encode_addi(S0.hw as u32, sp, frame_size));
        }

        for &(v, off) in &alloc.incoming_stack_params {
            let rd = Self::phys(alloc, v)? as u32;
            self.push_u32(encode_lw(rd, S0.hw as u32, off));
        }

        if is_sret {
            self.push_u32(encode_addi(S1.hw as u32, A0.hw as u32, 0));
        }
        Ok(())
    }

    /// Emit function epilogue: restore callee-saved regs/ra/s0, adjust sp, return.
    pub fn emit_epilogue(&mut self) {
        let sp = SP.hw as u32;
        let frame_size = self.frame.total_size as i32;

        let callee_saves = self.frame.callee_save_offsets.clone();
        for &(preg, off) in callee_saves.iter().rev() {
            self.push_u32(encode_lw(preg.hw as u32, sp, off));
        }

        if let Some(ra_off) = self.frame.ra_offset_from_sp {
            self.push_u32(encode_lw(abi::RA.hw as u32, sp, ra_off));
        }

        if let Some(fp_off) = self.frame.fp_offset_from_sp {
            self.push_u32(encode_lw(S0.hw as u32, sp, fp_off));
        }

        self.push_u32(encode_addi(sp, sp, frame_size));
        self.push_u32(encode_ret());
    }

    /// Get the physical register for a vreg.
    /// Returns Err if the vreg is not assigned (shouldn't happen after successful regalloc).
    fn phys(alloc: &Allocation, v: VReg) -> Result<PhysReg, NativeError> {
        let i = v.0 as usize;
        alloc
            .vreg_to_phys
            .get(i)
            .copied()
            .flatten()
            .ok_or_else(|| NativeError::UnassignedVReg(v.0))
    }

    /// Temporary registers for spill handling and multi-instruction lowering.
    const TEMP0: PhysReg = 5; // t0
    const TEMP1: PhysReg = 6; // t1
    const TEMP2: PhysReg = 7; // t2

    /// Emit a load from a spill slot into a temporary register.
    /// Returns the temporary register.
    fn load_spill(&mut self, slot_index: u32, temp: PhysReg) -> PhysReg {
        let offset = self.frame.spill_offset_from_fp(slot_index).unwrap_or(-8);
        self.push_u32(encode_lw(temp as u32, S0.hw as u32, offset));
        temp
    }

    /// Emit a store from a temporary register to a spill slot.
    fn store_spill(&mut self, slot_index: u32, temp: PhysReg) {
        let offset = self.frame.spill_offset_from_fp(slot_index).unwrap_or(-8);
        self.push_u32(encode_sw(temp as u32, S0.hw as u32, offset));
    }

    /// Get or load a vreg for use (source operand).
    /// If the vreg is spilled, loads it into the specified temp register.
    /// Otherwise returns the assigned physical register.
    fn use_vreg(
        &mut self,
        alloc: &Allocation,
        v: VReg,
        temp: PhysReg,
    ) -> Result<PhysReg, NativeError> {
        if let Some(imm) = alloc.rematerial_iconst32(v) {
            let rd = temp as u32;
            for w in iconst32_sequence(rd, imm) {
                self.push_u32(w);
            }
            return Ok(temp);
        }
        if let Some(slot_index) = alloc.spill_slot(v) {
            // VReg is spilled - load from stack into temp register
            Ok(self.load_spill(slot_index, temp))
        } else {
            // VReg has a physical register
            Self::phys(alloc, v)
        }
    }

    /// Get or reserve a vreg for definition (destination operand).
    /// If the vreg is spilled, returns the temp register (caller must store after use).
    /// Otherwise returns the assigned physical register.
    fn def_vreg(
        &mut self,
        alloc: &Allocation,
        v: VReg,
        temp: PhysReg,
    ) -> Result<PhysReg, NativeError> {
        if alloc.is_spilled(v) {
            // VReg is spilled - use temp as temporary, caller must store
            Ok(temp)
        } else {
            // VReg has a physical register
            Self::phys(alloc, v)
        }
    }

    /// Store a spilled vreg after it was written to a temporary register.
    /// Call this after `def_vreg` when the vreg was spilled.
    fn store_def_vreg(&mut self, alloc: &Allocation, v: VReg, temp: PhysReg) {
        if let Some(slot_index) = alloc.spill_slot(v) {
            // VReg was spilled - store temp to stack
            self.store_spill(slot_index, temp);
        }
    }

    fn spill_fp_off(&self, slot: u32) -> Result<i32, NativeError> {
        self.frame
            .spill_offset_from_fp(slot)
            .ok_or(NativeError::UnassignedVReg(slot))
    }

    fn emit_call_preserves_before(
        &mut self,
        alloc: &Allocation,
        rets: &[VReg],
        caller_is_sret: bool,
        vinsts: &[VInst],
        pos: usize,
    ) -> Result<(), NativeError> {
        let Some(layout) = self.call_save.clone() else {
            return Ok(());
        };
        if let Some(s1_slot) = layout.s1_slot {
            if caller_is_sret {
                let o = self.spill_fp_off(s1_slot)?;
                self.push_u32(encode_sw(S1.hw as u32, S0.hw as u32, o));
            }
        }
        let list = regs_saved_for_call(alloc, rets, layout.clobber_hw, vinsts, pos);
        if list.len() as u32 > layout.max_per_call {
            return Err(NativeError::TooManyVRegs {
                count: list.len(),
                max: layout.max_per_call as usize,
            });
        }
        for (i, (_, p)) in list.iter().enumerate() {
            let slot = layout.slot_base + i as u32;
            let o = self.spill_fp_off(slot)?;
            self.push_u32(encode_sw(*p as u32, S0.hw as u32, o));
        }
        Ok(())
    }

    fn emit_call_preserves_after(
        &mut self,
        alloc: &Allocation,
        rets: &[VReg],
        caller_is_sret: bool,
        vinsts: &[VInst],
        pos: usize,
    ) -> Result<(), NativeError> {
        let Some(layout) = self.call_save.clone() else {
            return Ok(());
        };
        let list = regs_saved_for_call(alloc, rets, layout.clobber_hw, vinsts, pos);
        let ret_homes = phys_homes_of_non_spilled(alloc, rets);
        for (i, (_, p)) in list.iter().enumerate().rev() {
            if ret_homes & (1u32 << *p) != 0 {
                continue;
            }
            let slot = layout.slot_base + i as u32;
            let o = self.spill_fp_off(slot)?;
            self.push_u32(encode_lw(*p as u32, S0.hw as u32, o));
        }
        if let Some(s1_slot) = layout.s1_slot {
            if caller_is_sret {
                let o = self.spill_fp_off(s1_slot)?;
                self.push_u32(encode_lw(S1.hw as u32, S0.hw as u32, o));
            }
        }
        Ok(())
    }

    /// Direct-return call: args in a0–a7, results in a0–a1.
    fn emit_call_direct<S: OperandSource>(
        &mut self,
        op_src: &mut S,
        preserve_alloc: &Allocation,
        target: &SymbolRef,
        args: &[VReg],
        rets: &[VReg],
        caller_is_sret: bool,
        vinsts: &[VInst],
        pos: usize,
        preserve_embedded: bool,
    ) -> Result<(), NativeError> {
        if preserve_embedded {
            self.emit_call_preserves_before(preserve_alloc, rets, caller_is_sret, vinsts, pos)?;
        }
        let reg_cap = ARG_REGS.len();
        let sp_hw = SP.hw as u32;
        for (i, a) in args.iter().enumerate().take(reg_cap) {
            let from = op_src.use_vreg_os(self, *a, Self::TEMP0)? as u32;
            let to = ARG_REGS[i].hw as u32;
            if from != to {
                self.push_u32(encode_addi(to, from, 0));
            }
        }
        for (i, a) in args.iter().enumerate().skip(reg_cap) {
            let from = op_src.use_vreg_os(self, *a, Self::TEMP0)? as u32;
            let stk_off = ((i - reg_cap) * 4) as i32;
            self.push_u32(encode_sw(from, sp_hw, stk_off));
        }
        let auipc_off = self.code.len();
        let ra = abi::RA.hw as u32;
        self.push_u32(encode_auipc(ra, 0));
        self.push_u32(encode_jalr(ra, ra, 0));
        self.relocs.push(NativeReloc {
            offset: auipc_off,
            symbol: target.name.clone(),
        });
        for (i, r) in rets.iter().enumerate() {
            if i >= RET_REGS.len() {
                return Err(NativeError::TooManyReturns(i + 1));
            }
            let dst = op_src.def_vreg_os(self, *r, Self::TEMP0)? as u32;
            let ret_hw = RET_REGS[i].hw as u32;
            if dst != ret_hw {
                self.push_u32(encode_addi(dst, ret_hw, 0));
            }
            op_src.store_def_vreg_os(self, *r, Self::TEMP0);
        }
        if preserve_embedded {
            self.emit_call_preserves_after(preserve_alloc, rets, caller_is_sret, vinsts, pos)?;
        }
        Ok(())
    }

    /// Callee uses sret: pass buffer address in a0, args in a1–a7, load results from frame slot.
    fn emit_call_sret<S: OperandSource>(
        &mut self,
        op_src: &mut S,
        preserve_alloc: &Allocation,
        target: &SymbolRef,
        args: &[VReg],
        rets: &[VReg],
        caller_is_sret: bool,
        vinsts: &[VInst],
        pos: usize,
        preserve_embedded: bool,
    ) -> Result<(), NativeError> {
        if preserve_embedded {
            self.emit_call_preserves_before(preserve_alloc, rets, caller_is_sret, vinsts, pos)?;
        }
        let sret_off = self
            .frame
            .sret_slot_offset_from_fp()
            .ok_or(NativeError::MissingSretSlot)?;
        let reg_cap = ARG_REGS.len() - 1;
        let a0 = A0.hw as u32;
        let s0 = S0.hw as u32;
        let sp_hw = SP.hw as u32;
        self.push_u32(encode_addi(a0, s0, sret_off));
        for (i, a) in args.iter().enumerate().take(reg_cap) {
            let from = op_src.use_vreg_os(self, *a, Self::TEMP0)? as u32;
            let to = ARG_REGS[i + 1].hw as u32;
            if from != to {
                self.push_u32(encode_addi(to, from, 0));
            }
        }
        for (i, a) in args.iter().enumerate().skip(reg_cap) {
            let from = op_src.use_vreg_os(self, *a, Self::TEMP0)? as u32;
            let stk_off = ((i - reg_cap) * 4) as i32;
            self.push_u32(encode_sw(from, sp_hw, stk_off));
        }
        let auipc_off = self.code.len();
        let ra = abi::RA.hw as u32;
        self.push_u32(encode_auipc(ra, 0));
        self.push_u32(encode_jalr(ra, ra, 0));
        self.relocs.push(NativeReloc {
            offset: auipc_off,
            symbol: target.name.clone(),
        });
        let base_off = sret_off;
        for (i, r) in rets.iter().enumerate() {
            let off = base_off + (i * 4) as i32;
            let rd = op_src.def_vreg_os(self, *r, Self::TEMP0)? as u32;
            self.push_u32(encode_lw(rd, s0, off));
            op_src.store_def_vreg_os(self, *r, Self::TEMP0);
        }
        if preserve_embedded {
            self.emit_call_preserves_after(preserve_alloc, rets, caller_is_sret, vinsts, pos)?;
        }
        Ok(())
    }

    fn ensure_label_slot(&mut self, id: LabelId) {
        let i = id as usize;
        if i >= self.label_offsets.len() {
            self.label_offsets.resize(i + 1, None);
        }
    }

    fn label_offset_get(&self, id: LabelId) -> Option<usize> {
        self.label_offsets.get(id as usize).copied().flatten()
    }

    fn record_label(&mut self, id: LabelId) -> Result<(), NativeError> {
        self.ensure_label_slot(id);
        if self.label_offsets[id as usize].is_some() {
            return Err(NativeError::DuplicateLabel(id));
        }
        self.label_offsets[id as usize] = Some(self.code.len());
        Ok(())
    }

    /// Patch [`BranchFixup`] / [`JalFixup`] placeholders after all labels are known.
    pub fn resolve_branch_fixups(&mut self) -> Result<(), NativeError> {
        for f in &self.branch_fixups {
            let target = self
                .label_offsets
                .get(f.target as usize)
                .copied()
                .flatten()
                .ok_or(NativeError::UnresolvedLabel(f.target))?;
            let pc_rel = target as i32 - f.instr_offset as i32;
            if !branch_offset_valid(pc_rel) {
                return Err(NativeError::BranchOffsetOutOfRange);
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
                .ok_or(NativeError::UnresolvedLabel(f.target))?;
            let pc_rel = target as i32 - f.instr_offset as i32;
            if !jal_offset_valid(pc_rel) {
                return Err(NativeError::BranchOffsetOutOfRange);
            }
            let w = encode_jal(f.rd, pc_rel);
            self.code[f.instr_offset..f.instr_offset + 4].copy_from_slice(&w.to_le_bytes());
        }
        Ok(())
    }

    fn emit_icmp<S: OperandSource>(
        &mut self,
        src: &mut S,
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
        cond: IcmpCond,
    ) -> Result<(), NativeError> {
        let rs_l = src.use_vreg_os(self, lhs, Self::TEMP0)? as u32;
        let rs_r = src.use_vreg_os(self, rhs, Self::TEMP1)? as u32;
        let rd = src.def_vreg_os(self, dst, Self::TEMP0)? as u32;
        let scratch = Self::TEMP2 as u32;

        match cond {
            IcmpCond::LtS => {
                self.push_u32(encode_slt(rd, rs_l, rs_r));
            }
            IcmpCond::LtU => {
                self.push_u32(encode_sltu(rd, rs_l, rs_r));
            }
            IcmpCond::GtS => {
                self.push_u32(encode_slt(rd, rs_r, rs_l));
            }
            IcmpCond::GtU => {
                self.push_u32(encode_sltu(rd, rs_r, rs_l));
            }
            IcmpCond::Eq => {
                self.push_u32(encode_xor(scratch, rs_l, rs_r));
                self.push_u32(encode_sltiu(rd, scratch, 1));
            }
            IcmpCond::Ne => {
                self.push_u32(encode_xor(scratch, rs_l, rs_r));
                self.push_u32(encode_sltu(rd, 0, scratch));
            }
            IcmpCond::LeS => {
                self.push_u32(encode_slt(scratch, rs_r, rs_l));
                self.push_u32(encode_xori(rd, scratch, 1));
            }
            IcmpCond::LeU => {
                self.push_u32(encode_sltu(scratch, rs_r, rs_l));
                self.push_u32(encode_xori(rd, scratch, 1));
            }
            IcmpCond::GeS => {
                self.push_u32(encode_slt(scratch, rs_l, rs_r));
                self.push_u32(encode_xori(rd, scratch, 1));
            }
            IcmpCond::GeU => {
                self.push_u32(encode_sltu(scratch, rs_l, rs_r));
                self.push_u32(encode_xori(rd, scratch, 1));
            }
        }

        src.store_def_vreg_os(self, dst, Self::TEMP0);
        Ok(())
    }

    /// `dst = (src == imm) ? 1 : 0`
    fn emit_ieq_imm<S: OperandSource>(
        &mut self,
        src: &mut S,
        dst: VReg,
        src_v: VReg,
        imm: i32,
    ) -> Result<(), NativeError> {
        let mut rs = src.use_vreg_os(self, src_v, Self::TEMP0)? as u32;
        let rd = src.def_vreg_os(self, dst, Self::TEMP0)? as u32;
        const IMM12: core::ops::RangeInclusive<i32> = -2048_i32..=2047_i32;
        if IMM12.contains(&imm) {
            let scratch = Self::TEMP2 as u32;
            self.push_u32(encode_xori(scratch, rs, imm));
            self.push_u32(encode_sltiu(rd, scratch, 1));
        } else {
            if rs == Self::TEMP1 as u32 {
                self.push_u32(encode_addi(Self::TEMP0 as u32, rs, 0));
                rs = Self::TEMP0 as u32;
            }
            for w in iconst32_sequence(Self::TEMP1 as u32, imm) {
                self.push_u32(w);
            }
            self.push_u32(encode_xor(Self::TEMP2 as u32, rs, Self::TEMP1 as u32));
            self.push_u32(encode_sltiu(rd, Self::TEMP2 as u32, 1));
        }
        src.store_def_vreg_os(self, dst, Self::TEMP0);
        Ok(())
    }

    fn emit_select32<S: OperandSource>(
        &mut self,
        src: &mut S,
        dst: VReg,
        cond: VReg,
        if_true: VReg,
        if_false: VReg,
    ) -> Result<(), NativeError> {
        let p_true = src.use_vreg_os(self, if_true, Self::TEMP0)? as u32;
        let p_false = src.use_vreg_os(self, if_false, Self::TEMP1)? as u32;
        let p_cond = src.use_vreg_os(self, cond, Self::TEMP2)? as u32;

        self.push_u32(encode_sub(Self::TEMP0 as u32, p_true, p_false));
        self.push_u32(encode_and(Self::TEMP0 as u32, Self::TEMP0 as u32, p_cond));

        let rd = src.def_vreg_os(self, dst, Self::TEMP0)? as u32;
        self.push_u32(encode_add(rd, Self::TEMP0 as u32, p_false));
        src.store_def_vreg_os(self, dst, Self::TEMP0);
        Ok(())
    }

    /// Emit explicit [`FastAllocation`] edits before or after instruction `pos`.
    pub(crate) fn emit_fast_edits(
        &mut self,
        fast: &FastAllocation,
        pos: usize,
        before: bool,
    ) -> Result<(), NativeError> {
        for (ep, edit) in &fast.edits {
            let want = match ep {
                EditPos::Before(i) => before && *i == pos,
                EditPos::After(i) => !before && *i == pos,
            };
            if !want {
                continue;
            }
            self.emit_edit(*edit)?;
        }
        Ok(())
    }

    fn emit_edit(&mut self, edit: Edit) -> Result<(), NativeError> {
        match edit {
            Edit::Move { from, to } => match (from, to) {
                (Location::Reg(s), Location::Reg(d)) => {
                    if s != d {
                        self.push_u32(encode_addi(d as u32, s as u32, 0));
                    }
                }
                (Location::Reg(r), Location::Stack(slot)) => {
                    let o = self.spill_fp_off(slot)?;
                    self.push_u32(encode_sw(r as u32, S0.hw as u32, o));
                }
                (Location::Stack(slot), Location::Reg(r)) => {
                    let o = self.spill_fp_off(slot)?;
                    self.push_u32(encode_lw(r as u32, S0.hw as u32, o));
                }
                (Location::Imm(k), Location::Reg(r)) => {
                    for w in iconst32_sequence(r as u32, k) {
                        self.push_u32(w);
                    }
                }
                _ => {
                    return Err(NativeError::Lower(LowerError::UnsupportedOp {
                        description: String::from("emit_edit: unsupported Move"),
                    }));
                }
            },
        }
        Ok(())
    }

    pub(crate) fn emit_vinst<S: OperandSource>(
        &mut self,
        inst: &VInst,
        vinsts: &[VInst],
        pos: usize,
        src: &mut S,
        meta_alloc: &Allocation,
        is_sret: bool,
    ) -> Result<(), NativeError> {
        self.current_src_op = inst.src_op();
        match inst {
            VInst::Add32 {
                dst, src1, src2, ..
            } => {
                // Use TEMP0 for src1, TEMP1 for src2 if spilled
                let rs1 = src.use_vreg_os(self, *src1, Self::TEMP0)? as u32;
                let rs2 = src.use_vreg_os(self, *src2, Self::TEMP1)? as u32;
                // Result can go to TEMP0 if dst is spilled
                let rd = src.def_vreg_os(self, *dst, Self::TEMP0)? as u32;
                self.push_u32(crate::isa::rv32::inst::encode_add(rd, rs1, rs2));
                src.store_def_vreg_os(self, *dst, Self::TEMP0);
            }
            VInst::Sub32 {
                dst, src1, src2, ..
            } => {
                let rs1 = src.use_vreg_os(self, *src1, Self::TEMP0)? as u32;
                let rs2 = src.use_vreg_os(self, *src2, Self::TEMP1)? as u32;
                let rd = src.def_vreg_os(self, *dst, Self::TEMP0)? as u32;
                self.push_u32(encode_sub(rd, rs1, rs2));
                src.store_def_vreg_os(self, *dst, Self::TEMP0);
            }
            VInst::Neg32 {
                dst, src: src_v, ..
            } => {
                let rs = src.use_vreg_os(self, *src_v, Self::TEMP0)? as u32;
                let rd = src.def_vreg_os(self, *dst, Self::TEMP0)? as u32;
                // Emit: sub rd, x0, rs (where x0=0 is the hardware zero register)
                self.push_u32(encode_sub(rd, 0, rs));
                src.store_def_vreg_os(self, *dst, Self::TEMP0);
            }
            VInst::Mul32 {
                dst, src1, src2, ..
            } => {
                let rs1 = src.use_vreg_os(self, *src1, Self::TEMP0)? as u32;
                let rs2 = src.use_vreg_os(self, *src2, Self::TEMP1)? as u32;
                let rd = src.def_vreg_os(self, *dst, Self::TEMP0)? as u32;
                self.push_u32(encode_mul(rd, rs1, rs2));
                src.store_def_vreg_os(self, *dst, Self::TEMP0);
            }
            VInst::And32 {
                dst, src1, src2, ..
            } => {
                let rs1 = src.use_vreg_os(self, *src1, Self::TEMP0)? as u32;
                let rs2 = src.use_vreg_os(self, *src2, Self::TEMP1)? as u32;
                let rd = src.def_vreg_os(self, *dst, Self::TEMP0)? as u32;
                self.push_u32(encode_and(rd, rs1, rs2));
                src.store_def_vreg_os(self, *dst, Self::TEMP0);
            }
            VInst::Or32 {
                dst, src1, src2, ..
            } => {
                let rs1 = src.use_vreg_os(self, *src1, Self::TEMP0)? as u32;
                let rs2 = src.use_vreg_os(self, *src2, Self::TEMP1)? as u32;
                let rd = src.def_vreg_os(self, *dst, Self::TEMP0)? as u32;
                self.push_u32(encode_or(rd, rs1, rs2));
                src.store_def_vreg_os(self, *dst, Self::TEMP0);
            }
            VInst::Xor32 {
                dst, src1, src2, ..
            } => {
                let rs1 = src.use_vreg_os(self, *src1, Self::TEMP0)? as u32;
                let rs2 = src.use_vreg_os(self, *src2, Self::TEMP1)? as u32;
                let rd = src.def_vreg_os(self, *dst, Self::TEMP0)? as u32;
                self.push_u32(encode_xor(rd, rs1, rs2));
                src.store_def_vreg_os(self, *dst, Self::TEMP0);
            }
            VInst::Bnot32 {
                dst, src: src_v, ..
            } => {
                let rs = src.use_vreg_os(self, *src_v, Self::TEMP0)? as u32;
                let rd = src.def_vreg_os(self, *dst, Self::TEMP0)? as u32;
                self.push_u32(encode_xori(rd, rs, -1));
                src.store_def_vreg_os(self, *dst, Self::TEMP0);
            }
            VInst::Shl32 {
                dst, src1, src2, ..
            } => {
                let rs1 = src.use_vreg_os(self, *src1, Self::TEMP0)? as u32;
                let rs2 = src.use_vreg_os(self, *src2, Self::TEMP1)? as u32;
                let rd = src.def_vreg_os(self, *dst, Self::TEMP0)? as u32;
                self.push_u32(encode_sll(rd, rs1, rs2));
                src.store_def_vreg_os(self, *dst, Self::TEMP0);
            }
            VInst::ShrS32 {
                dst, src1, src2, ..
            } => {
                let rs1 = src.use_vreg_os(self, *src1, Self::TEMP0)? as u32;
                let rs2 = src.use_vreg_os(self, *src2, Self::TEMP1)? as u32;
                let rd = src.def_vreg_os(self, *dst, Self::TEMP0)? as u32;
                self.push_u32(encode_sra(rd, rs1, rs2));
                src.store_def_vreg_os(self, *dst, Self::TEMP0);
            }
            VInst::ShrU32 {
                dst, src1, src2, ..
            } => {
                let rs1 = src.use_vreg_os(self, *src1, Self::TEMP0)? as u32;
                let rs2 = src.use_vreg_os(self, *src2, Self::TEMP1)? as u32;
                let rd = src.def_vreg_os(self, *dst, Self::TEMP0)? as u32;
                self.push_u32(encode_srl(rd, rs1, rs2));
                src.store_def_vreg_os(self, *dst, Self::TEMP0);
            }
            VInst::DivS32 { dst, lhs, rhs, .. } => {
                let rs1 = src.use_vreg_os(self, *lhs, Self::TEMP0)? as u32;
                let rs2 = src.use_vreg_os(self, *rhs, Self::TEMP1)? as u32;
                let rd = src.def_vreg_os(self, *dst, Self::TEMP0)? as u32;
                self.push_u32(encode_div(rd, rs1, rs2));
                src.store_def_vreg_os(self, *dst, Self::TEMP0);
            }
            VInst::DivU32 { dst, lhs, rhs, .. } => {
                let rs1 = src.use_vreg_os(self, *lhs, Self::TEMP0)? as u32;
                let rs2 = src.use_vreg_os(self, *rhs, Self::TEMP1)? as u32;
                let rd = src.def_vreg_os(self, *dst, Self::TEMP0)? as u32;
                self.push_u32(encode_divu(rd, rs1, rs2));
                src.store_def_vreg_os(self, *dst, Self::TEMP0);
            }
            VInst::RemS32 { dst, lhs, rhs, .. } => {
                let rs1 = src.use_vreg_os(self, *lhs, Self::TEMP0)? as u32;
                let rs2 = src.use_vreg_os(self, *rhs, Self::TEMP1)? as u32;
                let rd = src.def_vreg_os(self, *dst, Self::TEMP0)? as u32;
                self.push_u32(encode_rem(rd, rs1, rs2));
                src.store_def_vreg_os(self, *dst, Self::TEMP0);
            }
            VInst::RemU32 { dst, lhs, rhs, .. } => {
                let rs1 = src.use_vreg_os(self, *lhs, Self::TEMP0)? as u32;
                let rs2 = src.use_vreg_os(self, *rhs, Self::TEMP1)? as u32;
                let rd = src.def_vreg_os(self, *dst, Self::TEMP0)? as u32;
                self.push_u32(encode_remu(rd, rs1, rs2));
                src.store_def_vreg_os(self, *dst, Self::TEMP0);
            }
            VInst::Icmp32 {
                dst,
                lhs,
                rhs,
                cond,
                ..
            } => {
                self.emit_icmp(src, *dst, *lhs, *rhs, *cond)?;
            }
            VInst::IeqImm32 {
                dst,
                src: src_v,
                imm,
                ..
            } => {
                self.emit_ieq_imm(src, *dst, *src_v, *imm)?;
            }
            VInst::Select32 {
                dst,
                cond,
                if_true,
                if_false,
                ..
            } => {
                self.emit_select32(src, *dst, *cond, *if_true, *if_false)?;
            }
            VInst::Br { target, .. } => {
                let instr_off = self.code.len();
                if let Some(tgt) = self.label_offset_get(*target) {
                    let imm = tgt as i32 - instr_off as i32;
                    if !jal_offset_valid(imm) {
                        return Err(NativeError::BranchOffsetOutOfRange);
                    }
                    self.push_u32(encode_jal(0, imm));
                } else {
                    self.push_u32(0);
                    self.jal_fixups.push(JalFixup {
                        instr_offset: instr_off,
                        target: *target,
                        rd: 0,
                    });
                }
            }
            VInst::BrIf {
                cond,
                target,
                invert,
                ..
            } => {
                let rs1 = src.use_vreg_os(self, *cond, Self::TEMP0)? as u32;
                let instr_off = self.code.len();
                if let Some(tgt) = self.label_offset_get(*target) {
                    let imm = tgt as i32 - instr_off as i32;
                    if !branch_offset_valid(imm) {
                        return Err(NativeError::BranchOffsetOutOfRange);
                    }
                    let w = if *invert {
                        encode_beq(rs1, 0, imm)
                    } else {
                        encode_bne(rs1, 0, imm)
                    };
                    self.push_u32(w);
                } else {
                    self.push_u32(0);
                    self.branch_fixups.push(BranchFixup {
                        instr_offset: instr_off,
                        target: *target,
                        rs1,
                        rs2: 0,
                        is_beq: *invert,
                    });
                }
            }
            VInst::Mov32 {
                dst, src: src_v, ..
            } => {
                let rs = src.use_vreg_os(self, *src_v, Self::TEMP0)? as u32;
                let rd = src.def_vreg_os(self, *dst, Self::TEMP0)? as u32;
                if rd != rs {
                    self.push_u32(encode_addi(rd, rs, 0));
                }
                src.store_def_vreg_os(self, *dst, Self::TEMP0);
            }
            VInst::Load32 {
                dst, base, offset, ..
            } => {
                // base must not use TEMP0 if dst will use TEMP0
                // For simplicity: load base first (into TEMP1), then use TEMP0 for result
                let rs1 = src.use_vreg_os(self, *base, Self::TEMP1)? as u32;
                let rd = src.def_vreg_os(self, *dst, Self::TEMP0)? as u32;
                self.push_u32(encode_lw(rd, rs1, *offset));
                src.store_def_vreg_os(self, *dst, Self::TEMP0);
            }
            VInst::Store32 {
                src: val_v,
                base,
                offset,
                ..
            } => {
                let rs2 = src.use_vreg_os(self, *val_v, Self::TEMP0)? as u32;
                let rs1 = src.use_vreg_os(self, *base, Self::TEMP1)? as u32;
                self.push_u32(encode_sw(rs2, rs1, *offset));
            }
            VInst::IConst32 { dst, val, .. } => {
                if meta_alloc.rematerial_iconst32(*dst).is_some() {
                    // No register/stack home; each use emits `iconst32_sequence`.
                } else {
                    let rd = src.def_vreg_os(self, *dst, Self::TEMP0)? as u32;
                    for w in iconst32_sequence(rd, *val) {
                        self.push_u32(w);
                    }
                    src.store_def_vreg_os(self, *dst, Self::TEMP0);
                }
            }
            VInst::SlotAddr { dst, slot, .. } => {
                let off = self
                    .frame
                    .lpir_offset_from_sp(*slot)
                    .ok_or(NativeError::InvalidLpirSlot(*slot))?;
                let sp_reg = SP.hw as u32;
                let rd = src.def_vreg_os(self, *dst, Self::TEMP0)? as u32;
                if (-2048..2048).contains(&off) {
                    self.push_u32(encode_addi(rd, sp_reg, off));
                } else {
                    let t_off = Self::TEMP1 as u32;
                    let t_alt = Self::TEMP2 as u32;
                    let scratch = if rd == t_off { t_alt } else { t_off };
                    for w in iconst32_sequence(scratch, off) {
                        self.push_u32(w);
                    }
                    self.push_u32(encode_add(rd, sp_reg, scratch));
                }
                src.store_def_vreg_os(self, *dst, Self::TEMP0);
            }
            VInst::MemcpyWords {
                dst_base,
                src_base,
                size,
                ..
            } => {
                let t_data = Self::TEMP0 as u32;
                let p_src = Self::TEMP1 as u32;
                let p_dst = Self::TEMP2 as u32;
                let r_src = src.use_vreg_os(self, *src_base, Self::TEMP1)? as u32;
                let r_dst = src.use_vreg_os(self, *dst_base, Self::TEMP2)? as u32;
                if r_src != p_src {
                    self.push_u32(encode_addi(p_src, r_src, 0));
                }
                if r_dst != p_dst {
                    self.push_u32(encode_addi(p_dst, r_dst, 0));
                }
                let mut remaining = *size as i32;
                while remaining > 0 {
                    let mut local_off = 0i32;
                    while local_off + 4 <= remaining && local_off <= 2047 - 3 {
                        self.push_u32(encode_lw(t_data, p_src, local_off));
                        self.push_u32(encode_sw(t_data, p_dst, local_off));
                        local_off += 4;
                    }
                    if local_off == 0 {
                        return Err(NativeError::Lower(LowerError::UnsupportedOp {
                            description: String::from("internal: memcpy chunk"),
                        }));
                    }
                    if local_off < remaining {
                        self.push_u32(encode_addi(p_src, p_src, local_off));
                        self.push_u32(encode_addi(p_dst, p_dst, local_off));
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
                let preserve_embedded = self.call_save.is_some();
                if *callee_uses_sret {
                    self.emit_call_sret(
                        src,
                        meta_alloc,
                        target,
                        args.as_slice(),
                        rets.as_slice(),
                        is_sret,
                        vinsts,
                        pos,
                        preserve_embedded,
                    )?;
                } else {
                    self.emit_call_direct(
                        src,
                        meta_alloc,
                        target,
                        args.as_slice(),
                        rets.as_slice(),
                        is_sret,
                        vinsts,
                        pos,
                        preserve_embedded,
                    )?;
                }
            }
            VInst::Ret { vals, .. } => {
                if is_sret {
                    // Sret: store values to buffer pointed to by s1
                    // s1 was loaded with the sret buffer address in the prologue
                    // (since a0 may be clobbered during function execution)
                    let base_reg = S1.hw as u32; // s1
                    for (i, v) in vals.iter().enumerate() {
                        let val_reg = src.use_vreg_os(self, *v, Self::TEMP0)? as u32;
                        let offset = (i * 4) as i32;
                        // Store each scalar to s1 + offset
                        self.push_u32(encode_sw(val_reg, base_reg, offset));
                    }
                    // Return value buffer address is already in a0 per ABI
                } else {
                    // Direct return: move values to a0-a1
                    for (i, v) in vals.iter().enumerate() {
                        let val_reg = src.use_vreg_os(self, *v, Self::TEMP0)? as u32;
                        let dst = RET_REGS[i].hw as u32;
                        if val_reg != dst {
                            self.push_u32(encode_addi(dst, val_reg, 0));
                        }
                    }
                }
            }
            VInst::Label(id, _) => {
                self.record_label(*id)?;
            }
        }
        self.current_src_op = None;
        Ok(())
    }
}

fn allocate_for_emit(
    func: &lpir::IrFunction,
    vinsts: &[VInst],
    func_abi: &FuncAbi,
    loop_regions: &[crate::lower::LoopRegion],
    alloc_trace: bool,
) -> Result<Allocation, NativeError> {
    match REG_ALLOC_ALGORITHM {
        RegAllocAlgorithm::LinearScan => LinearScan::new().allocate_with_func_abi(
            func,
            vinsts,
            func_abi,
            loop_regions,
            alloc_trace,
        ),
        RegAllocAlgorithm::Greedy => {
            let _ = alloc_trace;
            GreedyAlloc::new().allocate_with_func_abi(func, vinsts, func_abi)
        }
        RegAllocAlgorithm::Fast => Err(NativeError::FastallocInternal(
            "REG_ALLOC_ALGORITHM::Fast uses FastAllocator::allocate_with_func_abi from emit_function_bytes, not allocate_for_emit",
        )),
    }
}

/// Insert s1 spill/reload around each call (matches [`AllocationAdapter`] ordering) when fastalloc
/// omitted them because `s1_slot` is only known after frame layout.
fn inject_s1_edits_for_fastalloc_calls(
    edits: &mut Vec<(EditPos, Edit)>,
    vinsts: &[VInst],
    s1_slot: u32,
) {
    let s1_hw = S1.hw;
    // Insert from the last call upward so indices stay valid as the vec grows.
    for pos in (0..vinsts.len()).rev() {
        if !matches!(vinsts[pos], VInst::Call { .. }) {
            continue;
        }
        let s1_before = (
            EditPos::Before(pos),
            Edit::Move {
                from: Location::Reg(s1_hw),
                to: Location::Stack(s1_slot),
            },
        );
        let s1_after = (
            EditPos::After(pos),
            Edit::Move {
                from: Location::Stack(s1_slot),
                to: Location::Reg(s1_hw),
            },
        );
        let before_insert = edits
            .iter()
            .position(|(ep, _)| matches!(ep, EditPos::Before(p) if *p == pos))
            .unwrap_or_else(|| {
                edits
                    .iter()
                    .enumerate()
                    .find_map(|(i, (ep, _))| match ep {
                        EditPos::Before(p) if *p > pos => Some(i),
                        _ => None,
                    })
                    .unwrap_or(edits.len())
            });
        let after_insert = edits
            .iter()
            .enumerate()
            .filter(|(_, (ep, _))| matches!(ep, EditPos::After(p) if *p == pos))
            .map(|(i, _)| i)
            .last()
            .map(|i| i + 1)
            .unwrap_or_else(|| {
                edits
                    .iter()
                    .enumerate()
                    .find_map(|(i, (ep, _))| match ep {
                        EditPos::After(p) if *p > pos => Some(i),
                        _ => None,
                    })
                    .unwrap_or(edits.len())
            });
        if after_insert >= before_insert {
            edits.insert(after_insert, s1_after);
            edits.insert(before_insert, s1_before);
        } else {
            edits.insert(before_insert, s1_before);
            edits.insert(after_insert + 1, s1_after);
        }
    }
}

/// Emit one function to RV32 bytes (and relocations). Used by ELF writer and debug assembly.
///
/// # Arguments
/// * `func` - The LPIR function to emit
/// * `ir` - Module containing `func` (for call lowering)
/// * `module_abi` - Pre-computed ABI and max callee sret size
/// * `fn_sig` - Surface signature (ABI classification, must match `func` parameter layout)
/// * `float_mode` - Floating point mode (Q32 or SoftFloat)
/// * `debug_info` - Whether to include debug line information
/// * `alloc_trace` - When true and [`crate::config::REG_ALLOC_ALGORITHM`] is [`RegAllocAlgorithm::LinearScan`], print allocation trace to stderr
pub fn emit_function_bytes(
    func: &lpir::IrFunction,
    ir: &lpir::IrModule,
    module_abi: &ModuleAbi,
    fn_sig: &lps_shared::LpsFnSig,
    float_mode: lpir::FloatMode,
    debug_info: bool,
    alloc_trace: bool,
) -> Result<EmittedFunction, NativeError> {
    let lowered = crate::lower::lower_ops(func, ir, module_abi, float_mode)?;
    let vinsts = &lowered.vinsts;
    let slots = func.total_param_slots() as usize;
    let func_abi = super::abi::func_abi_rv32(fn_sig, slots);
    let (alloc, fast_from_walk) = match REG_ALLOC_ALGORITHM {
        RegAllocAlgorithm::Fast => {
            let (fast, alloc) =
                FastAllocator::new().allocate_with_func_abi(func, vinsts, &func_abi)?;
            (alloc, Some(fast))
        }
        RegAllocAlgorithm::LinearScan | RegAllocAlgorithm::Greedy => (
            allocate_for_emit(func, vinsts, &func_abi, &lowered.loop_regions, alloc_trace)?,
            None,
        ),
    };
    let is_leaf = !vinsts.iter().any(|v| v.is_call());
    let is_sret = func_abi.is_sret();

    // Compute used callee-saved registers from allocation
    let mut used_callee_saved = PregSet::EMPTY;
    for preg_opt in &alloc.vreg_to_phys {
        if let Some(preg) = preg_opt {
            let p = PReg::int(*preg);
            if callee_saved_int().contains(p) {
                used_callee_saved.insert(p);
            }
        }
    }
    if let Some(ref f) = fast_from_walk {
        for h in &f.operand_homes {
            if let OperandHome::Reg(p) = h {
                let pr = PReg::int(*p);
                if callee_saved_int().contains(pr) {
                    used_callee_saved.insert(pr);
                }
            }
        }
    }
    // For sret, s1 is always "used" (reserved for preservation)
    if is_sret {
        used_callee_saved.insert(S1);
    }

    let caller_sret_bytes = module_abi.max_callee_sret_bytes();
    let has_call = vinsts.iter().any(|v| v.is_call());
    let clobber_hw = call_clobber_hw(&func_abi);
    let max_call_saves = if has_call {
        let m = max_regs_saved_across_calls(vinsts, &alloc, clobber_hw);
        if let Some(ref f) = fast_from_walk {
            m.max(f.max_call_preserve_slots)
        } else {
            m
        }
    } else {
        0
    };
    let s1_save_words = u32::from(is_sret && has_call);
    let extra_spill = max_call_saves.saturating_add(s1_save_words);
    let lpir_slot_sizes: Vec<(u32, u32)> = func
        .slots
        .iter()
        .enumerate()
        .map(|(i, s)| (i as u32, s.size))
        .collect();
    let caller_out_bytes = max_caller_outgoing_stack_bytes(vinsts);
    let frame = FrameLayout::compute(
        &func_abi,
        alloc.spill_count().saturating_add(extra_spill),
        used_callee_saved,
        &lpir_slot_sizes,
        is_leaf,
        caller_sret_bytes,
        caller_out_bytes,
    );

    let slot_base = alloc.spill_count();
    let s1_slot = if is_sret && has_call {
        Some(slot_base + max_call_saves)
    } else {
        None
    };
    let call_save = if has_call {
        Some(CallSaveLayout {
            slot_base,
            max_per_call: max_call_saves,
            clobber_hw,
            s1_slot,
        })
    } else {
        None
    };

    let call_save_edit_params = call_save.as_ref().map(|layout| CallSaveEditParams {
        slot_base: layout.slot_base,
        clobber_hw: layout.clobber_hw,
        s1_slot: layout.s1_slot,
        caller_is_sret: is_sret,
    });

    let mut ctx = if crate::config::USE_FAST_ALLOC_EMIT {
        let fast = if let Some(mut f) = fast_from_walk {
            if is_sret && has_call {
                if let Some(ss) = s1_slot {
                    inject_s1_edits_for_fastalloc_calls(&mut f.edits, vinsts, ss);
                }
            }
            f
        } else {
            AllocationAdapter::adapt(&alloc, vinsts, call_save_edit_params)
        };
        let mut ctx = EmitContext::with_frame(frame, debug_info, None);
        ctx.emit_prologue(is_sret, &alloc)?;
        for (pos, v) in vinsts.iter().enumerate() {
            ctx.emit_fast_edits(&fast, pos, true)?;
            let mut fos = FastOperandSource {
                fast: &fast,
                idx: fast.operand_base[pos],
                last_def: None,
            };
            ctx.emit_vinst(v, vinsts, pos, &mut fos, &alloc, is_sret)?;
            ctx.emit_fast_edits(&fast, pos, false)?;
        }
        ctx
    } else {
        let mut ctx = EmitContext::with_frame(frame, debug_info, call_save);
        ctx.emit_prologue(is_sret, &alloc)?;
        for (pos, v) in vinsts.iter().enumerate() {
            let mut los = LegacyOperandSource { alloc: &alloc };
            ctx.emit_vinst(v, vinsts, pos, &mut los, &alloc, is_sret)?;
        }
        ctx
    };

    ctx.resolve_branch_fixups()?;
    ctx.emit_epilogue();
    Ok(EmittedFunction {
        code: ctx.code,
        relocs: ctx.relocs,
        debug_lines: ctx.debug_lines,
    })
}

/// Append all local functions from `ir` into one RV32 ELF relocatable object.
///
/// # Arguments
/// * `ir` - The LPIR module to emit
/// * `sig` - Module signatures containing function metadata (for ABI classification)
/// * `float_mode` - Floating point mode (Q32 or SoftFloat)
/// * `alloc_trace` - When true, print allocation trace per function to stderr
pub fn emit_module_elf(
    ir: &lpir::IrModule,
    sig: &lps_shared::LpsModuleSig,
    float_mode: lpir::FloatMode,
    alloc_trace: bool,
) -> Result<Vec<u8>, NativeError> {
    if ir.functions.is_empty() {
        return Err(NativeError::EmptyModule);
    }

    let module_abi = ModuleAbi::from_ir_and_sig(ir, sig);

    // Build a map from function name to LpsFnSig for ABI classification
    let sig_map: BTreeMap<&str, &lps_shared::LpsFnSig> =
        sig.functions.iter().map(|s| (s.name.as_str(), s)).collect();

    let mut obj = Object::new(BinaryFormat::Elf, Architecture::Riscv32, Endianness::Little);
    obj.flags = FileFlags::Elf {
        os_abi: elf::ELFOSABI_NONE,
        abi_version: 0,
        e_flags: elf::EF_RISCV_FLOAT_ABI_SOFT,
    };

    let text = obj.section_id(StandardSection::Text);
    let mut undefined_syms: BTreeMap<String, SymbolId> = BTreeMap::new();

    for func in &ir.functions {
        // Get the signature for this function, or use a default (void -> void) if not found
        let default_sig = lps_shared::LpsFnSig {
            name: func.name.clone(),
            return_type: lps_shared::LpsType::Void,
            parameters: alloc::vec::Vec::new(),
        };
        let fn_sig = sig_map
            .get(func.name.as_str())
            .copied()
            .unwrap_or(&default_sig);
        let emitted = emit_function_bytes(
            func,
            ir,
            &module_abi,
            fn_sig,
            float_mode,
            false,
            alloc_trace,
        )?;
        let ctx = emitted;

        let func_off = obj.append_section_data(text, &ctx.code, 4);
        let scope = if func.is_entry {
            SymbolScope::Linkage
        } else {
            SymbolScope::Compilation
        };
        obj.add_symbol(Symbol {
            name: func.name.as_bytes().to_vec(),
            value: func_off,
            size: ctx.code.len() as u64,
            kind: SymbolKind::Text,
            scope,
            weak: false,
            section: SymbolSection::Section(text),
            flags: SymbolFlags::None,
        });

        for r in &ctx.relocs {
            let sym_id = if let Some(id) = undefined_syms.get(&r.symbol) {
                *id
            } else {
                let id = obj.add_symbol(Symbol {
                    name: r.symbol.as_bytes().to_vec(),
                    value: 0,
                    size: 0,
                    kind: SymbolKind::Text,
                    scope: SymbolScope::Linkage,
                    weak: false,
                    section: SymbolSection::Undefined,
                    flags: SymbolFlags::None,
                });
                undefined_syms.insert(r.symbol.clone(), id);
                id
            };
            obj.add_relocation(
                text,
                Relocation {
                    offset: func_off + r.offset as u64,
                    symbol: sym_id,
                    addend: 0,
                    flags: object::RelocationFlags::Elf {
                        // Standard R_RISCV_CALL_PLT is 17. The object crate incorrectly defines it as 19.
                        r_type: 17,
                    },
                },
            )
            .map_err(|e| NativeError::ObjectWrite(e.to_string()))?;
        }
    }

    obj.write()
        .map_err(|e| NativeError::ObjectWrite(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::abi::ModuleAbi;
    use crate::regalloc::LinearScan;
    use alloc::vec;

    use lpir::{IrFunction, IrModule, Op};
    use lps_shared::{FnParam, LpsFnSig, LpsModuleSig, LpsType, ParamQualifier};

    fn ir_single(f: IrFunction) -> IrModule {
        IrModule {
            imports: vec![],
            functions: vec![f],
        }
    }

    fn leaf_sig_module() -> LpsModuleSig {
        LpsModuleSig {
            functions: vec![leaf_lps_sig()],
        }
    }

    fn call_c_sig_module() -> LpsModuleSig {
        LpsModuleSig {
            functions: vec![call_test_lps_sig()],
        }
    }

    /// [`LpsFnSig`] consistent with [`leaf_add`] (vmctx + two scalar params, scalar return).
    fn leaf_lps_sig() -> LpsFnSig {
        LpsFnSig {
            name: String::from("leaf_add"),
            return_type: LpsType::Int,
            parameters: vec![
                FnParam {
                    name: String::from("a"),
                    ty: LpsType::Int,
                    qualifier: ParamQualifier::In,
                },
                FnParam {
                    name: String::from("b"),
                    ty: LpsType::Int,
                    qualifier: ParamQualifier::In,
                },
            ],
        }
    }

    /// Matches [`reloc_recorded_on_call`] IR (two float params, float return).
    fn call_test_lps_sig() -> LpsFnSig {
        LpsFnSig {
            name: String::from("c"),
            return_type: LpsType::Float,
            parameters: vec![
                FnParam {
                    name: String::from("a"),
                    ty: LpsType::Float,
                    qualifier: ParamQualifier::In,
                },
                FnParam {
                    name: String::from("b"),
                    ty: LpsType::Float,
                    qualifier: ParamQualifier::In,
                },
            ],
        }
    }

    fn leaf_add() -> IrFunction {
        IrFunction {
            name: String::from("leaf_add"),
            is_entry: true,
            vmctx_vreg: VReg(0),
            param_count: 2,
            return_types: vec![lpir::IrType::I32],
            vreg_types: vec![
                lpir::IrType::I32,
                lpir::IrType::I32,
                lpir::IrType::I32,
                lpir::IrType::I32,
            ],
            slots: vec![],
            body: vec![
                Op::Iadd {
                    dst: VReg(3),
                    lhs: VReg(1),
                    rhs: VReg(2),
                },
                Op::Return {
                    values: lpir::types::VRegRange { start: 0, count: 1 },
                },
            ],
            vreg_pool: vec![VReg(3)],
        }
    }

    #[test]
    fn emit_leaf_prologue_epilogue_size() {
        let f = leaf_add();
        let ir = ir_single(f.clone());
        let mabi = ModuleAbi::from_ir_and_sig(&ir, &leaf_sig_module());
        let lowered = crate::lower::lower_ops(&f, &ir, &mabi, lpir::FloatMode::Q32).expect("lower");
        let func_abi = func_abi_rv32(&leaf_lps_sig(), f.total_param_slots() as usize);
        let a = LinearScan::new()
            .allocate_with_func_abi(&f, &lowered.vinsts, &func_abi, &lowered.loop_regions, false)
            .expect("alloc");
        let mut ctx = EmitContext::new(true, false);
        ctx.emit_prologue(false, &a).expect("prologue");
        for (pos, i) in lowered.vinsts.iter().enumerate() {
            let mut los = LegacyOperandSource { alloc: &a };
            ctx.emit_vinst(i, lowered.vinsts.as_slice(), pos, &mut los, &a, false)
                .expect("emit");
        }
        ctx.emit_epilogue();
        assert!(ctx.code.len() >= 12);
        assert!(ctx.relocs.is_empty());
    }

    #[test]
    fn debug_lines_populated_when_enabled() {
        let f = leaf_add();
        let ir = ir_single(f.clone());
        let mabi = ModuleAbi::from_ir_and_sig(&ir, &leaf_sig_module());
        let e = emit_function_bytes(
            &f,
            &ir,
            &mabi,
            &leaf_lps_sig(),
            lpir::FloatMode::Q32,
            true,
            false,
        )
        .expect("emit");
        assert!(
            !e.debug_lines.is_empty(),
            "expected per-instruction debug lines"
        );
    }

    #[test]
    fn reloc_recorded_on_call() {
        let f = IrFunction {
            name: String::from("c"),
            is_entry: true,
            vmctx_vreg: VReg(0),
            param_count: 2,
            return_types: vec![],
            vreg_types: vec![lpir::IrType::I32; 4],
            slots: vec![],
            body: vec![
                Op::Fadd {
                    dst: VReg(3),
                    lhs: VReg(1),
                    rhs: VReg(2),
                },
                Op::Return {
                    values: lpir::types::VRegRange { start: 0, count: 1 },
                },
            ],
            vreg_pool: vec![VReg(3)],
        };
        let ir = ir_single(f.clone());
        let mabi = ModuleAbi::from_ir_and_sig(&ir, &call_c_sig_module());
        let lowered = crate::lower::lower_ops(&f, &ir, &mabi, lpir::FloatMode::Q32).expect("lower");
        let func_abi = func_abi_rv32(&call_test_lps_sig(), f.total_param_slots() as usize);
        let a = LinearScan::new()
            .allocate_with_func_abi(&f, &lowered.vinsts, &func_abi, &lowered.loop_regions, false)
            .expect("alloc");
        let mut ctx = EmitContext::new(false, false);
        ctx.emit_prologue(false, &a).expect("prologue");
        for (pos, i) in lowered.vinsts.iter().enumerate() {
            let mut los = LegacyOperandSource { alloc: &a };
            ctx.emit_vinst(i, lowered.vinsts.as_slice(), pos, &mut los, &a, false)
                .expect("emit");
        }
        ctx.emit_epilogue();
        assert_eq!(ctx.relocs.len(), 1);
        assert!(ctx.relocs[0].symbol.contains("fadd"));
    }
}
