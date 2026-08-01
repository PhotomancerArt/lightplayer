//! Xtensa (ESP32-S3 / LX7, LX6-compatible) forward emitter:
//! VInst + AllocOutput → machine code bytes.
//!
//! Ported from the experiment repo's `xt-mini-emit/src/emit.rs` (hardware-
//! proven on S3 silicon, LX6-conformance-verified) onto the exact interface of
//! [`crate::isa::rv32::emit`]. Original Photomancer code; machine encodings are
//! entirely delegated to `lp-xt-inst`; no GPL source (binutils, QEMU, GCC) was
//! consulted — see the experiment repo's license-provenance ADR.
//!
//! ## Layout contract
//!
//! ```text
//! [ j over pool ][ pad ][ literal pool ][ ENTRY, code ... ]   (pool non-empty)
//! [ ENTRY, code ... ]                                         (pool empty)
//! ^ offset 0 = the function entry either way
//! ```
//!
//! `L32R` reaches literals *backward only* (`target = ((PC+3) & !3) +
//! (one_extend(imm16) << 2)`), so the pool must precede the code. The generic
//! module assembler ([`crate::link::link_jit`] / [`crate::isa::IsaEmitOutput`])
//! has no entry-offset channel — every function's entry is byte 0 of its blob —
//! so the pool sits behind a single 3-byte `j` at offset 0 (plus one never-
//! executed pad byte to 4-align the pool). The windowed ABI permits this:
//! `CALLX8` latches `PS.CALLINC`, which survives the `j` until `ENTRY`
//! executes. The blob is padded to a multiple of 4 bytes so concatenated
//! functions keep their pools word-aligned (the code region itself is loaded
//! 4-aligned by both the JIT buffer and the device runner).
//!
//! ## Register model (from [`super::gpr`] / [`super::abi`] — no inline numbers)
//!
//! - `a0`/`a1`: RA / SP. **FP == SP** (`ENTRY` fixes the frame; `a1` is
//!   invariant), so every rv32 FP-relative access becomes SP-relative here.
//! - `a2..=a7`: the call-preserved program bank (callee-view args, RET_REGS).
//! - `a8`/`a9` ([`SCRATCH`]/[`SCRATCH2`]): emitter scratch — the only two
//!   scratch registers (rv32 has three temps). The Icmp/Select/Memcpy
//!   expansions below are adapted from xt-mini-emit's so every operand-spill
//!   combination fits in two scratch registers; the condition-mapping table and
//!   all immediate policies are xt-mini-emit's unchanged.
//! - `a10..=a15` ([`gpr::OUT_ARG_REGS`]): caller-view call-argument staging;
//!   results come back in [`gpr::CALL_RET_REGS`] (`a10`/`a11`).
//! - sret: the buffer pointer arrives in `a2` and **stays** in `a2`
//!   (`preserved_reg == ptr_reg` — see `super::abi::classify_return`), so
//!   rv32's prologue `mv s1, a0` has no counterpart here.
//!
//! ## Immediate discipline
//!
//! [`fn@lp_xt_inst::encode`] masks fields and silently truncates, so **every**
//! immediate is gated through [`super::imm`] before encoding; out-of-range
//! values take that table's documented fallback or return an emit error —
//! never silent truncation. Frames beyond `ENTRY`'s 32760-byte immediate are a
//! documented hard error (no `MOVSP` idiom — pinned by the experiment's ABI
//! ADR).
//!
//! ## The float half
//!
//! Every float `VInst`, the float spill/reload edits and the FP register model
//! live in [`super::emit_fp`], behind the `float-f32` feature. This module owns
//! the layout contract, the literal pool, branch relaxation, the frame and the
//! integer instruction set; that one owns the FP encodings, the Boolean-
//! register discipline and the `lsi`/`ssi` immediate discipline. The frame is
//! *not* split between them: float support changes nothing about it.
//!
//! ## Branch fixups
//!
//! Conditional branches (`beqz`/`bnez`, BRI12 ±2 KB) are layout items resolved
//! by an iterative sizing pass; out-of-range branches relax to the inverted
//! branch over a `j` (±128 KB). Relaxation is monotonic short→long, so the
//! loop converges. `j` and `l32r` reach failures are hard errors.

use alloc::string::String;
use alloc::vec::Vec;

use lp_xt_inst::{
    AluRrr, AluRs, AluRt, BrRr, BrZ, CallxOp, Inst, LoadOp, NullaryOp, Reg, ShiftSetOp, StoreOp,
    encode,
};

use crate::abi::{FrameLayout, RegClass};
use crate::isa::shared::{IsaEmitOutput, NativeReloc};
use crate::isa::xt::gpr::{self, SCRATCH, SCRATCH2, SP_REG};
use crate::isa::xt::imm::{self, ImmOp};
use crate::regalloc::{Alloc, AllocError, AllocOutput, Edit, EditPoint};
use crate::vinst::{AluImmOp, AluOp, IcmpCond, LabelId, ModuleSymbols, VInst, VReg};

/// Primary emitter scratch (`a8`) as an encoder operand.
pub(super) const S0: Reg = Reg::new(SCRATCH);
/// Secondary emitter scratch (`a9`) as an encoder operand.
pub(super) const S1: Reg = Reg::new(SCRATCH2);
/// The stack pointer (`a1`) as an encoder operand.
pub(super) const SP: Reg = Reg::new(SP_REG);

/// The windowed call increment this backend emits (`CALLX8`), fixed by
/// [`super::gpr::CALL_ROTATION`]'s register model.
const CALLX: CallxOp = CallxOp::Callx8;

// ---------------------------------------------------------------------------
// Literals and layout items
// ---------------------------------------------------------------------------

/// A literal-pool slot.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Literal {
    /// A known 32-bit constant (deduplicated by value).
    Const(u32),
    /// The absolute address of a call target, patched by the linker/JIT
    /// (deduplicated by symbol; reported as a call relocation).
    Sym(crate::vinst::SymbolId),
}

/// A layout item. Fixed bytes accumulate into [`Item::Bytes`] runs; items with
/// layout-dependent encodings stay symbolic until [`EmitContext::finish`].
#[derive(Clone, Debug)]
enum Item {
    /// Fixed, already-encoded bytes.
    Bytes(Vec<u8>),
    /// `beqz`/`bnez reg, label` — may relax to the inverted form over a `j`.
    CondBr { nez: bool, reg: Reg, label: LabelId },
    /// `j label`.
    Jump { label: LabelId },
    /// `l32r rt, <literal slot>`.
    L32r { rt: Reg, lit: usize },
    /// Label definition (zero size).
    LabelDef(LabelId),
}

#[derive(Clone, Debug)]
struct Slot {
    item: Item,
    /// Byte offset within the buffer (valid after layout).
    offset: u32,
    /// CondBr only: relaxed to the long (branch-over-`j`) form.
    long: bool,
}

/// `beqz`/`bnez` (BRI12) taken-target range (the legality table's
/// `Branch12Disp`, as `i64` bounds for the relaxation loop's arithmetic).
const BRI12_MIN: i64 = -2048;
const BRI12_MAX: i64 = 2047;

// ---------------------------------------------------------------------------
// Call-ABI slot mapping (caller view)
// ---------------------------------------------------------------------------

/// First LPIR call-arg index that spills to the outgoing stack area — the
/// Xtensa instance of `IsaTarget::lpir_call_stack_args_start` (6 register
/// args; legacy sret callees reserve `ARG_REGS[0]` for the emitter-injected
/// sret pointer). Local so this module never names the `IsaTarget` variant.
fn stack_args_start(callee_uses_sret: bool, caller_passes_sret_ptr: bool) -> usize {
    if callee_uses_sret && !caller_passes_sret_ptr {
        gpr::ARG_REGS.len() - 1
    } else {
        gpr::ARG_REGS.len()
    }
}

/// Caller-view staging register for the `arg_index`-th LPIR call operand —
/// rv32's `lpir_call_arg_target_hw` slot logic unchanged, mapped onto
/// [`gpr::OUT_ARG_REGS`] (the window rotation is invisible to the slot
/// mapping). `None` = stack-passed.
fn call_arg_staging_hw(
    callee_uses_sret: bool,
    caller_passes_sret_ptr: bool,
    caller_sret_vm_abi_swap: bool,
    arg_index: usize,
) -> Option<u8> {
    let slot = if !callee_uses_sret {
        arg_index
    } else if !caller_passes_sret_ptr {
        1usize.saturating_add(arg_index)
    } else if caller_sret_vm_abi_swap {
        // Shader / `needs_vmctx` path: [vmctx, sret, …] → [slot1, slot0, …]
        // (caller stages sret→a10, vmctx→a11).
        match arg_index {
            0 => 1,
            1 => 0,
            i => i,
        }
    } else {
        arg_index
    };
    gpr::OUT_ARG_REGS.get(slot).copied()
}

// ---------------------------------------------------------------------------
// Emit context
// ---------------------------------------------------------------------------

/// Emit context for building Xtensa machine code.
pub struct EmitContext<'a> {
    items: Vec<Slot>,
    literals: Vec<Literal>,
    /// `(item index, byte offset within that Bytes run, src_op)` — translated
    /// to absolute `(code_offset, Some(src_op))` debug lines after layout.
    marks: Vec<(usize, u32, u32)>,
    pub(super) frame: FrameLayout,
    symbols: &'a ModuleSymbols,
    /// Kept for API parity with [`emit_function`] (e.g. future pool-indexed lowering).
    #[allow(
        dead_code,
        reason = "reserved for emit API parity with pool-indexed lowering"
    )]
    vreg_pool: &'a [VReg],
    /// Labels already defined (duplicate detection), indexed by `LabelId`.
    defined_labels: Vec<bool>,
    /// Resolved label offsets, rebuilt by each layout pass.
    label_offsets: Vec<Option<u32>>,
    /// Next synthetic label id (minted above every id the VInst stream uses).
    next_label: LabelId,
    collect_debug_lines: bool,
}

impl<'a> EmitContext<'a> {
    fn new(
        frame: FrameLayout,
        symbols: &'a ModuleSymbols,
        vreg_pool: &'a [VReg],
        first_free_label: LabelId,
        collect_debug_lines: bool,
    ) -> Self {
        Self {
            items: Vec::new(),
            literals: Vec::new(),
            marks: Vec::new(),
            frame,
            symbols,
            vreg_pool,
            defined_labels: Vec::new(),
            label_offsets: Vec::new(),
            next_label: first_free_label,
            collect_debug_lines,
        }
    }

    // --- item / byte helpers ----------------------------------------------

    fn push_item(&mut self, item: Item, src_op: Option<u32>) {
        if self.collect_debug_lines
            && let Some(op) = src_op
        {
            self.marks.push((self.items.len(), 0, op));
        }
        self.items.push(Slot {
            item,
            offset: 0,
            long: false,
        });
    }

    /// Append one encoded instruction to the current `Bytes` run.
    pub(super) fn inst(&mut self, i: Inst, src_op: Option<u32>) {
        let bytes = encode(&i);
        let idx = match self.items.last() {
            Some(Slot {
                item: Item::Bytes(_),
                ..
            }) => self.items.len() - 1,
            _ => {
                self.items.push(Slot {
                    item: Item::Bytes(Vec::new()),
                    offset: 0,
                    long: false,
                });
                self.items.len() - 1
            }
        };
        let Item::Bytes(run) = &mut self.items[idx].item else {
            unreachable!("run selected above");
        };
        if self.collect_debug_lines
            && let Some(op) = src_op
        {
            self.marks.push((idx, run.len() as u32, op));
        }
        run.extend_from_slice(&bytes);
    }

    /// Intern a literal, deduplicated within this function's pool.
    fn lit(&mut self, l: Literal) -> usize {
        if let Some(i) = self.literals.iter().position(|&x| x == l) {
            return i;
        }
        self.literals.push(l);
        self.literals.len() - 1
    }

    /// Mint a fresh synthetic label (internal branch targets).
    fn fresh_label(&mut self) -> LabelId {
        let l = self.next_label;
        self.next_label += 1;
        l
    }

    /// Record a label definition (duplicate-defined labels are an error, as
    /// on rv32).
    fn record_label(&mut self, id: LabelId, src_op: Option<u32>) -> Result<(), AllocError> {
        let i = id as usize;
        if i >= self.defined_labels.len() {
            self.defined_labels.resize(i + 1, false);
        }
        if self.defined_labels[i] {
            return Err(crate::emit_err!("label {id} defined twice"));
        }
        self.defined_labels[i] = true;
        self.push_item(Item::LabelDef(id), src_op);
        Ok(())
    }

    // --- small code helpers ------------------------------------------------

    /// `mov rd, rs` (wide form `or rd, rs, rs`, as the assembler emits).
    fn mov(&mut self, rd: Reg, rs: Reg, src_op: Option<u32>) {
        if rd != rs {
            self.inst(Inst::Rrr(AluRrr::Or, rd, rs, rs), src_op);
        }
    }

    /// Materialize a 32-bit constant into `rd` (`movi`, else pooled `l32r` —
    /// the table's [`ImmOp::Movi`] fallback `LiteralPool`).
    pub(super) fn iconst(&mut self, rd: Reg, val: i32, src_op: Option<u32>) {
        if imm::is_legal(ImmOp::Movi, val) {
            self.inst(Inst::Movi(rd, val), src_op);
        } else {
            let lit = self.lit(Literal::Const(val as u32));
            self.push_item(Item::L32r { rt: rd, lit }, src_op);
        }
    }

    /// `rd = rs + imm` via `addi`/`addmi`/split (the table's [`ImmOp::Addi`]
    /// fallback chain), falling back to materializing into `tmp` + `add`.
    /// `tmp` must differ from `rs` when the constant path is reachable
    /// (checked; `tmp == rd` is fine — the constant path allows it only when
    /// `rd != rs`).
    pub(super) fn add_imm(
        &mut self,
        rd: Reg,
        rs: Reg,
        imm: i32,
        tmp: Reg,
        src_op: Option<u32>,
    ) -> Result<(), AllocError> {
        if imm == 0 {
            self.mov(rd, rs, src_op);
        } else if imm::is_legal(ImmOp::Addi, imm) {
            self.inst(Inst::Addi(rd, rs, imm), src_op);
        } else if imm::is_legal(ImmOp::Addmi, imm) {
            self.inst(Inst::Addmi(rd, rs, imm), src_op);
        } else {
            // addmi (high part, multiple of 256) + addi (signed low byte).
            let low = (imm << 24) >> 24;
            let high = imm.wrapping_sub(low);
            if imm::is_legal(ImmOp::Addmi, high) {
                self.inst(Inst::Addmi(rd, rs, high), src_op);
                self.inst(Inst::Addi(rd, rd, low), src_op);
            } else {
                if tmp == rs {
                    return Err(crate::emit_err!(
                        "add_imm: materialization scratch aliases the source register"
                    ));
                }
                self.iconst(tmp, imm, src_op);
                self.inst(Inst::Rrr(AluRrr::Add, rd, rs, tmp), src_op);
            }
        }
        Ok(())
    }

    /// Reduce a load/store address to an encodable `(base, offset)` pair,
    /// computing `base + offset` into [`S0`] when the offset is illegal for
    /// `op` (negative, out of range, or misaligned — the table's
    /// `AddressScratch` fallback). The value operand must therefore not live
    /// in `S0` when the fallback is reachable; the constant scratch is `S1`.
    pub(super) fn mem_addr(
        &mut self,
        base: Reg,
        offset: i32,
        op: ImmOp,
        src_op: Option<u32>,
    ) -> Result<(Reg, u32), AllocError> {
        if imm::is_legal(op, offset) {
            Ok((base, offset as u32))
        } else {
            self.add_imm(S0, base, offset, S1, src_op)?;
            Ok((S0, 0))
        }
    }

    /// Load spill slot `slot` into `dst` (SP-relative — FP == SP on Xtensa).
    /// Out-of-range slot offsets compute the address into `dst` itself.
    fn spill_load(&mut self, dst: Reg, slot: u8, src_op: Option<u32>) -> Result<(), AllocError> {
        let off = self
            .frame
            .spill_offset_from_sp(slot as u32)
            .ok_or(crate::emit_err!())?;
        if imm::is_legal(ImmOp::L32i, off) {
            self.inst(Inst::Load(LoadOp::L32i, dst, SP, off as u32), src_op);
        } else {
            // SP != dst always, so dst can double as the materialization tmp.
            self.add_imm(dst, SP, off, dst, src_op)?;
            self.inst(Inst::Load(LoadOp::L32i, dst, dst, 0), src_op);
        }
        Ok(())
    }

    /// Store `src` into spill slot `slot` (SP-relative). Out-of-range slot
    /// offsets go through an address scratch that never aliases `src`.
    fn spill_store(&mut self, src: Reg, slot: u8, src_op: Option<u32>) -> Result<(), AllocError> {
        let off = self
            .frame
            .spill_offset_from_sp(slot as u32)
            .ok_or(crate::emit_err!())?;
        if imm::is_legal(ImmOp::S32i, off) {
            self.inst(Inst::Store(StoreOp::S32i, src, SP, off as u32), src_op);
        } else {
            let scr = if src == S1 { S0 } else { S1 };
            self.add_imm(scr, SP, off, scr, src_op)?;
            self.inst(Inst::Store(StoreOp::S32i, src, scr, 0), src_op);
        }
        Ok(())
    }

    // --- operand plumbing (rv32-parity) ------------------------------------

    pub(super) fn operand_alloc(
        output: &AllocOutput,
        inst_idx: usize,
        operand_idx: usize,
    ) -> Alloc {
        output.operand_alloc(inst_idx as u16, operand_idx as u16)
    }

    pub(super) fn is_dead_def(output: &AllocOutput, inst_idx: usize, def_op_idx: usize) -> bool {
        matches!(
            Self::operand_alloc(output, inst_idx, def_op_idx),
            Alloc::None
        )
    }

    /// The `a`-register named by a register allocation.
    ///
    /// The single gate between the allocator's class-aware `Alloc` and this
    /// emitter's `Reg`. Rejects a float-class allocation outright rather than
    /// unwrapping the hardware index: the Xtensa FPU backend is a later
    /// milestone, and `a0..a15` and `f0..f15` are different register files, so
    /// an integer instruction against a float index would be silently wrong
    /// rather than merely unimplemented.
    pub(super) fn hw(preg: crate::abi::PackedPReg) -> Result<Reg, AllocError> {
        match preg.class() {
            RegClass::Float => Err(crate::emit_err!(
                "allocation names float register f{} — Xtensa has no FPU backend",
                preg.hw()
            )),
            RegClass::Int if preg.hw() < 16 => Ok(Reg::new(preg.hw())),
            RegClass::Int => Err(crate::emit_err!("allocation names non-GPR a{}", preg.hw())),
        }
    }

    /// Use a vreg: return its physical register, reloading from spill into
    /// `temp` if needed.
    pub(super) fn use_vreg(
        &mut self,
        output: &AllocOutput,
        inst_idx: usize,
        operand_idx: usize,
        temp: Reg,
        src_op: Option<u32>,
    ) -> Result<Reg, AllocError> {
        match Self::operand_alloc(output, inst_idx, operand_idx) {
            Alloc::Reg(preg) => Self::hw(preg),
            Alloc::Stack(slot) => {
                self.spill_load(temp, slot, src_op)?;
                Ok(temp)
            }
            Alloc::None => Err(crate::emit_err!()),
        }
    }

    /// Def a vreg: return the physical register to write to (the caller must
    /// [`Self::store_def_vreg`] afterwards when the def is spilled).
    pub(super) fn def_vreg(
        &mut self,
        output: &AllocOutput,
        inst_idx: usize,
        operand_idx: usize,
        temp: Reg,
    ) -> Result<Reg, AllocError> {
        match Self::operand_alloc(output, inst_idx, operand_idx) {
            Alloc::Reg(preg) => Self::hw(preg),
            Alloc::Stack(_) => Ok(temp),
            Alloc::None => Err(crate::emit_err!()),
        }
    }

    /// Store a spilled def after it was written to `temp`.
    pub(super) fn store_def_vreg(
        &mut self,
        output: &AllocOutput,
        inst_idx: usize,
        operand_idx: usize,
        temp: Reg,
        src_op: Option<u32>,
    ) -> Result<(), AllocError> {
        if let Alloc::Stack(slot) = Self::operand_alloc(output, inst_idx, operand_idx) {
            self.spill_store(temp, slot, src_op)?;
        }
        Ok(())
    }

    /// Emit an allocator edit (reload/spill/reg move) as concrete instructions.
    fn emit_edit(&mut self, edit: &Edit, src_op: Option<u32>) -> Result<(), AllocError> {
        // Float-class edits use `lsi`/`ssi`/`mov.s`; see `emit_fp` for which
        // shapes it claims and why stack-to-stack deliberately is not one of
        // them (it is a class-free word copy the integer path already does).
        #[cfg(feature = "float-f32")]
        if self.emit_float_edit(edit, src_op)? {
            return Ok(());
        }
        match edit {
            Edit::Move { from, to } => match (*from, *to) {
                (Alloc::None, _) | (_, Alloc::None) => return Err(crate::emit_err!()),
                (Alloc::Reg(src), Alloc::Reg(dst)) => {
                    self.mov(Self::hw(dst)?, Self::hw(src)?, src_op);
                }
                (Alloc::Stack(slot), Alloc::Reg(dst)) => {
                    self.spill_load(Self::hw(dst)?, slot, src_op)?;
                }
                (Alloc::Reg(src), Alloc::Stack(slot)) => {
                    self.spill_store(Self::hw(src)?, slot, src_op)?;
                }
                (Alloc::Stack(s_from), Alloc::Stack(s_to)) => {
                    self.spill_load(S0, s_from, src_op)?;
                    self.spill_store(S0, s_to, src_op)?;
                }
            },
            Edit::LoadIncomingArg { fp_offset, to } => {
                // rv32 reads `[FP + fp_offset]` where FP == the caller's SP.
                // Xtensa has no FP: `callee SP + ENTRY frame == caller SP`, so
                // the same word lives at `[SP + total_size + fp_offset]`.
                let off = self.frame.total_size as i32 + *fp_offset;
                match *to {
                    Alloc::Reg(dst) => {
                        let dst = Self::hw(dst)?;
                        let (b, o) = self.mem_addr(SP, off, ImmOp::L32i, src_op)?;
                        self.inst(Inst::Load(LoadOp::L32i, dst, b, o), src_op);
                    }
                    Alloc::Stack(slot) => {
                        let (b, o) = self.mem_addr(SP, off, ImmOp::L32i, src_op)?;
                        self.inst(Inst::Load(LoadOp::L32i, S0, b, o), src_op);
                        self.spill_store(S0, slot, src_op)?;
                    }
                    Alloc::None => return Err(crate::emit_err!()),
                }
            }
        }
        Ok(())
    }

    // --- prologue / epilogue -----------------------------------------------

    /// Prologue: one `entry a1, frame` — the whole frame setup under the
    /// windowed ABI (RA/FP/callee-saves are handled by the window rotation and
    /// the reserved frame-top save areas; see `super::abi`). sret functions
    /// emit nothing extra: the pointer arrives in `a2` and stays there.
    fn emit_prologue(&mut self, _is_sret: bool) -> Result<(), AllocError> {
        let frame_size = self.frame.total_size;
        if !imm::is_legal(ImmOp::EntryFrame, frame_size as i32) {
            // Silent truncation by the encoder would mean `entry a1, 0` —
            // documented hard error instead (no MOVSP idiom by policy).
            return Err(crate::emit_err!(
                "frame of {frame_size} bytes exceeds ENTRY's immediate limit of 32760"
            ));
        }
        self.inst(Inst::Entry(SP, frame_size), None);
        Ok(())
    }

    /// Epilogue: `retw` (wide form; density is a later optimization).
    fn emit_epilogue(&mut self) {
        self.inst(Inst::Nullary(NullaryOp::Retw), None);
    }

    // --- expansions ---------------------------------------------------------

    /// `acc = (l COND r) ? 1 : 0` via the branch-if-true table (operands
    /// swapped for the conditions Xtensa lacks — xt-mini-emit's mapping):
    ///
    /// ```text
    ///   b<cond> rs, rt, +5      ; taken -> the movi 1     (3 bytes)
    ///   movi    acc, 0                                    (3 bytes)
    ///   j       +2              ; over the movi 1         (3 bytes)
    ///   movi    acc, 1                                    (3 bytes)
    /// ```
    ///
    /// All four instructions are fixed-size, so the skips are constants.
    /// Unlike xt-mini-emit's accumulate-then-branch form, `acc` is written
    /// only *after* the branch reads `l`/`r`, so `acc` may alias either
    /// operand or a reload scratch — which is what lets every spill
    /// combination fit the two scratch registers.
    fn icmp_core(&mut self, acc: Reg, l: Reg, r: Reg, cond: IcmpCond, src_op: Option<u32>) {
        let (op, rs, rt) = match cond {
            IcmpCond::Eq => (BrRr::Beq, l, r),
            IcmpCond::Ne => (BrRr::Bne, l, r),
            IcmpCond::LtS => (BrRr::Blt, l, r),
            IcmpCond::GeS => (BrRr::Bge, l, r),
            IcmpCond::LtU => (BrRr::Bltu, l, r),
            IcmpCond::GeU => (BrRr::Bgeu, l, r),
            IcmpCond::GtS => (BrRr::Blt, r, l),
            IcmpCond::LeS => (BrRr::Bge, r, l),
            IcmpCond::GtU => (BrRr::Bltu, r, l),
            IcmpCond::LeU => (BrRr::Bgeu, r, l),
        };
        debug_assert!(imm::is_legal(ImmOp::Branch8Disp, 5));
        debug_assert!(imm::is_legal(ImmOp::JDisp, 2));
        self.inst(Inst::BranchRr(op, rs, rt, 5), src_op);
        self.inst(Inst::Movi(acc, 0), src_op);
        self.inst(Inst::J(2), src_op);
        self.inst(Inst::Movi(acc, 1), src_op);
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
        let l = self.use_vreg(output, inst_idx, 1, S0, src_op)?;
        let r = self.use_vreg(output, inst_idx, 2, S1, src_op)?;
        let rd = self.def_vreg(output, inst_idx, 0, S0)?;
        self.icmp_core(rd, l, r, cond, src_op);
        self.store_def_vreg(output, inst_idx, 0, S0, src_op)
    }

    /// `IcmpImm`: materialize the immediate (movi / pooled l32r) then the
    /// register compare — xt-mini-emit's lowering. All conditions are
    /// supported (rv32 currently special-cases Eq only).
    fn emit_icmp_imm(
        &mut self,
        output: &AllocOutput,
        inst_idx: usize,
        imm_val: i32,
        cond: IcmpCond,
        src_op: Option<u32>,
    ) -> Result<(), AllocError> {
        if Self::is_dead_def(output, inst_idx, 0) {
            return Ok(());
        }
        let s = self.use_vreg(output, inst_idx, 1, S0, src_op)?;
        self.iconst(S1, imm_val, src_op);
        let rd = self.def_vreg(output, inst_idx, 0, S0)?;
        self.icmp_core(rd, s, S1, cond, src_op);
        self.store_def_vreg(output, inst_idx, 0, S0, src_op)
    }

    /// `Select`: branchy form (`beqz` over the true-value load).
    ///
    /// xt-mini-emit lowers Select with `movnez`, which needs `if_true`,
    /// `cond`, and the `if_false` accumulator live at once — three registers,
    /// which the two-scratch monorepo emitter cannot guarantee when operands
    /// are spilled. The branchy form needs at most two live scratch values at
    /// any point and reuses the relaxable-branch machinery:
    ///
    /// ```text
    ///   <if_false -> S0>
    ///   beqz cond, Lskip
    ///   <if_true -> S0>
    /// Lskip:
    ///   mov dst, S0
    /// ```
    fn emit_select(
        &mut self,
        output: &AllocOutput,
        inst_idx: usize,
        src_op: Option<u32>,
    ) -> Result<(), AllocError> {
        if Self::is_dead_def(output, inst_idx, 0) {
            return Ok(());
        }
        let f = self.use_vreg(output, inst_idx, 3, S0, src_op)?;
        self.mov(S0, f, src_op);
        let c = self.use_vreg(output, inst_idx, 1, S1, src_op)?;
        let skip = self.fresh_label();
        self.push_item(
            Item::CondBr {
                nez: false,
                reg: c,
                label: skip,
            },
            src_op,
        );
        let t = self.use_vreg(output, inst_idx, 2, S1, src_op)?;
        self.mov(S0, t, src_op);
        self.record_label(skip, src_op)?;
        let rd = self.def_vreg(output, inst_idx, 0, S0)?;
        self.mov(rd, S0, src_op);
        self.store_def_vreg(output, inst_idx, 0, S0, src_op)
    }

    fn emit_alu_rrr(
        &mut self,
        output: &AllocOutput,
        inst_idx: usize,
        op: AluOp,
        src_op: Option<u32>,
    ) -> Result<(), AllocError> {
        if Self::is_dead_def(output, inst_idx, 0) {
            return Ok(());
        }
        let s1 = self.use_vreg(output, inst_idx, 1, S0, src_op)?;
        let s2 = self.use_vreg(output, inst_idx, 2, S1, src_op)?;
        let rd = self.def_vreg(output, inst_idx, 0, S0)?;
        let direct = match op {
            AluOp::Add => Some(AluRrr::Add),
            AluOp::Sub => Some(AluRrr::Sub),
            AluOp::Mul => Some(AluRrr::Mull),
            AluOp::MulH => Some(AluRrr::Mulsh),
            AluOp::And => Some(AluRrr::And),
            AluOp::Or => Some(AluRrr::Or),
            AluOp::Xor => Some(AluRrr::Xor),
            AluOp::DivS => Some(AluRrr::Quos),
            AluOp::DivU => Some(AluRrr::Quou),
            AluOp::RemS => Some(AluRrr::Rems),
            AluOp::RemU => Some(AluRrr::Remu),
            AluOp::Sll | AluOp::SrlU | AluOp::SraS => None,
        };
        if let Some(x) = direct {
            self.inst(Inst::Rrr(x, rd, s1, s2), src_op);
        } else {
            // Register-amount shifts go through SAR (`ssl`/`ssr` latch the
            // amount mod 32, matching RISC-V's `& 31` semantics).
            match op {
                AluOp::Sll => {
                    self.inst(Inst::ShiftSet(ShiftSetOp::Ssl, s2), src_op);
                    self.inst(Inst::Rs(AluRs::Sll, rd, s1), src_op);
                }
                AluOp::SrlU => {
                    self.inst(Inst::ShiftSet(ShiftSetOp::Ssr, s2), src_op);
                    self.inst(Inst::Rt(AluRt::Srl, rd, s1), src_op);
                }
                AluOp::SraS => {
                    self.inst(Inst::ShiftSet(ShiftSetOp::Ssr, s2), src_op);
                    self.inst(Inst::Rt(AluRt::Sra, rd, s1), src_op);
                }
                _ => unreachable!("direct ops handled above"),
            }
        }
        self.store_def_vreg(output, inst_idx, 0, S0, src_op)
    }

    fn emit_alu_rri(
        &mut self,
        output: &AllocOutput,
        inst_idx: usize,
        op: AluImmOp,
        imm_val: i32,
        src_op: Option<u32>,
    ) -> Result<(), AllocError> {
        if Self::is_dead_def(output, inst_idx, 0) {
            return Ok(());
        }
        let s = self.use_vreg(output, inst_idx, 1, S0, src_op)?;
        let rd = self.def_vreg(output, inst_idx, 0, S0)?;
        match op {
            AluImmOp::Addi => self.add_imm(rd, s, imm_val, S1, src_op)?,
            AluImmOp::Andi | AluImmOp::Ori | AluImmOp::Xori => {
                // The key Xtensa fact: no and/or/xor-immediate forms exist
                // (the table's `NoImmForm`) — materialize + RRR, always.
                self.iconst(S1, imm_val, src_op);
                let x = match op {
                    AluImmOp::Andi => AluRrr::And,
                    AluImmOp::Ori => AluRrr::Or,
                    _ => AluRrr::Xor,
                };
                self.inst(Inst::Rrr(x, rd, s, S1), src_op);
            }
            AluImmOp::Slli => {
                let sa = imm_val as u32 & 31;
                if sa == 0 {
                    self.mov(rd, s, src_op);
                } else {
                    debug_assert!(imm::is_legal(ImmOp::SlliSa, sa as i32));
                    self.inst(Inst::Slli(rd, s, sa as u8), src_op);
                }
            }
            AluImmOp::SrliU => {
                let sa = imm_val as u32 & 31;
                if sa == 0 {
                    self.mov(rd, s, src_op);
                } else if imm::is_legal(ImmOp::SrliSa, sa as i32) {
                    self.inst(Inst::Srli(rd, s, sa as u8), src_op);
                } else {
                    // srli sa>=16 has no encoding; extract the top (32-sa)
                    // bits instead (the table's `OtherOpcode` fallback).
                    debug_assert!(imm::extui_legal(sa as i32, 32 - sa as i32));
                    self.inst(Inst::Extui(rd, s, sa as u8, (32 - sa) as u8), src_op);
                }
            }
            AluImmOp::SraiS => {
                let sa = imm_val as u32 & 31;
                if sa == 0 {
                    self.mov(rd, s, src_op);
                } else {
                    debug_assert!(imm::is_legal(ImmOp::SraiSa, sa as i32));
                    self.inst(Inst::Srai(rd, s, sa as u8), src_op);
                }
            }
            AluImmOp::Slti => {
                self.iconst(S1, imm_val, src_op);
                self.icmp_core(rd, s, S1, IcmpCond::LtS, src_op);
            }
            AluImmOp::SltiU => {
                self.iconst(S1, imm_val, src_op);
                self.icmp_core(rd, s, S1, IcmpCond::LtU, src_op);
            }
        }
        self.store_def_vreg(output, inst_idx, 0, S0, src_op)
    }

    /// Word-granular memcpy. Register-resident bases use xt-mini-emit's
    /// bump-and-restore (unrolled `l32i`/`s32i` through [`S0`], both bases
    /// advanced by whole ≤1024-byte chunks and restored exactly — the
    /// program-visible contract keeps the base registers unchanged). When
    /// both bases are spilled there is no third register for the data word,
    /// so each word reloads the pointers from their spill slots (slow, and
    /// per-word offsets are capped by the addmi-split range — far beyond any
    /// LPIR aggregate).
    fn emit_memcpy(
        &mut self,
        output: &AllocOutput,
        inst_idx: usize,
        size: u32,
        src_op: Option<u32>,
    ) -> Result<(), AllocError> {
        if !size.is_multiple_of(4) {
            return Err(crate::emit_err!(
                "MemcpyWords size {size} not a multiple of 4"
            ));
        }
        if size == 0 {
            return Ok(());
        }
        let a_dst = Self::operand_alloc(output, inst_idx, 0);
        let a_src = Self::operand_alloc(output, inst_idx, 1);
        if a_dst == a_src {
            // Self-copy is the identity; emitting it anyway would double-bump
            // a shared base register between chunks.
            return Ok(());
        }
        if let (Alloc::Stack(sd), Alloc::Stack(ss)) = (a_dst, a_src) {
            // Slow path: no register can hold a base across the data load.
            const MAX_SLOW_OFF: i32 = 32512 + 127; // addmi + addi reach
            if size as i32 > MAX_SLOW_OFF {
                return Err(crate::emit_err!(
                    "MemcpyWords of {size} bytes with both bases spilled exceeds the addressable range"
                ));
            }
            let mut off = 0u32;
            while off < size {
                self.spill_load(S0, ss, src_op)?;
                if imm::is_legal(ImmOp::L32i, off as i32) {
                    self.inst(Inst::Load(LoadOp::L32i, S0, S0, off), src_op);
                } else {
                    self.add_imm(S0, S0, off as i32, S0, src_op)?;
                    self.inst(Inst::Load(LoadOp::L32i, S0, S0, 0), src_op);
                }
                self.spill_load(S1, sd, src_op)?;
                if imm::is_legal(ImmOp::S32i, off as i32) {
                    self.inst(Inst::Store(StoreOp::S32i, S0, S1, off), src_op);
                } else {
                    self.add_imm(S1, S1, off as i32, S1, src_op)?;
                    self.inst(Inst::Store(StoreOp::S32i, S0, S1, 0), src_op);
                }
                off += 4;
            }
            return Ok(());
        }

        // Fast path: at most one base spilled — the spilled one reloads into
        // S1, the other stays in its pool register (bumped and restored).
        let dst = self.use_vreg(output, inst_idx, 0, S1, src_op)?;
        let src = self.use_vreg(output, inst_idx, 1, S1, src_op)?;
        if dst == src {
            return Ok(());
        }
        const CHUNK: u32 = 1024; // offsets 0..=1020 step 4
        let mut copied = 0u32;
        let mut bumped = 0i32;
        while copied < size {
            let chunk = (size - copied).min(CHUNK);
            let mut off = 0;
            while off < chunk {
                debug_assert!(imm::is_legal(ImmOp::L32i, off as i32));
                self.inst(Inst::Load(LoadOp::L32i, S0, src, off), src_op);
                self.inst(Inst::Store(StoreOp::S32i, S0, dst, off), src_op);
                off += 4;
            }
            copied += chunk;
            if copied < size {
                // chunk == 1024 here: addmi-encodable, no scratch needed.
                self.add_imm(src, src, chunk as i32, S0, src_op)?;
                self.add_imm(dst, dst, chunk as i32, S0, src_op)?;
                bumped += chunk as i32;
            }
        }
        if bumped != 0 {
            self.add_imm(src, src, -bumped, S0, src_op)?;
            self.add_imm(dst, dst, -bumped, S0, src_op)?;
        }
        Ok(())
    }

    /// Clobber-free double move `dsts[i] <- srcs[i]` (xt-mini-emit's ordering:
    /// straight, reversed, or the full swap bounced through [`S0`]).
    fn move2(&mut self, dsts: [Reg; 2], srcs: [Reg; 2], src_op: Option<u32>) {
        let [d0, d1] = dsts;
        let [s0, s1] = srcs;
        if d0 != s1 {
            self.mov(d0, s0, src_op);
            self.mov(d1, s1, src_op);
        } else if d1 != s0 {
            self.mov(d1, s1, src_op);
            self.mov(d0, s0, src_op);
        } else {
            self.mov(S0, s0, src_op);
            self.mov(d1, s1, src_op);
            self.mov(d0, S0, src_op);
        }
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "mirrors the VInst::Call field set"
    )]
    fn emit_call(
        &mut self,
        output: &AllocOutput,
        inst_idx: usize,
        target: crate::vinst::SymbolId,
        n_args: usize,
        n_rets: usize,
        callee_uses_sret: bool,
        caller_passes_sret_ptr: bool,
        caller_sret_vm_abi_swap: bool,
        src_op: Option<u32>,
    ) -> Result<(), AllocError> {
        let cap = stack_args_start(callee_uses_sret, caller_passes_sret_ptr);

        // Store overflow args to the outgoing stack area at [SP + offset]
        // (the frame bottom; the callee reads them at its SP + frame + off).
        for i in cap..n_args {
            let operand_idx = n_rets + i;
            let stack_off = ((i - cap) * 4) as i32;
            match Self::operand_alloc(output, inst_idx, operand_idx) {
                Alloc::Reg(src) => {
                    let src = Self::hw(src)?;
                    let (b, o) = self.mem_addr(SP, stack_off, ImmOp::S32i, src_op)?;
                    self.inst(Inst::Store(StoreOp::S32i, src, b, o), src_op);
                }
                Alloc::Stack(slot) => {
                    self.spill_load(S0, slot, src_op)?;
                    let (b, o) = self.mem_addr(SP, stack_off, ImmOp::S32i, src_op)?;
                    self.inst(Inst::Store(StoreOp::S32i, S0, b, o), src_op);
                }
                Alloc::None => {}
            }
        }

        // Stage register args at the caller-view staging bank a10..=a15.
        // Under the real allocator each arg's alloc *is* its staging register
        // (walk.rs pins `allocs[arg] = Reg(target)` and inserts the Before
        // moves; verify.rs asserts it), so these movs are elided; they remain
        // as the correctness net for hand-built AllocOutputs. Sources inside
        // the staging bank at the wrong slot would be clobbered by an earlier
        // stage — refuse rather than emit silently wrong code.
        for i in 0..n_args.min(cap) {
            let Some(dest) = call_arg_staging_hw(
                callee_uses_sret,
                caller_passes_sret_ptr,
                caller_sret_vm_abi_swap,
                i,
            ) else {
                return Err(crate::emit_err!(
                    "register-pass arg {i} has no staging slot"
                ));
            };
            // `call_arg_staging_hw` names an `a`-register directly, so the
            // class is integer by construction.
            let dest = Self::hw(crate::abi::PackedPReg::int(dest))?;
            match Self::operand_alloc(output, inst_idx, n_rets + i) {
                Alloc::Reg(src) => {
                    let src = Self::hw(src)?;
                    if src != dest && gpr::is_out_arg_reg(src.num()) {
                        return Err(crate::emit_err!(
                            "call arg {i} allocated to staging register a{} but staged at a{}",
                            src.num(),
                            dest.num()
                        ));
                    }
                    self.mov(dest, src, src_op);
                }
                Alloc::Stack(slot) => self.spill_load(dest, slot, src_op)?,
                Alloc::None => {}
            }
        }

        // Legacy sret: the emitter synthesizes the callee's sret pointer from
        // the caller's sret slot into the first staging register.
        if callee_uses_sret && !caller_passes_sret_ptr {
            let sret_sp_off = self
                .frame
                .sret_slot_offset_from_fp()
                .ok_or(crate::emit_err!())?
                + self.frame.total_size as i32;
            let dest = Reg::new(gpr::OUT_ARG_REGS[0]);
            self.add_imm(dest, SP, sret_sp_off, S1, src_op)?;
        }

        // The call: pooled absolute callee address + l32r + callx8. The
        // literal slot's offset is the call relocation (patched by
        // `super::link::patch_call_literal` / R_XTENSA_32); slots are
        // deduplicated per symbol, so repeated callees share one slot and one
        // relocation.
        let lit = self.lit(Literal::Sym(target));
        self.push_item(Item::L32r { rt: S0, lit }, src_op);
        self.inst(Inst::Callx(CALLX, S0), src_op);

        if callee_uses_sret && !caller_passes_sret_ptr {
            // Read results back from the caller-side sret buffer.
            let sret_sp_off = self
                .frame
                .sret_slot_offset_from_fp()
                .ok_or(crate::emit_err!())?
                + self.frame.total_size as i32;
            for i in 0..n_rets {
                let buf_off = sret_sp_off + (i as i32) * 4;
                match Self::operand_alloc(output, inst_idx, i) {
                    Alloc::Reg(dst) => {
                        let dst = Self::hw(dst)?;
                        let (b, o) = self.mem_addr(SP, buf_off, ImmOp::L32i, src_op)?;
                        self.inst(Inst::Load(LoadOp::L32i, dst, b, o), src_op);
                    }
                    Alloc::Stack(slot) => {
                        let (b, o) = self.mem_addr(SP, buf_off, ImmOp::L32i, src_op)?;
                        self.inst(Inst::Load(LoadOp::L32i, S0, b, o), src_op);
                        self.spill_store(S0, slot, src_op)?;
                    }
                    Alloc::None => {}
                }
            }
        } else if !callee_uses_sret {
            // Direct results come back in the caller-view a10/a11
            // (CALL_RET_REGS). As with arg staging, the real allocator pins
            // each ret's alloc to exactly these registers, so the moves are
            // elided; spilled rets store through scratch.
            if n_rets > gpr::CALL_RET_REGS.len() {
                return Err(crate::emit_err!(
                    "{n_rets} direct return words exceed CALL_RET_REGS"
                ));
            }
            // Spill stores first (they clobber no registers), then the
            // register moves in clobber-free order.
            let mut reg_moves: [Option<Reg>; 2] = [None, None];
            for i in 0..n_rets {
                let src = Reg::new(gpr::CALL_RET_REGS[i]);
                match Self::operand_alloc(output, inst_idx, i) {
                    Alloc::Reg(dst) => reg_moves[i] = Some(Self::hw(dst)?),
                    Alloc::Stack(slot) => self.spill_store(src, slot, src_op)?,
                    Alloc::None => {}
                }
            }
            let ret0 = Reg::new(gpr::CALL_RET_REGS[0]);
            match reg_moves {
                [None, None] => {}
                [Some(d0), None] => self.mov(d0, ret0, src_op),
                [None, Some(d1)] => self.mov(d1, Reg::new(gpr::CALL_RET_REGS[1]), src_op),
                [Some(d0), Some(d1)] => {
                    self.move2([d0, d1], [ret0, Reg::new(gpr::CALL_RET_REGS[1])], src_op)
                }
            }
        }
        // callee_uses_sret && caller_passes_sret_ptr: results land in the
        // caller-provided buffer; nothing to read back (rv32 parity).
        Ok(())
    }

    fn emit_ret(
        &mut self,
        output: &AllocOutput,
        inst_idx: usize,
        n_vals: usize,
        is_sret: bool,
        src_op: Option<u32>,
    ) -> Result<(), AllocError> {
        if is_sret {
            // Store the scalars through the preserved sret pointer (a2 for
            // the whole function — see the module docs). Offsets are
            // `4 * i` with `i < 256`, always within s32i's 0..=1020 range.
            let base = Reg::new(gpr::ARG_REGS[0]);
            for i in 0..n_vals {
                let off = (i * 4) as i32;
                if !imm::is_legal(ImmOp::S32i, off) {
                    return Err(crate::emit_err!());
                }
                let src = self.use_vreg(output, inst_idx, i, S0, src_op)?;
                self.inst(Inst::Store(StoreOp::S32i, src, base, off as u32), src_op);
            }
            return Ok(());
        }
        if n_vals > gpr::RET_REGS.len() {
            return Err(crate::emit_err!(
                "{n_vals} direct return words exceed RET_REGS (wider returns use sret)"
            ));
        }
        match n_vals {
            0 => {}
            1 => {
                let s = self.use_vreg(output, inst_idx, 0, S0, src_op)?;
                self.mov(Reg::new(gpr::RET_REGS[0]), s, src_op);
            }
            2 => {
                // Destinations a2/a3 overlap the program bank: clobber-free
                // ordering (spilled sources reload into scratch, which never
                // aliases the destinations).
                let s0 = self.use_vreg(output, inst_idx, 0, S0, src_op)?;
                let s1 = self.use_vreg(output, inst_idx, 1, S1, src_op)?;
                self.move2(
                    [Reg::new(gpr::RET_REGS[0]), Reg::new(gpr::RET_REGS[1])],
                    [s0, s1],
                    src_op,
                );
            }
            _ => unreachable!("bounded above"),
        }
        Ok(())
    }

    /// FuelCheck expansion (see `VInst::FuelCheck` docs; semantics identical
    /// to rv32's arm — same vmctx offsets, check-then-decrement):
    ///
    /// ```text
    ///   l32i S0, rv, FUEL       # fuel low word
    ///   bnez S0, +8             # not exhausted -> skip the trap block
    ///   movi S0, TRAP_CODE_OUT_OF_FUEL
    ///   s32i S0, rv, TRAP
    ///   j    <trap_label>       # abort: epilogue restores state
    ///   addi S0, S0, -1         # decrement=true only
    ///   s32i S0, rv, FUEL       # decrement=true only
    /// ```
    ///
    /// The trap block is three fixed 3-byte instructions (`j` is always
    /// 3 bytes with ±128 KB reach), so the `bnez` skip is the constant +8.
    fn emit_fuel_check(
        &mut self,
        output: &AllocOutput,
        inst_idx: usize,
        decrement: bool,
        trap_label: LabelId,
        src_op: Option<u32>,
    ) -> Result<(), AllocError> {
        let fuel_off = lpvm::VMCTX_OFFSET_FUEL as i32;
        let trap_off = lpvm::VMCTX_OFFSET_TRAP as i32;
        if !imm::is_legal(ImmOp::L32i, fuel_off) || !imm::is_legal(ImmOp::S32i, trap_off) {
            return Err(crate::emit_err!(
                "vmctx fuel/trap offsets out of l32i/s32i range"
            ));
        }
        let trap_code = lpvm::TRAP_CODE_OUT_OF_FUEL as i32;
        if !imm::is_legal(ImmOp::Movi, trap_code) {
            return Err(crate::emit_err!("trap code out of movi range"));
        }
        // Reload temp is S1 so the expansion's own scratch (S0) never aliases
        // the vmctx register (rv32 uses TEMP2 for the same reason).
        let rv = self.use_vreg(output, inst_idx, 0, S1, src_op)?;
        self.inst(Inst::Load(LoadOp::L32i, S0, rv, fuel_off as u32), src_op);
        debug_assert!(imm::is_legal(ImmOp::Branch12Disp, 8));
        self.inst(Inst::BranchZ(BrZ::Bnez, S0, 8), src_op);
        self.inst(Inst::Movi(S0, trap_code), src_op);
        self.inst(Inst::Store(StoreOp::S32i, S0, rv, trap_off as u32), src_op);
        self.push_item(Item::Jump { label: trap_label }, src_op);
        if decrement {
            self.inst(Inst::Addi(S0, S0, -1), src_op);
            self.inst(Inst::Store(StoreOp::S32i, S0, rv, fuel_off as u32), src_op);
        }
        Ok(())
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
            VInst::AluRRR { op, .. } => self.emit_alu_rrr(output, inst_idx, *op, src_op)?,
            VInst::AluRRI { op, imm, .. } => {
                self.emit_alu_rri(output, inst_idx, *op, *imm, src_op)?;
            }
            VInst::Neg { .. } => {
                if Self::is_dead_def(output, inst_idx, 0) {
                    return Ok(());
                }
                let s = self.use_vreg(output, inst_idx, 1, S0, src_op)?;
                let rd = self.def_vreg(output, inst_idx, 0, S0)?;
                self.inst(Inst::Rt(AluRt::Neg, rd, s), src_op);
                self.store_def_vreg(output, inst_idx, 0, S0, src_op)?;
            }
            VInst::Bnot { .. } => {
                // No `not` and no xor-immediate: materialize -1 and xor.
                if Self::is_dead_def(output, inst_idx, 0) {
                    return Ok(());
                }
                let s = self.use_vreg(output, inst_idx, 1, S0, src_op)?;
                let rd = self.def_vreg(output, inst_idx, 0, S0)?;
                self.iconst(S1, -1, src_op);
                self.inst(Inst::Rrr(AluRrr::Xor, rd, s, S1), src_op);
                self.store_def_vreg(output, inst_idx, 0, S0, src_op)?;
            }
            VInst::Icmp { cond, .. } => self.emit_icmp(output, inst_idx, *cond, src_op)?,
            VInst::IcmpImm { imm, cond, .. } => {
                self.emit_icmp_imm(output, inst_idx, *imm, *cond, src_op)?;
            }
            VInst::Select { .. } => self.emit_select(output, inst_idx, src_op)?,
            VInst::Br { target, .. } => {
                self.push_item(Item::Jump { label: *target }, src_op);
            }
            VInst::BrIf { target, invert, .. } => {
                let c = self.use_vreg(output, inst_idx, 0, S0, src_op)?;
                self.push_item(
                    Item::CondBr {
                        nez: !invert,
                        reg: c,
                        label: *target,
                    },
                    src_op,
                );
            }
            VInst::Mov { .. } => {
                if Self::is_dead_def(output, inst_idx, 0) {
                    return Ok(());
                }
                let s = self.use_vreg(output, inst_idx, 1, S0, src_op)?;
                if let Alloc::Stack(slot) = Self::operand_alloc(output, inst_idx, 0) {
                    // Store the source directly to the spill slot (rv32's
                    // mov-to-spill shortcut).
                    self.spill_store(s, slot, src_op)?;
                } else {
                    let rd = self.def_vreg(output, inst_idx, 0, S0)?;
                    self.mov(rd, s, src_op);
                }
            }
            VInst::Load32 { offset, .. } => {
                self.emit_load(output, inst_idx, *offset, LoadOp::L32i, ImmOp::L32i, src_op)?;
            }
            VInst::Load8U { offset, .. } => {
                self.emit_load(output, inst_idx, *offset, LoadOp::L8ui, ImmOp::L8ui, src_op)?;
            }
            VInst::Load8S { offset, .. } => {
                // No l8si on Xtensa: zero-extending load + sign-extend from
                // bit 7 (`sext` position 7 is legal per the table).
                if Self::is_dead_def(output, inst_idx, 0) {
                    return Ok(());
                }
                let b = self.use_vreg(output, inst_idx, 1, S0, src_op)?;
                let (b, o) = self.mem_addr(b, *offset, ImmOp::L8ui, src_op)?;
                let rd = self.def_vreg(output, inst_idx, 0, S0)?;
                self.inst(Inst::Load(LoadOp::L8ui, rd, b, o), src_op);
                debug_assert!(imm::is_legal(ImmOp::SextBit, 7));
                self.inst(Inst::Sext(rd, rd, 7), src_op);
                self.store_def_vreg(output, inst_idx, 0, S0, src_op)?;
            }
            VInst::Load16U { offset, .. } => {
                self.emit_load(
                    output,
                    inst_idx,
                    *offset,
                    LoadOp::L16ui,
                    ImmOp::L16ui,
                    src_op,
                )?;
            }
            VInst::Load16S { offset, .. } => {
                self.emit_load(
                    output,
                    inst_idx,
                    *offset,
                    LoadOp::L16si,
                    ImmOp::L16si,
                    src_op,
                )?;
            }
            VInst::Store32 { offset, .. } => {
                self.emit_store(
                    output,
                    inst_idx,
                    *offset,
                    StoreOp::S32i,
                    ImmOp::S32i,
                    src_op,
                )?;
            }
            VInst::Store8 { offset, .. } => {
                self.emit_store(output, inst_idx, *offset, StoreOp::S8i, ImmOp::S8i, src_op)?;
            }
            VInst::Store16 { offset, .. } => {
                self.emit_store(
                    output,
                    inst_idx,
                    *offset,
                    StoreOp::S16i,
                    ImmOp::S16i,
                    src_op,
                )?;
            }
            VInst::IConst32 { val, .. } => {
                if Self::is_dead_def(output, inst_idx, 0) {
                    return Ok(());
                }
                let rd = self.def_vreg(output, inst_idx, 0, S0)?;
                self.iconst(rd, *val, src_op);
                self.store_def_vreg(output, inst_idx, 0, S0, src_op)?;
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
                let rd = self.def_vreg(output, inst_idx, 0, S0)?;
                self.add_imm(rd, SP, off, S1, src_op)?;
                self.store_def_vreg(output, inst_idx, 0, S0, src_op)?;
            }
            VInst::MemcpyWords { size, .. } => {
                self.emit_memcpy(output, inst_idx, *size, src_op)?;
            }
            VInst::Call {
                target,
                args,
                rets,
                callee_uses_sret,
                caller_passes_sret_ptr,
                caller_sret_vm_abi_swap,
                ..
            } => {
                self.emit_call(
                    output,
                    inst_idx,
                    *target,
                    args.len(),
                    rets.len(),
                    *callee_uses_sret,
                    *caller_passes_sret_ptr,
                    *caller_sret_vm_abi_swap,
                    src_op,
                )?;
            }
            VInst::Ret { vals, .. } => {
                self.emit_ret(output, inst_idx, vals.len(), is_sret, src_op)?;
            }
            VInst::Label(id, _) => self.record_label(*id, src_op)?,
            VInst::FuelCheck {
                decrement,
                trap_label,
                ..
            } => {
                self.emit_fuel_check(output, inst_idx, *decrement, *trap_label, src_op)?;
            }
            // Hardware float — the float half lives in `super::emit_fp`.
            #[cfg(feature = "float-f32")]
            VInst::FAluRRR { .. }
            | VInst::FAluRR { .. }
            | VInst::Fcmp { .. }
            | VInst::FSelect { .. }
            | VInst::FLoad32 { .. }
            | VInst::FStore32 { .. }
            | VInst::Wfr { .. }
            | VInst::Rfr { .. }
            | VInst::IToF { .. } => {
                self.emit_float_vinst(vinst, output, inst_idx, src_op)?;
            }
            // Without `float-f32` there is no FP emitter linked, so a float
            // VInst reaching here means lowering produced instructions this
            // build cannot encode, and the only safe answer is to refuse
            // loudly. Erroring rather than skipping matters: a silently dropped
            // FP instruction leaves the destination holding whatever was there
            // before and renders a plausible wrong frame.
            #[cfg(not(feature = "float-f32"))]
            VInst::FAluRRR { .. }
            | VInst::FAluRR { .. }
            | VInst::Fcmp { .. }
            | VInst::FSelect { .. }
            | VInst::FLoad32 { .. }
            | VInst::FStore32 { .. }
            | VInst::Wfr { .. }
            | VInst::Rfr { .. }
            | VInst::IToF { .. } => {
                return Err(crate::emit_err!(
                    "xt emitter: {} needs the `float-f32` feature (M7 D9)",
                    vinst.mnemonic()
                ));
            }
        }
        Ok(())
    }

    fn emit_load(
        &mut self,
        output: &AllocOutput,
        inst_idx: usize,
        offset: i32,
        op: LoadOp,
        imm_op: ImmOp,
        src_op: Option<u32>,
    ) -> Result<(), AllocError> {
        if Self::is_dead_def(output, inst_idx, 0) {
            return Ok(());
        }
        let b = self.use_vreg(output, inst_idx, 1, S0, src_op)?;
        let (b, o) = self.mem_addr(b, offset, imm_op, src_op)?;
        let rd = self.def_vreg(output, inst_idx, 0, S0)?;
        self.inst(Inst::Load(op, rd, b, o), src_op);
        self.store_def_vreg(output, inst_idx, 0, S0, src_op)
    }

    fn emit_store(
        &mut self,
        output: &AllocOutput,
        inst_idx: usize,
        offset: i32,
        op: StoreOp,
        imm_op: ImmOp,
        src_op: Option<u32>,
    ) -> Result<(), AllocError> {
        // Address first (its scratch fallback computes into S0, materializing
        // through S1), then the value reload into S1 — linear, no overlap.
        let b = self.use_vreg(output, inst_idx, 1, S0, src_op)?;
        let (b, o) = self.mem_addr(b, offset, imm_op, src_op)?;
        let s = self.use_vreg(output, inst_idx, 0, S1, src_op)?;
        self.inst(Inst::Store(op, s, b, o), src_op);
        Ok(())
    }

    // --- layout + final encode ---------------------------------------------

    /// Rebuild the label-offset table for the current item offsets.
    fn resolve_labels(&mut self) {
        self.label_offsets.clear();
        self.label_offsets.resize(self.defined_labels.len(), None);
        for s in &self.items {
            if let Item::LabelDef(l) = s.item {
                self.label_offsets[l as usize] = Some(s.offset);
            }
        }
    }

    fn label_offset(&self, l: LabelId) -> Result<u32, AllocError> {
        self.label_offsets
            .get(l as usize)
            .copied()
            .flatten()
            .ok_or(crate::emit_err!("undefined label {l}"))
    }

    fn item_size(item: &Item, long: bool) -> u32 {
        match item {
            Item::Bytes(b) => b.len() as u32,
            Item::LabelDef(_) => 0,
            Item::Jump { .. } | Item::L32r { .. } => 3,
            Item::CondBr { .. } => {
                if long {
                    6
                } else {
                    3
                }
            }
        }
    }

    /// Finish emission: lay out items (iterative branch relaxation), place the
    /// pool behind the entry `j`, and encode everything.
    fn finish(mut self) -> Result<IsaEmitOutput, AllocError> {
        let n_lits = self.literals.len() as u32;
        // Pool prefix: `j` (3) + pad (1) + pool; absent when the pool is empty.
        let code_start = if n_lits == 0 { 0 } else { 4 + 4 * n_lits };

        // Iterative sizing: conditional-branch sizes depend on offsets.
        // Relaxation is monotonic (short -> long only), so this converges.
        let mut converged = false;
        for _ in 0..64 {
            let mut off = code_start;
            for s in &mut self.items {
                s.offset = off;
                off += Self::item_size(&s.item, s.long);
            }
            self.resolve_labels();
            let mut changed = false;
            for i in 0..self.items.len() {
                let s = &self.items[i];
                if s.long {
                    continue;
                }
                if let Item::CondBr { label, .. } = s.item {
                    let target = self.label_offset(label)? as i64;
                    let diff = target - (s.offset as i64 + 4);
                    if !(BRI12_MIN..=BRI12_MAX).contains(&diff) {
                        self.items[i].long = true;
                        changed = true;
                    }
                }
            }
            if !changed {
                converged = true;
                break;
            }
        }
        if !converged {
            return Err(crate::emit_err!("branch relaxation failed to converge"));
        }

        let total = self
            .items
            .last()
            .map_or(code_start, |s| s.offset + Self::item_size(&s.item, s.long));

        let mut code: Vec<u8> = Vec::with_capacity(total as usize + 4);
        let mut relocs: Vec<NativeReloc> = Vec::new();

        if n_lits > 0 {
            // Entry `j` over the pool: target = code_start = PC(0) + 4 + off.
            let j_off = code_start as i32 - 4;
            if !imm::is_legal(ImmOp::JDisp, j_off) {
                return Err(crate::emit_err!("literal pool exceeds J reach"));
            }
            code.extend_from_slice(&encode(&Inst::J(j_off)));
            code.push(0); // never-executed pad byte; pool starts 4-aligned
            for (i, l) in self.literals.iter().enumerate() {
                match l {
                    Literal::Const(v) => code.extend_from_slice(&v.to_le_bytes()),
                    Literal::Sym(id) => {
                        relocs.push(NativeReloc {
                            offset: 4 + 4 * i,
                            symbol: String::from(self.symbols.name(*id)),
                        });
                        code.extend_from_slice(&0u32.to_le_bytes());
                    }
                }
            }
            debug_assert_eq!(code.len() as u32, code_start);
        }

        for s in &self.items {
            let pc = s.offset as i64;
            debug_assert_eq!(code.len() as u32, s.offset);
            match &s.item {
                Item::Bytes(b) => code.extend_from_slice(b),
                Item::LabelDef(_) => {}
                Item::Jump { label } => {
                    let target = self.label_offset(*label)? as i64;
                    let off = target - (pc + 4);
                    if !imm::is_legal(ImmOp::JDisp, off as i32) {
                        return Err(crate::emit_err!("J offset {off} out of range"));
                    }
                    code.extend_from_slice(&encode(&Inst::J(off as i32)));
                }
                Item::CondBr { nez, reg, label } => {
                    let target = self.label_offset(*label)? as i64;
                    let kind = |nez: bool| if nez { BrZ::Bnez } else { BrZ::Beqz };
                    if !s.long {
                        let diff = target - (pc + 4);
                        debug_assert!((BRI12_MIN..=BRI12_MAX).contains(&diff));
                        code.extend_from_slice(&encode(&Inst::BranchZ(
                            kind(*nez),
                            *reg,
                            diff as i32,
                        )));
                    } else {
                        // Inverted branch over `j target` (the branch skips
                        // the 3-byte J: target = PC + 4 + 2).
                        code.extend_from_slice(&encode(&Inst::BranchZ(kind(!*nez), *reg, 2)));
                        let off = target - (pc + 3 + 4);
                        if !imm::is_legal(ImmOp::JDisp, off as i32) {
                            return Err(crate::emit_err!("relaxed branch J offset out of range"));
                        }
                        code.extend_from_slice(&encode(&Inst::J(off as i32)));
                    }
                }
                Item::L32r { rt, lit } => {
                    // target = ((PC + 3) & !3) + (one_extend(imm16) << 2).
                    // The 16-bit field is ONE-extended (value = field -
                    // 0x10000): the legal displacement is -262144..=-4 bytes,
                    // and pool-before-code makes every displacement backward.
                    let lit_off = (4 + 4 * *lit) as i64;
                    let base = (pc + 3) & !3;
                    let disp = lit_off - base;
                    if !imm::is_legal(ImmOp::L32rDisp, disp as i32) {
                        return Err(crate::emit_err!(
                            "L32R displacement {disp} outside the backward range -262144..=-4"
                        ));
                    }
                    let field = ((disp >> 2) & 0xFFFF) as u16;
                    code.extend_from_slice(&encode(&Inst::L32r(*rt, field)));
                }
            }
        }
        debug_assert_eq!(code.len() as u32, total);

        // Pad the blob to a 4-byte multiple so concatenated functions keep
        // their pools word-aligned (never executed — code ends in `retw`).
        while !code.len().is_multiple_of(4) {
            code.push(0);
        }

        let debug_lines = self
            .marks
            .iter()
            .map(|&(idx, within, op)| (self.items[idx].offset + within, Some(op)))
            .collect();

        Ok(IsaEmitOutput {
            code,
            relocs,
            debug_lines,
        })
    }
}

/// Highest label id the VInst stream names, so synthetic labels never collide.
fn first_free_label(vinsts: &[VInst]) -> LabelId {
    let mut max: Option<LabelId> = None;
    for v in vinsts {
        let id = match v {
            VInst::Label(id, _) => Some(*id),
            VInst::Br { target, .. } | VInst::BrIf { target, .. } => Some(*target),
            VInst::FuelCheck { trap_label, .. } => Some(*trap_label),
            _ => None,
        };
        if let Some(id) = id {
            max = Some(max.map_or(id, |m: LabelId| m.max(id)));
        }
    }
    max.map_or(0, |m| m + 1)
}

/// Emit a function to Xtensa machine code (the `IsaTarget::emit_function`
/// backend for the ESP32-S3 / classic-ESP32 target). Same contract as
/// [`crate::isa::rv32::emit::emit_function`].
pub(crate) fn emit_function(
    vinsts: &[VInst],
    vreg_pool: &[VReg],
    output: &AllocOutput,
    frame: FrameLayout,
    symbols: &ModuleSymbols,
    is_sret: bool,
    collect_debug_lines: bool,
) -> Result<crate::isa::IsaEmitOutput, AllocError> {
    log::debug!(
        "[native-xt] emit_function: starting with {} vinsts, {} edits",
        vinsts.len(),
        output.edits.len()
    );
    let mut ctx = EmitContext::new(
        frame,
        symbols,
        vreg_pool,
        first_free_label(vinsts),
        collect_debug_lines,
    );

    ctx.emit_prologue(is_sret)?;

    let mut edit_cursor = 0usize;

    if vinsts.is_empty() {
        while edit_cursor < output.edits.len() {
            let (point, edit) = &output.edits[edit_cursor];
            if *point != EditPoint::Before(0) {
                break;
            }
            ctx.emit_edit(edit, None)?;
            edit_cursor += 1;
        }
    } else {
        for (inst_idx, vinst) in vinsts.iter().enumerate() {
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

    ctx.emit_epilogue();

    let result = ctx.finish()?;
    log::debug!(
        "[native-xt] emit_function: complete, {} bytes emitted",
        result.code.len()
    );
    Ok(result)
}

// ---------------------------------------------------------------------------
// Emulator-backed tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use alloc::vec;

    use lp_xt_emu::{Emulator, RunOutcome};
    use lps_shared::{LpsFnKind, LpsFnSig, LpsType};

    use super::*;
    use crate::abi::PregSet;
    use crate::regalloc::walk::build_operand_layout;
    use crate::vinst::{SRC_OP_NONE, SymbolId, VRegSlice};

    fn v(n: u16) -> VReg {
        VReg(n)
    }

    /// A FrameLayout for a hand-built test function.
    fn frame(spills: u32, lpir: &[(u32, u32)], is_leaf: bool, outgoing: u32) -> FrameLayout {
        let sig = LpsFnSig {
            name: "t".into(),
            return_type: LpsType::Int,
            parameters: vec![],
            kind: LpsFnKind::UserDefined,
        };
        let abi = crate::isa::xt::abi::func_abi_xt(&sig, None);
        FrameLayout::compute(&abi, spills, PregSet::EMPTY, lpir, is_leaf, 0, outgoing)
    }

    /// Build an AllocOutput from a per-vreg allocation map, mirroring the
    /// allocator's operand layout (defs first, then uses).
    fn alloc_output(
        vinsts: &[VInst],
        pool: &[VReg],
        map: &[(u16, Alloc)],
        edits: Vec<(EditPoint, Edit)>,
        spills: u32,
    ) -> AllocOutput {
        let (inst_alloc_offsets, total, _classes) = build_operand_layout(vinsts, pool);
        let mut allocs = vec![Alloc::None; total];
        for (idx, inst) in vinsts.iter().enumerate() {
            let mut ops: Vec<VReg> = Vec::new();
            inst.for_each_def(pool, |r| ops.push(r));
            inst.for_each_use(pool, |r| ops.push(r));
            for (k, r) in ops.iter().enumerate() {
                let a = map
                    .iter()
                    .find(|(vr, _)| *vr == r.0)
                    .map(|(_, a)| *a)
                    .unwrap_or_else(|| panic!("test map missing v{}", r.0));
                allocs[inst_alloc_offsets[idx] as usize + k] = a;
            }
        }
        AllocOutput {
            allocs,
            inst_alloc_offsets,
            edits,
            num_spill_slots: spills,
            trace: crate::regalloc::trace_sink_new(),
        }
    }

    fn emit(
        vinsts: &[VInst],
        pool: &[VReg],
        output: &AllocOutput,
        frame: FrameLayout,
    ) -> IsaEmitOutput {
        let symbols = ModuleSymbols::default();
        emit_function(vinsts, pool, output, frame, &symbols, false, true).expect("emit")
    }

    fn run(code: &[u8], arg: u32) -> u32 {
        let mut emu = Emulator::new();
        match emu.run(code, 0, arg) {
            RunOutcome::Ok(v) => v,
            RunOutcome::Trap(t) => panic!("emulator trap: {t:?}"),
        }
    }

    const NONE: u16 = SRC_OP_NONE;

    fn slice(start: u16, count: u8) -> VRegSlice {
        VRegSlice { start, count }
    }

    /// Return-a-constant (movi path, no pool: entry is the ENTRY opcode).
    #[test]
    fn ret_constant() {
        let pool = [v(0)];
        let vinsts = [
            VInst::IConst32 {
                dst: v(0),
                val: 42,
                src_op: NONE,
            },
            VInst::Ret {
                vals: slice(0, 1),
                src_op: NONE,
            },
        ];
        let out = alloc_output(&vinsts, &pool, &[(0, Alloc::int_reg(3))], vec![], 0);
        let e = emit(&vinsts, &pool, &out, frame(0, &[], true, 0));
        assert!(e.relocs.is_empty());
        assert_eq!(run(&e.code, 0), 42);
    }

    /// A pooled constant forces the `[j][pad][pool][entry]` layout with the
    /// function entry still at offset 0.
    #[test]
    fn add_pooled_constant() {
        let pool = [v(0), v(1), v(2)];
        let vinsts = [
            VInst::IConst32 {
                dst: v(1),
                val: 1_000_000,
                src_op: NONE,
            },
            VInst::AluRRR {
                op: AluOp::Add,
                dst: v(2),
                src1: v(0),
                src2: v(1),
                src_op: NONE,
            },
            VInst::Ret {
                vals: slice(2, 1),
                src_op: NONE,
            },
        ];
        let out = alloc_output(
            &vinsts,
            &pool,
            &[
                (0, Alloc::int_reg(2)),
                (1, Alloc::int_reg(3)),
                (2, Alloc::int_reg(4)),
            ],
            vec![],
            0,
        );
        let e = emit(&vinsts, &pool, &out, frame(0, &[], true, 0));
        // Blob starts with a `j` over the pool (op0 = 6, n = 0).
        assert_eq!(e.code[0] & 0x3f, 0x06, "expected leading j over the pool");
        assert_eq!(run(&e.code, 7), 1_000_007);
        assert_eq!(run(&e.code, 0), 1_000_000);
    }

    /// Branchy loop: sum 1..=n (labels, backward CondBr, Icmp).
    #[test]
    fn loop_sum() {
        let pool = [v(1)];
        let vinsts = [
            VInst::IConst32 {
                dst: v(1),
                val: 0,
                src_op: NONE,
            },
            VInst::IConst32 {
                dst: v(2),
                val: 1,
                src_op: NONE,
            },
            VInst::Label(0, NONE),
            VInst::AluRRR {
                op: AluOp::Add,
                dst: v(1),
                src1: v(1),
                src2: v(2),
                src_op: NONE,
            },
            VInst::AluRRI {
                op: AluImmOp::Addi,
                dst: v(2),
                src: v(2),
                imm: 1,
                src_op: NONE,
            },
            VInst::Icmp {
                dst: v(3),
                lhs: v(2),
                rhs: v(0),
                cond: IcmpCond::LeS,
                src_op: NONE,
            },
            VInst::BrIf {
                cond: v(3),
                target: 0,
                invert: false,
                src_op: NONE,
            },
            VInst::Ret {
                vals: slice(0, 1),
                src_op: NONE,
            },
        ];
        let out = alloc_output(
            &vinsts,
            &pool,
            &[
                (0, Alloc::int_reg(2)),
                (1, Alloc::int_reg(3)),
                (2, Alloc::int_reg(4)),
                (3, Alloc::int_reg(5)),
            ],
            vec![],
            0,
        );
        let e = emit(&vinsts, &pool, &out, frame(0, &[], true, 0));
        assert_eq!(run(&e.code, 10), 55);
        assert_eq!(run(&e.code, 1), 1);
    }

    /// AND with an out-of-range immediate must materialize via the pool
    /// (Xtensa has no andi at all).
    #[test]
    fn andi_materializes() {
        let pool = [v(0), v(1)];
        let vinsts = [
            VInst::AluRRI {
                op: AluImmOp::Andi,
                dst: v(1),
                src: v(0),
                imm: 0x00FF_00FF,
                src_op: NONE,
            },
            VInst::Ret {
                vals: slice(1, 1),
                src_op: NONE,
            },
        ];
        let out = alloc_output(
            &vinsts,
            &pool,
            &[(0, Alloc::int_reg(2)), (1, Alloc::int_reg(3))],
            vec![],
            0,
        );
        let e = emit(&vinsts, &pool, &out, frame(0, &[], true, 0));
        assert_eq!(run(&e.code, 0x1234_5678), 0x0034_0078);
    }

    /// Store/load at an offset beyond s32i/l32i's 1020-byte ceiling goes
    /// through the address-scratch fallback.
    #[test]
    fn large_offset_store_load() {
        let pool = [v(0), v(2)];
        let vinsts = [
            VInst::SlotAddr {
                dst: v(1),
                slot: 0,
                src_op: NONE,
            },
            VInst::Store32 {
                src: v(0),
                base: v(1),
                offset: 1096,
                src_op: NONE,
            },
            VInst::Load32 {
                dst: v(2),
                base: v(1),
                offset: 1096,
                src_op: NONE,
            },
            VInst::Ret {
                vals: slice(1, 1),
                src_op: NONE,
            },
        ];
        let out = alloc_output(
            &vinsts,
            &pool,
            &[
                (0, Alloc::int_reg(2)),
                (1, Alloc::int_reg(3)),
                (2, Alloc::int_reg(4)),
            ],
            vec![],
            0,
        );
        let e = emit(&vinsts, &pool, &out, frame(0, &[(0, 1104)], true, 0));
        assert_eq!(run(&e.code, 0xDEAD_BEEF), 0xDEAD_BEEF);
    }

    /// Spilled operands and allocator edits: value round-trips through a
    /// spill slot (Edit::Move Reg->Stack, then a Ret use from the slot).
    #[test]
    fn spill_roundtrip_via_edit() {
        let pool = [v(0)];
        let vinsts = [VInst::Ret {
            vals: slice(0, 1),
            src_op: NONE,
        }];
        let edits = vec![(
            EditPoint::Before(0),
            Edit::Move {
                from: Alloc::int_reg(2),
                to: Alloc::Stack(0),
            },
        )];
        let out = alloc_output(&vinsts, &pool, &[(0, Alloc::Stack(0))], edits, 1);
        let e = emit(&vinsts, &pool, &out, frame(1, &[], true, 0));
        assert_eq!(run(&e.code, 0xC0FF_EE00), 0xC0FF_EE00);
    }

    /// Edit::LoadIncomingArg reads `[callee SP + frame + fp_offset]` — the
    /// caller's outgoing-arg area (callee SP + ENTRY frame == caller SP).
    #[test]
    fn load_incoming_stack_arg() {
        let pool = [v(0)];
        let vinsts = [VInst::Ret {
            vals: slice(0, 1),
            src_op: NONE,
        }];
        let edits = vec![(
            EditPoint::Before(0),
            Edit::LoadIncomingArg {
                fp_offset: 0,
                to: Alloc::int_reg(3),
            },
        )];
        let out = alloc_output(&vinsts, &pool, &[(0, Alloc::int_reg(3))], edits, 0);
        let e = emit(&vinsts, &pool, &out, frame(0, &[], true, 0));

        let mut emu = Emulator::new();
        let caller_sp = emu.profile.initial_sp();
        emu.mem.load_bytes(caller_sp, &0x1BAD_B002u32.to_le_bytes());
        match emu.run(&e.code, 0, 0) {
            RunOutcome::Ok(got) => assert_eq!(got, 0x1BAD_B002),
            RunOutcome::Trap(t) => panic!("emulator trap: {t:?}"),
        }
    }

    /// Full call path: pooled absolute callee address + l32r + callx8, the
    /// relocation patched through `super::super::link::patch_call_literal`,
    /// argument staged in a10, result read from a10, preserved bank surviving
    /// the call by rotation.
    #[test]
    fn call_through_patched_literal() {
        // Callee: sq(x) = x * x.
        let sq_pool = [v(0), v(1)];
        let sq_vinsts = [
            VInst::AluRRR {
                op: AluOp::Mul,
                dst: v(1),
                src1: v(0),
                src2: v(0),
                src_op: NONE,
            },
            VInst::Ret {
                vals: slice(1, 1),
                src_op: NONE,
            },
        ];
        let sq_out = alloc_output(
            &sq_vinsts,
            &sq_pool,
            &[(0, Alloc::int_reg(2)), (1, Alloc::int_reg(3))],
            vec![],
            0,
        );
        let sq = emit(&sq_vinsts, &sq_pool, &sq_out, frame(0, &[], true, 0));

        // Caller: f(x) = sq(x) + 1. Call operand pool: args=[v0], rets=[v1].
        let mut symbols = ModuleSymbols::default();
        let sym = symbols.intern("sq");
        assert_eq!(sym, SymbolId(0));
        let pool = [v(0), v(1)];
        let vinsts = [
            VInst::Call {
                target: sym,
                args: slice(0, 1),
                rets: slice(1, 1),
                callee_uses_sret: false,
                caller_passes_sret_ptr: false,
                caller_sret_vm_abi_swap: false,
                src_op: NONE,
            },
            VInst::AluRRI {
                op: AluImmOp::Addi,
                dst: v(2),
                src: v(1),
                imm: 1,
                src_op: NONE,
            },
            VInst::Ret {
                vals: slice(2, 1),
                src_op: NONE,
            },
        ];
        // The extra pool entry for AluRRI/Ret vregs.
        let pool = {
            let mut p = pool.to_vec();
            p.push(v(2));
            p
        };
        let out = alloc_output(
            &vinsts,
            &pool,
            &[
                (0, Alloc::int_reg(2)),
                (1, Alloc::int_reg(3)),
                (2, Alloc::int_reg(4)),
            ],
            vec![],
            0,
        );
        let caller = emit_function(
            &vinsts,
            &pool,
            &out,
            frame(0, &[], false, 0),
            &symbols,
            false,
            true,
        )
        .expect("emit caller");
        assert_eq!(caller.relocs.len(), 1);
        assert_eq!(caller.relocs[0].symbol, "sq");

        // "Link": concatenate (caller blob is 4-aligned by contract) and
        // patch the literal slot with the callee's absolute address.
        let mut code = caller.code.clone();
        assert!(code.len().is_multiple_of(4));
        let callee_off = code.len();
        code.extend_from_slice(&sq.code);

        let emu = Emulator::new();
        let target = emu.profile.code_ibus_base() + callee_off as u32;
        let reloc = crate::compile::NativeReloc {
            offset: caller.relocs[0].offset,
            symbol: caller.relocs[0].symbol.clone(),
            r_type: crate::isa::xt::link::R_XTENSA_32,
        };
        crate::isa::xt::link::patch_call_literal(&mut code, &reloc, target).expect("patch");

        assert_eq!(run(&code, 7), 50);
        assert_eq!(run(&code, 0), 1);
    }

    /// FuelCheck: check-then-decrement — with fuel = 2 the loop body runs
    /// exactly twice before the observed-zero check traps to the label.
    #[test]
    fn fuel_check_decrement_traps_after_fuel_runs_out() {
        let fuel_off = lpvm::VMCTX_OFFSET_FUEL as i32;
        let pool = [v(3)];
        let vinsts = [
            VInst::SlotAddr {
                dst: v(1),
                slot: 0,
                src_op: NONE,
            },
            VInst::IConst32 {
                dst: v(2),
                val: 2,
                src_op: NONE,
            },
            VInst::Store32 {
                src: v(2),
                base: v(1),
                offset: fuel_off,
                src_op: NONE,
            },
            VInst::IConst32 {
                dst: v(3),
                val: 0,
                src_op: NONE,
            },
            VInst::Label(0, NONE),
            VInst::FuelCheck {
                vmctx: v(1),
                decrement: true,
                trap_label: 1,
                src_op: NONE,
            },
            VInst::AluRRI {
                op: AluImmOp::Addi,
                dst: v(3),
                src: v(3),
                imm: 1,
                src_op: NONE,
            },
            VInst::Br {
                target: 0,
                src_op: NONE,
            },
            VInst::Label(1, NONE),
            VInst::Ret {
                vals: slice(0, 1),
                src_op: NONE,
            },
        ];
        let out = alloc_output(
            &vinsts,
            &pool,
            &[
                (1, Alloc::int_reg(3)),
                (2, Alloc::int_reg(4)),
                (3, Alloc::int_reg(5)),
            ],
            vec![],
            0,
        );
        let e = emit(&vinsts, &pool, &out, frame(0, &[(0, 16)], true, 0));
        assert_eq!(run(&e.code, 0), 2);
    }

    /// IcmpImm (full condition roster) + Select.
    #[test]
    fn icmp_imm_and_select() {
        let pool = [v(4)];
        let vinsts = [
            VInst::IcmpImm {
                dst: v(1),
                src: v(0),
                imm: 100,
                cond: IcmpCond::GtU,
                src_op: NONE,
            },
            VInst::IConst32 {
                dst: v(2),
                val: 7,
                src_op: NONE,
            },
            VInst::IConst32 {
                dst: v(3),
                val: 9,
                src_op: NONE,
            },
            VInst::Select {
                dst: v(4),
                cond: v(1),
                if_true: v(2),
                if_false: v(3),
                src_op: NONE,
            },
            VInst::Ret {
                vals: slice(0, 1),
                src_op: NONE,
            },
        ];
        let out = alloc_output(
            &vinsts,
            &pool,
            &[
                (0, Alloc::int_reg(2)),
                (1, Alloc::int_reg(3)),
                (2, Alloc::int_reg(4)),
                (3, Alloc::int_reg(5)),
                (4, Alloc::int_reg(6)),
            ],
            vec![],
            0,
        );
        let e = emit(&vinsts, &pool, &out, frame(0, &[], true, 0));
        assert_eq!(run(&e.code, 101), 7);
        assert_eq!(run(&e.code, 100), 9);
        assert_eq!(run(&e.code, 0), 9);
    }

    /// A conditional branch spanning >2 KB of code must relax to the
    /// inverted-branch-over-`j` form (and still run correctly both ways).
    #[test]
    fn branch_relaxation_over_2kb() {
        let mut vinsts = vec![
            VInst::IConst32 {
                dst: v(2),
                val: 9,
                src_op: NONE,
            },
            VInst::BrIf {
                cond: v(0),
                target: 0,
                invert: false,
                src_op: NONE,
            },
        ];
        // ~2100 bytes of filler (each IConst32 is one 3-byte movi).
        for _ in 0..700 {
            vinsts.push(VInst::IConst32 {
                dst: v(1),
                val: 1,
                src_op: NONE,
            });
        }
        vinsts.push(VInst::IConst32 {
            dst: v(2),
            val: 77,
            src_op: NONE,
        });
        vinsts.push(VInst::Label(0, NONE));
        vinsts.push(VInst::Ret {
            vals: slice(0, 1),
            src_op: NONE,
        });
        let pool = [v(2)];
        let out = alloc_output(
            &vinsts,
            &pool,
            &[
                (0, Alloc::int_reg(2)),
                (1, Alloc::int_reg(3)),
                (2, Alloc::int_reg(4)),
            ],
            vec![],
            0,
        );
        let e = emit(&vinsts, &pool, &out, frame(0, &[], true, 0));
        assert_eq!(run(&e.code, 1), 9, "taken long branch skips the fillers");
        assert_eq!(run(&e.code, 0), 77, "fall-through path runs the fillers");
    }

    /// MemcpyWords with register bases: copies exactly, bases restored.
    #[test]
    fn memcpy_words_between_slots() {
        let pool = [v(4)];
        let vinsts = [
            VInst::SlotAddr {
                dst: v(1),
                slot: 0,
                src_op: NONE,
            },
            VInst::SlotAddr {
                dst: v(2),
                slot: 1,
                src_op: NONE,
            },
            VInst::Store32 {
                src: v(0),
                base: v(1),
                offset: 0,
                src_op: NONE,
            },
            VInst::AluRRI {
                op: AluImmOp::Addi,
                dst: v(3),
                src: v(0),
                imm: 1,
                src_op: NONE,
            },
            VInst::Store32 {
                src: v(3),
                base: v(1),
                offset: 4,
                src_op: NONE,
            },
            VInst::MemcpyWords {
                dst_base: v(2),
                src_base: v(1),
                size: 8,
                src_op: NONE,
            },
            VInst::Load32 {
                dst: v(4),
                base: v(2),
                offset: 4,
                src_op: NONE,
            },
            VInst::Ret {
                vals: slice(0, 1),
                src_op: NONE,
            },
        ];
        let out = alloc_output(
            &vinsts,
            &pool,
            &[
                (0, Alloc::int_reg(2)),
                (1, Alloc::int_reg(3)),
                (2, Alloc::int_reg(4)),
                (3, Alloc::int_reg(5)),
                (4, Alloc::int_reg(6)),
            ],
            vec![],
            0,
        );
        let e = emit(&vinsts, &pool, &out, frame(0, &[(0, 8), (1, 8)], true, 0));
        assert_eq!(run(&e.code, 41), 42);
    }

    /// Frames beyond ENTRY's 32760-byte immediate are refused, never
    /// truncated.
    #[test]
    fn oversized_frame_is_refused() {
        let pool = [v(0)];
        let vinsts = [VInst::Ret {
            vals: slice(0, 1),
            src_op: NONE,
        }];
        let out = alloc_output(&vinsts, &pool, &[(0, Alloc::int_reg(2))], vec![], 0);
        let big = frame(0, &[(0, 40_000)], true, 0);
        let symbols = ModuleSymbols::default();
        let r = emit_function(&vinsts, &pool, &out, big, &symbols, false, false);
        assert!(r.is_err(), "40KB frame must be an emit error");
    }
}
