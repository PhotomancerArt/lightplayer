//! The float half of the Xtensa emitter: every float `VInst`, and the float
//! spill/reload edits, encoded through `lp-xt-inst`'s FP model.
//!
//! Gated on `float-f32` (M7 D9) — a Fixed-only image links none of this. The
//! integer half is [`super::emit`], which owns the layout contract, the
//! literal pool, branch relaxation and the frame; this module adds no
//! machinery of its own and, like its sibling, **never packs bytes**.
//!
//! ## The FP register model
//!
//! Values live in `f0`–`f15` ([`super::fpr`]), a flat file the window rotation
//! does not touch. Fifteen are allocatable; `f15` is the emitter's scratch,
//! the FR counterpart of `a8`, needed because the allocator can place a float
//! *destination* on the stack and an FP instruction still has to write into a
//! register before it can be stored. Address-register scratch stays `a8`/`a9`
//! throughout, so the two files' scratch reservations never interact.
//!
//! Float values cross every call and function boundary in **address**
//! registers as raw IEEE bit patterns (M7 D1/D2). Lowering, not this module,
//! inserts the [`VInst::Wfr`]/[`VInst::Rfr`] transfers that implement it; all
//! this module knows is how to encode them.
//!
//! ## The Boolean-register invariant
//!
//! FP compares write a Boolean register, never an address register. M7 uses
//! one fixed BR — [`fpr::CMP_BREG`] (`b0`) — as an implicit scratch and
//! materializes the 0/1 into the compare's integer destination inside the same
//! emitted sequence, so the allocator never learns Boolean registers exist
//! (D5).
//!
//! **Invariant: no Boolean register is ever live across a `VInst` boundary.**
//!
//! That single sentence is what makes one fixed `b0` safe rather than a source
//! of aliasing. Every sequence below that sets `b0` also consumes it before it
//! returns. A future fused compare-and-branch optimization — the obvious next
//! step, since `bt`/`bf` can branch on `b0` directly and would save the two
//! `movi`s — must either preserve this invariant or teach the allocator about
//! the Boolean file; it may not quietly extend a `b0` live range across an
//! instruction boundary, because the very next compare would clobber it.
//!
//! ## The `lsi`/`ssi` immediate discipline
//!
//! `lsi`/`ssi` offsets are `0..=1020` and must be a multiple of 4 — the
//! encoding holds `offset / 4` in an 8-bit field. `lp-xt-inst`'s encoder
//! computes `(offset / 4) & 0xff` with **no range check**, so an out-of-range
//! offset does not fail: it silently encodes a *different slot*. A frame with
//! more than 255 spill slots below a float access would read or write the
//! wrong four bytes and produce a plausible wrong number, with nothing in the
//! disassembly looking unusual.
//!
//! So every float memory access here goes through
//! [`ImmOp::FpLsiOffset`](super::imm::ImmOp::FpLsiOffset) and takes that
//! table's `AddressScratch` fallback when the offset does not fit: compute
//! `base + offset` into `a8` (materializing through `a9`), then access at
//! offset 0. There is no path from this module to an unchecked FP offset.
//!
//! ## No frame change accompanies FP
//!
//! `FrameLayout`, the prologue's single `entry` and the epilogue's single
//! `retw` are untouched by float support, and that is a *derived* fact, not an
//! omission: no FR is callee-saved (measured, M6-P4), so there is no FP
//! callee-save region to lay out. Float spills are 4 bytes in the existing
//! class-tagged spill index space at the **bottom** of the frame, while the
//! window-overflow handler scribbles in the reservation at the **top**. They
//! cannot collide (M7 D7), and `tests/xt_pipeline_f32.rs` pins that rather
//! than leaving it argued.

use lp_xt_inst::{BReg, FReg, FpCmpOp, FpLsiOp, FpMovArOp, FpRrOp, FpRrrOp, Inst, IntToFpOp, Reg};

use super::emit::{EmitContext, S0, S1, SP};
use super::fpr;
use super::imm::{self, ImmOp};
use crate::abi::{PackedPReg, RegClass};
use crate::regalloc::{Alloc, AllocError, AllocOutput, Edit};
use crate::vinst::{FAluOp, FAluRROp, FcmpCond, VInst};

/// The emitter's float scratch as an encoder operand.
const F_SCRATCH: FReg = FReg::new(fpr::SCRATCH);

/// The Boolean register FP compares write, as an encoder operand.
const B_CMP: BReg = BReg::new(fpr::CMP_BREG);

impl EmitContext<'_> {
    // --- the class gate, float side ---------------------------------------

    /// The `f`-register named by a register allocation.
    ///
    /// The mirror of [`EmitContext::hw`], and the reason that one keeps
    /// rejecting `RegClass::Float` instead of learning to unwrap it: `a3` and
    /// `f3` share a hardware index but are different registers in different
    /// files, so a class confusion is not a crash, it is a bit pattern
    /// reinterpreted between the integer and float worlds. Two narrow gates
    /// that each reject the other class turn that into an emit error at the
    /// exact operand that was wrong.
    pub(super) fn fhw(preg: PackedPReg) -> Result<FReg, AllocError> {
        match preg.class() {
            RegClass::Int => Err(crate::emit_err!(
                "allocation names address register a{} where a float register is required",
                preg.hw()
            )),
            RegClass::Float if preg.hw() < fpr::FR_COUNT => Ok(FReg::new(preg.hw())),
            RegClass::Float => Err(crate::emit_err!("allocation names non-FR f{}", preg.hw())),
        }
    }

    /// Use a float vreg: return the FR holding it.
    ///
    /// Unlike the integer [`EmitContext::use_vreg`] there is no reload path,
    /// because there is nothing to reload: `regalloc::walk::alloc_use` returns
    /// `Alloc::Reg` on every path it has, inserting a reload *edit* before the
    /// instruction when the value was spilled. The only code that forces a use
    /// to the stack is the sret `Ret` constraint, and no float `VInst` is a
    /// `Ret` — float values reach a return through an `Rfr` first (D1).
    ///
    /// So a `Stack` use here means the allocator's contract changed, and the
    /// honest answer is to say so. Reloading into the single float scratch
    /// instead would be wrong the moment an instruction had two spilled float
    /// uses: the second reload would overwrite the first.
    fn fuse_vreg(
        &mut self,
        output: &AllocOutput,
        inst_idx: usize,
        operand_idx: usize,
    ) -> Result<FReg, AllocError> {
        match Self::operand_alloc(output, inst_idx, operand_idx) {
            Alloc::Reg(preg) => Self::fhw(preg),
            Alloc::Stack(slot) => Err(crate::emit_err!(
                "float use operand {operand_idx} allocated to spill slot {slot}; \
                 the allocator is expected to reload float uses into registers via edits"
            )),
            Alloc::None => Err(crate::emit_err!()),
        }
    }

    /// Def a float vreg: return the FR to write to. A stack-allocated def
    /// computes into [`F_SCRATCH`] and is stored by [`Self::fstore_def_vreg`].
    fn fdef_vreg(
        &mut self,
        output: &AllocOutput,
        inst_idx: usize,
        operand_idx: usize,
    ) -> Result<FReg, AllocError> {
        match Self::operand_alloc(output, inst_idx, operand_idx) {
            Alloc::Reg(preg) => Self::fhw(preg),
            Alloc::Stack(_) => Ok(F_SCRATCH),
            Alloc::None => Err(crate::emit_err!()),
        }
    }

    /// Store a spilled float def after it was written to [`F_SCRATCH`].
    ///
    /// Must be called *after* the instruction that produced the value, which
    /// is also what makes reusing `a8`/`a9` for the address fallback safe: any
    /// integer operand those held has already been consumed.
    fn fstore_def_vreg(
        &mut self,
        output: &AllocOutput,
        inst_idx: usize,
        operand_idx: usize,
        src_op: Option<u32>,
    ) -> Result<(), AllocError> {
        if let Alloc::Stack(slot) = Self::operand_alloc(output, inst_idx, operand_idx) {
            self.fspill_store(F_SCRATCH, slot, src_op)?;
        }
        Ok(())
    }

    // --- float spill slots -------------------------------------------------

    /// Reduce a float access to an encodable `(base, offset)` pair, taking the
    /// [`ImmOp::FpLsiOffset`] `AddressScratch` fallback when the offset does
    /// not fit. See the module doc for why the check is not optional.
    fn fp_mem_addr(
        &mut self,
        base: Reg,
        offset: i32,
        src_op: Option<u32>,
    ) -> Result<(Reg, u32), AllocError> {
        if imm::is_legal(ImmOp::FpLsiOffset, offset) {
            Ok((base, offset as u32))
        } else {
            // `a8 = base + offset`, materializing the constant through `a9`.
            // Neither can alias `base`: both are reserved from the allocatable
            // pool, so no vreg ever lives in them.
            self.add_imm(S0, base, offset, S1, src_op)?;
            Ok((S0, 0))
        }
    }

    /// `lsi dst, <spill slot>` (SP-relative — FP == SP on Xtensa).
    fn fspill_load(&mut self, dst: FReg, slot: u8, src_op: Option<u32>) -> Result<(), AllocError> {
        let off = self
            .frame
            .spill_offset_from_sp(slot as u32)
            .ok_or(crate::emit_err!())?;
        let (base, o) = self.fp_mem_addr(SP, off, src_op)?;
        self.inst(Inst::FpLsi(FpLsiOp::Lsi, dst, base, o), src_op);
        Ok(())
    }

    /// `ssi src, <spill slot>` (SP-relative).
    fn fspill_store(&mut self, src: FReg, slot: u8, src_op: Option<u32>) -> Result<(), AllocError> {
        let off = self
            .frame
            .spill_offset_from_sp(slot as u32)
            .ok_or(crate::emit_err!())?;
        let (base, o) = self.fp_mem_addr(SP, off, src_op)?;
        self.inst(Inst::FpLsi(FpLsiOp::Ssi, src, base, o), src_op);
        Ok(())
    }

    /// `mov.s`, elided when the registers coincide.
    fn fmov(&mut self, rd: FReg, rs: FReg, src_op: Option<u32>) {
        if rd != rs {
            self.inst(Inst::FpRr(FpRrOp::MovS, rd, rs), src_op);
        }
    }

    // --- allocator edits ---------------------------------------------------

    /// Handle an allocator edit that moves a **float** value, returning
    /// `false` when the edit is not float-class and the integer path should
    /// take it.
    ///
    /// A stack-to-stack move is deliberately left to the integer path even for
    /// floats: it is a bit-for-bit copy of one 4-byte slot to another, and
    /// `l32i`/`s32i` through `a8` move those bytes exactly as `lsi`/`ssi`
    /// would — without consuming the single float scratch, and without the
    /// class information the edit does not carry (both endpoints are
    /// `Alloc::Stack`, which has no class).
    pub(super) fn emit_float_edit(
        &mut self,
        edit: &Edit,
        src_op: Option<u32>,
    ) -> Result<bool, AllocError> {
        let Edit::Move { from, to } = edit else {
            // `LoadIncomingArg` reads a stack-passed parameter word. Float
            // parameters arrive in address registers as bit patterns (D1), so
            // the destination of one is always integer-class; a float
            // destination here would mean the ABI changed underneath lowering.
            if let Edit::LoadIncomingArg { to, .. } = edit
                && matches!(to, Alloc::Reg(p) if p.class() == RegClass::Float)
            {
                return Err(crate::emit_err!(
                    "incoming argument loaded straight into a float register; \
                     float arguments travel in address registers (M7 D1)"
                ));
            }
            return Ok(false);
        };
        match (*from, *to) {
            (Alloc::Reg(src), Alloc::Reg(dst))
                if src.class() == RegClass::Float && dst.class() == RegClass::Float =>
            {
                let (d, s) = (Self::fhw(dst)?, Self::fhw(src)?);
                self.fmov(d, s, src_op);
                Ok(true)
            }
            // A cross-class register move is not something lowering emits:
            // AR↔FR transfers are explicit `Wfr`/`Rfr` VInsts precisely so the
            // allocator sees ordinary same-class copies (D2). Reaching here
            // means a vreg's class changed between def and use.
            (Alloc::Reg(src), Alloc::Reg(dst)) if src.class() != dst.class() => {
                Err(crate::emit_err!(
                    "allocator edit moves between register classes ({:?} -> {:?}); \
                     AR/FR transfers are Wfr/Rfr VInsts, not moves (M7 D2)",
                    src.class(),
                    dst.class()
                ))
            }
            (Alloc::Stack(slot), Alloc::Reg(dst)) if dst.class() == RegClass::Float => {
                let d = Self::fhw(dst)?;
                self.fspill_load(d, slot, src_op)?;
                Ok(true)
            }
            (Alloc::Reg(src), Alloc::Stack(slot)) if src.class() == RegClass::Float => {
                let s = Self::fhw(src)?;
                self.fspill_store(s, slot, src_op)?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    // --- float VInst emission ----------------------------------------------

    /// Emit one float `VInst`. Operand indices follow the allocator's layout:
    /// defs in `for_each_def` order, then uses in `for_each_use` order.
    pub(super) fn emit_float_vinst(
        &mut self,
        vinst: &VInst,
        output: &AllocOutput,
        inst_idx: usize,
        src_op: Option<u32>,
    ) -> Result<(), AllocError> {
        match vinst {
            VInst::FAluRRR { op, .. } => {
                if Self::is_dead_def(output, inst_idx, 0) {
                    return Ok(());
                }
                let s1 = self.fuse_vreg(output, inst_idx, 1)?;
                let s2 = self.fuse_vreg(output, inst_idx, 2)?;
                let d = self.fdef_vreg(output, inst_idx, 0)?;
                let fop = match op {
                    FAluOp::Add => FpRrrOp::AddS,
                    FAluOp::Sub => FpRrrOp::SubS,
                    FAluOp::Mul => FpRrrOp::MulS,
                };
                self.inst(Inst::FpRrr(fop, d, s1, s2), src_op);
                self.fstore_def_vreg(output, inst_idx, 0, src_op)
            }
            VInst::FAluRR { op, .. } => {
                if Self::is_dead_def(output, inst_idx, 0) {
                    return Ok(());
                }
                let s = self.fuse_vreg(output, inst_idx, 1)?;
                let d = self.fdef_vreg(output, inst_idx, 0)?;
                let fop = match op {
                    FAluRROp::Mov => FpRrOp::MovS,
                    FAluRROp::Abs => FpRrOp::AbsS,
                    FAluRROp::Neg => FpRrOp::NegS,
                };
                // `mov.s fr, fr` is the identity; every other form is not, so
                // only the move may be elided.
                if *op == FAluRROp::Mov {
                    self.fmov(d, s, src_op);
                } else {
                    self.inst(Inst::FpRr(fop, d, s), src_op);
                }
                self.fstore_def_vreg(output, inst_idx, 0, src_op)
            }
            VInst::Fcmp { cond, .. } => self.emit_fcmp(output, inst_idx, *cond, src_op),
            VInst::FSelect { .. } => self.emit_fselect(output, inst_idx, src_op),
            VInst::FLoad32 { offset, .. } => {
                if Self::is_dead_def(output, inst_idx, 0) {
                    return Ok(());
                }
                let b = self.use_vreg(output, inst_idx, 1, S0, src_op)?;
                let (b, o) = self.fp_mem_addr(b, *offset, src_op)?;
                let d = self.fdef_vreg(output, inst_idx, 0)?;
                self.inst(Inst::FpLsi(FpLsiOp::Lsi, d, b, o), src_op);
                self.fstore_def_vreg(output, inst_idx, 0, src_op)
            }
            VInst::FStore32 { offset, .. } => {
                let s = self.fuse_vreg(output, inst_idx, 0)?;
                let b = self.use_vreg(output, inst_idx, 1, S0, src_op)?;
                let (b, o) = self.fp_mem_addr(b, *offset, src_op)?;
                self.inst(Inst::FpLsi(FpLsiOp::Ssi, s, b, o), src_op);
                Ok(())
            }
            VInst::Wfr { .. } => {
                if Self::is_dead_def(output, inst_idx, 0) {
                    return Ok(());
                }
                let s = self.use_vreg(output, inst_idx, 1, S0, src_op)?;
                let d = self.fdef_vreg(output, inst_idx, 0)?;
                self.inst(Inst::Wfr(d, s), src_op);
                self.fstore_def_vreg(output, inst_idx, 0, src_op)
            }
            VInst::Rfr { .. } => {
                if Self::is_dead_def(output, inst_idx, 0) {
                    return Ok(());
                }
                let s = self.fuse_vreg(output, inst_idx, 1)?;
                let d = self.def_vreg(output, inst_idx, 0, S0)?;
                self.inst(Inst::Rfr(d, s), src_op);
                self.store_def_vreg(output, inst_idx, 0, S0, src_op)
            }
            VInst::IToF { signed, .. } => {
                if Self::is_dead_def(output, inst_idx, 0) {
                    return Ok(());
                }
                let s = self.use_vreg(output, inst_idx, 1, S0, src_op)?;
                let d = self.fdef_vreg(output, inst_idx, 0)?;
                let op = if *signed {
                    IntToFpOp::FloatS
                } else {
                    IntToFpOp::UfloatS
                };
                // Scale 0: `float.s fr, as, 0` is the plain conversion. The
                // immediate is a binary *post*-scale (divide by 2^imm), and
                // LPIR has no scaled-conversion op to feed it.
                self.inst(Inst::IntToFp(op, d, s, 0), src_op);
                self.fstore_def_vreg(output, inst_idx, 0, src_op)
            }
            _ => Err(crate::emit_err!(
                "emit_float_vinst called with a non-float VInst: {}",
                vinst.mnemonic()
            )),
        }
    }

    /// `Fcmp` — compare into `b0`, then materialize 0/1 into an address
    /// register (D5).
    ///
    /// ```text
    ///   <cmp>.s b0, fs, ft
    ///   movi    a_dst, 0
    ///   movi    a_scr, 1
    ///   movt    a_dst, a_scr, b0      # movf for Ne
    /// ```
    ///
    /// The mapping is fixed by `docs/design/float.md` §3, which makes NaN
    /// behavior *Guaranteed*: ordered compares are false when either operand
    /// is NaN, and `!=` is true. `Gt`/`Ge` swap the operands rather than
    /// inventing predicates the ISA does not have.
    ///
    /// **`Ne` is `oeq.s` consumed with `movf`**, i.e. `!oeq` — "unordered or
    /// unequal", which is true on NaN as float.md requires. M7's plan (D5)
    /// tabulated `ueq.s` with `movf` instead; that computes `!ueq` =
    /// "ordered and unequal", which is *false* on NaN and would have silently
    /// broken the one comparison float.md singles out. The emulator caught it
    /// (`fcmp_is_correct_when_an_operand_is_nan`), and the plan's table is the
    /// thing that was wrong, not this code. The `un.s` predicate the ISA also
    /// offers is not needed: no LPIR condition asks for unorderedness alone.
    ///
    /// This does *not* make `Ne` a negated `Eq` at the `VInst` level — the two
    /// consume the same Boolean with opposite moves, and collapsing them would
    /// mean the emitter re-deriving the sense from context.
    fn emit_fcmp(
        &mut self,
        output: &AllocOutput,
        inst_idx: usize,
        cond: FcmpCond,
        src_op: Option<u32>,
    ) -> Result<(), AllocError> {
        if Self::is_dead_def(output, inst_idx, 0) {
            return Ok(());
        }
        let lhs = self.fuse_vreg(output, inst_idx, 1)?;
        let rhs = self.fuse_vreg(output, inst_idx, 2)?;

        // `movt` moves when the Boolean is set, `movf` when it is clear.
        let (op, swap, on_set) = match cond {
            FcmpCond::Eq => (FpCmpOp::OeqS, false, true),
            FcmpCond::Ne => (FpCmpOp::OeqS, false, false),
            FcmpCond::Lt => (FpCmpOp::OltS, false, true),
            FcmpCond::Le => (FpCmpOp::OleS, false, true),
            FcmpCond::Gt => (FpCmpOp::OltS, true, true),
            FcmpCond::Ge => (FpCmpOp::OleS, true, true),
        };
        let (fs, ft) = if swap { (rhs, lhs) } else { (lhs, rhs) };
        self.inst(Inst::FpCmp(op, B_CMP, fs, ft), src_op);

        let d = self.def_vreg(output, inst_idx, 0, S0)?;
        // The `1` needs a register of its own, distinct from the destination.
        // `d` is `a8` exactly when the def is spilled, so pick the other one.
        let scr = if d == S0 { S1 } else { S0 };
        self.iconst(d, 0, src_op);
        self.iconst(scr, 1, src_op);
        self.inst(Inst::MovBool(on_set, d, scr, B_CMP), src_op);
        // `b0` is dead here: the invariant in the module doc holds for this
        // sequence, and every other sequence in this file leaves it untouched.
        self.store_def_vreg(output, inst_idx, 0, S0, src_op)
    }

    /// `FSelect` — branch-free, via the FP conditional moves keyed on an
    /// address register.
    ///
    /// The obvious two-instruction form (`mov.s dst, if_false` then
    /// `movnez.s dst, if_true, cond`) is wrong when `dst` aliases `if_true`:
    /// the first instruction destroys the value the second is supposed to
    /// select. Inverting the sense in that case fixes it in *one* instruction
    /// rather than adding a scratch copy. The integer `emit_select` has the
    /// same trap, and `div_guard_is_correct_when_dst_aliases_an_operand` is
    /// the precedent for testing all three alias cases rather than reasoning
    /// about them.
    fn emit_fselect(
        &mut self,
        output: &AllocOutput,
        inst_idx: usize,
        src_op: Option<u32>,
    ) -> Result<(), AllocError> {
        if Self::is_dead_def(output, inst_idx, 0) {
            return Ok(());
        }
        let cond = self.use_vreg(output, inst_idx, 1, S0, src_op)?;
        let if_true = self.fuse_vreg(output, inst_idx, 2)?;
        let if_false = self.fuse_vreg(output, inst_idx, 3)?;
        let d = self.fdef_vreg(output, inst_idx, 0)?;

        if d == if_true {
            // dst already holds the true value; overwrite it with the false
            // value only when the condition is zero.
            self.inst(Inst::FpMovAr(FpMovArOp::MoveqzS, d, if_false, cond), src_op);
        } else {
            self.fmov(d, if_false, src_op);
            self.inst(Inst::FpMovAr(FpMovArOp::MovnezS, d, if_true, cond), src_op);
        }
        self.fstore_def_vreg(output, inst_idx, 0, src_op)
    }
}

// ---------------------------------------------------------------------------
// Emulator-backed tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use alloc::string::String;
    use alloc::vec;
    use alloc::vec::Vec;

    use lp_xt_emu::{Emulator, RunOutcome};
    use lp_xt_inst::{NullaryNarrowOp, SpecialReg, SrOp, encode};
    use lps_shared::{LpsFnKind, LpsFnSig, LpsType};

    use super::*;
    use crate::abi::{FrameLayout, PReg, PackedPReg, PregSet};
    use crate::isa::shared::IsaEmitOutput;
    use crate::isa::xt::emit::emit_function;
    use crate::regalloc::EditPoint;
    use crate::regalloc::walk::build_operand_layout;
    use crate::vinst::{ModuleSymbols, SRC_OP_NONE, VReg, VRegSlice};

    const NONE: u16 = SRC_OP_NONE;

    fn v(n: u16) -> VReg {
        VReg(n)
    }

    fn ireg(hw: u8) -> Alloc {
        Alloc::int_reg(hw)
    }

    fn freg(hw: u8) -> Alloc {
        Alloc::reg(PReg::float(hw))
    }

    fn slice(start: u16, count: u8) -> VRegSlice {
        VRegSlice { start, count }
    }

    fn frame(spills: u32) -> FrameLayout {
        frame_with_outgoing(spills, 0)
    }

    /// `outgoing` is the caller's outgoing stack-argument area, which sits at
    /// the *bottom* of the frame and therefore pushes every spill slot's
    /// SP-relative offset up by that much. It is the only lever a test has for
    /// driving a float spill past `lsi`'s reach, because `Alloc::Stack` carries
    /// a `u8` slot index and 255 slots reach exactly 1020 bytes on their own.
    fn frame_with_outgoing(spills: u32, outgoing: u32) -> FrameLayout {
        let sig = LpsFnSig {
            name: "t".into(),
            return_type: LpsType::Int,
            parameters: vec![],
            kind: LpsFnKind::UserDefined,
        };
        let abi = crate::isa::xt::abi::func_abi_xt(&sig, None);
        FrameLayout::compute(
            &abi,
            spills,
            PregSet::EMPTY,
            &[],
            outgoing == 0,
            0,
            outgoing,
        )
    }

    /// Build an `AllocOutput` from a per-vreg allocation map, mirroring the
    /// allocator's operand layout (defs first, then uses) — the same helper
    /// shape `super::emit`'s tests use.
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

    /// Arm the FPU, then fall through into the compiled function.
    ///
    /// `Emulator::run` stages a fresh `Cpu`, and `Cpu::new()` leaves `CPENABLE`
    /// clear **on purpose** — firmware that forgets to arm the coprocessor
    /// faults on the host instead of silently working (M7 D6; the emulator's
    /// own `an_unarmed_windowed_run_reports_a_coprocessor_trap` pins it). So a
    /// host test of compiled float code has to do what P5's board init will do,
    /// and this two-instruction preamble is the smallest honest version of it.
    ///
    /// It runs *before* the function's `ENTRY`, in the caller's window, which
    /// constrains the register it may use: after the rotation the caller's
    /// `a15` becomes the callee's `a7`, the sixth argument register, so this is
    /// safe for any function taking five arguments or fewer (asserted below).
    ///
    /// The preamble is padded to a multiple of 4 bytes because the emitted
    /// blob's literal pool must stay word-aligned — the layout contract in
    /// `super::emit` assumes the blob starts 4-aligned, and prepending an
    /// odd-sized preamble would quietly misalign every `l32r` target.
    fn arm_fpu_preamble() -> Vec<u8> {
        let mut p = Vec::new();
        p.extend_from_slice(&encode(&Inst::Movi(Reg::new(15), 1)));
        p.extend_from_slice(&encode(&Inst::Sr(
            SrOp::Wsr,
            SpecialReg::Cpenable,
            Reg::new(15),
        )));
        while p.len() % 4 != 0 {
            p.extend_from_slice(&encode(&Inst::NullaryN(NullaryNarrowOp::NopN)));
        }
        p
    }

    /// Run a compiled function with the FPU armed, `args` in `a2..`.
    fn run_f(code: &[u8], args: &[u32]) -> u32 {
        assert!(
            args.len() <= 5,
            "the arming preamble clobbers the sixth argument register"
        );
        let mut blob = arm_fpu_preamble();
        blob.extend_from_slice(code);
        let mut emu = Emulator::new();
        match emu.run_with_args(&blob, 0, args) {
            RunOutcome::Ok(v) => v,
            RunOutcome::Trap(t) => panic!("emulator trap: {t:?}"),
        }
    }

    fn bits(f: f32) -> u32 {
        f.to_bits()
    }

    /// `f32(a0) OP f32(a1) -> f32`, through `Wfr` / the op / `Rfr` — the D1
    /// boundary convention, so the test can hand bit patterns in and read one
    /// back out.
    fn binop_code(op: FAluOp) -> IsaEmitOutput {
        let pool = [v(0), v(1), v(2), v(3), v(4)];
        let vinsts = [
            VInst::Wfr {
                dst: v(2),
                src: v(0),
                src_op: NONE,
            },
            VInst::Wfr {
                dst: v(3),
                src: v(1),
                src_op: NONE,
            },
            VInst::FAluRRR {
                op,
                dst: v(4),
                src1: v(2),
                src2: v(3),
                src_op: NONE,
            },
            VInst::Rfr {
                dst: v(0),
                src: v(4),
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
                (0, ireg(2)),
                (1, ireg(3)),
                (2, freg(0)),
                (3, freg(1)),
                (4, freg(2)),
            ],
            vec![],
            0,
        );
        emit(&vinsts, &pool, &out, frame(0))
    }

    #[test]
    fn fadd_fsub_fmul_execute_on_the_emulator() {
        for (op, l, r, want) in [
            (FAluOp::Add, 1.5f32, 2.25f32, 3.75f32),
            (FAluOp::Sub, 1.5, 2.25, -0.75),
            (FAluOp::Mul, 1.5, 2.25, 3.375),
        ] {
            let e = binop_code(op);
            let got = run_f(&e.code, &[bits(l), bits(r)]);
            assert_eq!(
                f32::from_bits(got),
                want,
                "{op:?} {l} {r} gave {}",
                f32::from_bits(got)
            );
        }
    }

    /// float.md §3, a Guaranteed row: `-1.0 * 0.0` is `-0.0` — the sign, not
    /// the magnitude, carries the information.
    #[test]
    fn arithmetic_preserves_signed_zero() {
        let e = binop_code(FAluOp::Mul);
        let got = run_f(&e.code, &[bits(-1.0), bits(0.0)]);
        assert_eq!(got, bits(-0.0), "-1.0 * 0.0 is -0.0, not +0.0");
    }

    /// **Awaiting M6-P6.** NaN *propagation through arithmetic* is a
    /// deliberately-unresolved `lp-xt-emu` policy field (`nan_propagation`,
    /// vector family F2): the emulator panics rather than inventing which
    /// payload an `add.s` returns, because nothing has measured it on silicon.
    /// That is M6's design, not a defect here, and this test is left in place
    /// and marked rather than weakened — un-ignore it when the campaign closes
    /// the field.
    ///
    /// The NaN behaviors M7 *can* assert today are all here already and all
    /// pass: the compare predicates
    /// (`fcmp_is_correct_when_an_operand_is_nan`) and the sign-bit ops'
    /// payload preservation (`fabs_and_fneg_are_sign_bit_operations`), neither
    /// of which reads the policy.
    #[test]
    #[ignore = "awaiting M6-P6: lp-xt-emu's nan_propagation policy field is unresolved"]
    fn arithmetic_propagates_nan() {
        let e = binop_code(FAluOp::Add);
        let got = run_f(&e.code, &[bits(f32::NAN), bits(1.0)]);
        assert!(f32::from_bits(got).is_nan(), "NaN + 1.0 must stay NaN");
    }

    /// `abs.s` / `neg.s` are sign-bit operations: they must not canonicalize a
    /// NaN payload, and they must move a signed zero's sign.
    fn unop_code(op: FAluRROp) -> IsaEmitOutput {
        let pool = [v(0), v(1), v(2)];
        let vinsts = [
            VInst::Wfr {
                dst: v(1),
                src: v(0),
                src_op: NONE,
            },
            VInst::FAluRR {
                op,
                dst: v(2),
                src: v(1),
                src_op: NONE,
            },
            VInst::Rfr {
                dst: v(0),
                src: v(2),
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
            &[(0, ireg(2)), (1, freg(0)), (2, freg(1))],
            vec![],
            0,
        );
        emit(&vinsts, &pool, &out, frame(0))
    }

    #[test]
    fn fabs_and_fneg_are_sign_bit_operations() {
        let abs = unop_code(FAluRROp::Abs);
        assert_eq!(run_f(&abs.code, &[bits(-3.5)]), bits(3.5));
        assert_eq!(run_f(&abs.code, &[bits(-0.0)]), bits(0.0));
        // A NaN with a payload keeps its payload; only the sign bit clears.
        assert_eq!(run_f(&abs.code, &[0xFFC0_1234]), 0x7FC0_1234);

        let neg = unop_code(FAluRROp::Neg);
        assert_eq!(run_f(&neg.code, &[bits(3.5)]), bits(-3.5));
        assert_eq!(run_f(&neg.code, &[bits(0.0)]), bits(-0.0));
        assert_eq!(run_f(&neg.code, &[0x7FC0_1234]), 0xFFC0_1234);

        let mov = unop_code(FAluRROp::Mov);
        assert_eq!(run_f(&mov.code, &[0xFFC0_1234]), 0xFFC0_1234);
    }

    fn fcmp_code(cond: FcmpCond) -> IsaEmitOutput {
        let pool = [v(0), v(1), v(2), v(3), v(4)];
        let vinsts = [
            VInst::Wfr {
                dst: v(2),
                src: v(0),
                src_op: NONE,
            },
            VInst::Wfr {
                dst: v(3),
                src: v(1),
                src_op: NONE,
            },
            VInst::Fcmp {
                dst: v(4),
                lhs: v(2),
                rhs: v(3),
                cond,
                src_op: NONE,
            },
            VInst::Ret {
                vals: slice(4, 1),
                src_op: NONE,
            },
        ];
        let out = alloc_output(
            &vinsts,
            &pool,
            &[
                (0, ireg(2)),
                (1, ireg(3)),
                (2, freg(0)),
                (3, freg(1)),
                (4, ireg(2)),
            ],
            vec![],
            0,
        );
        emit(&vinsts, &pool, &out, frame(0))
    }

    /// All six conditions on ordinary values.
    #[test]
    fn fcmp_covers_all_six_conditions() {
        let cases: &[(FcmpCond, f32, f32, u32)] = &[
            (FcmpCond::Eq, 1.0, 1.0, 1),
            (FcmpCond::Eq, 1.0, 2.0, 0),
            (FcmpCond::Ne, 1.0, 2.0, 1),
            (FcmpCond::Ne, 1.0, 1.0, 0),
            (FcmpCond::Lt, 1.0, 2.0, 1),
            (FcmpCond::Lt, 2.0, 1.0, 0),
            (FcmpCond::Lt, 1.0, 1.0, 0),
            (FcmpCond::Le, 1.0, 1.0, 1),
            (FcmpCond::Le, 2.0, 1.0, 0),
            (FcmpCond::Gt, 2.0, 1.0, 1),
            (FcmpCond::Gt, 1.0, 2.0, 0),
            (FcmpCond::Gt, 1.0, 1.0, 0),
            (FcmpCond::Ge, 1.0, 1.0, 1),
            (FcmpCond::Ge, 1.0, 2.0, 0),
            (FcmpCond::Ge, 2.0, 1.0, 1),
        ];
        for &(cond, l, r, want) in cases {
            let e = fcmp_code(cond);
            assert_eq!(
                run_f(&e.code, &[bits(l), bits(r)]),
                want,
                "{cond:?} {l} {r}"
            );
        }
    }

    /// float.md §3, a *Guaranteed* row: every ordered comparison is false when
    /// an operand is NaN, and `!=` is true. This is why `Ne` is its own
    /// condition rather than a negated `Eq` — the two differ exactly here.
    #[test]
    fn fcmp_is_correct_when_an_operand_is_nan() {
        let nan = f32::NAN;
        for (cond, want) in [
            (FcmpCond::Eq, 0),
            (FcmpCond::Ne, 1),
            (FcmpCond::Lt, 0),
            (FcmpCond::Le, 0),
            (FcmpCond::Gt, 0),
            (FcmpCond::Ge, 0),
        ] {
            let e = fcmp_code(cond);
            for (l, r) in [(nan, 1.0f32), (1.0, nan), (nan, nan)] {
                assert_eq!(
                    run_f(&e.code, &[bits(l), bits(r)]),
                    want,
                    "{cond:?} with NaN ({l} {r})"
                );
            }
        }
    }

    /// The select's three alias cases. `dst == if_true` is the one a naive
    /// `mov.s` + `movnez.s` gets silently wrong, which is why it is tested
    /// rather than reasoned about.
    #[test]
    fn fselect_is_correct_in_every_alias_case() {
        // (dst, if_true, if_false) hardware float registers.
        for (d, t, f) in [(2u8, 0u8, 1u8), (0, 0, 1), (1, 0, 1)] {
            let pool = [v(0), v(1), v(2), v(3), v(4), v(5)];
            let vinsts = [
                VInst::Wfr {
                    dst: v(3),
                    src: v(1),
                    src_op: NONE,
                },
                VInst::Wfr {
                    dst: v(4),
                    src: v(2),
                    src_op: NONE,
                },
                VInst::FSelect {
                    dst: v(5),
                    cond: v(0),
                    if_true: v(3),
                    if_false: v(4),
                    src_op: NONE,
                },
                VInst::Rfr {
                    dst: v(0),
                    src: v(5),
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
                    (0, ireg(2)),
                    (1, ireg(3)),
                    (2, ireg(4)),
                    (3, freg(t)),
                    (4, freg(f)),
                    (5, freg(d)),
                ],
                vec![],
                0,
            );
            let e = emit(&vinsts, &pool, &out, frame(0));
            let got_true = run_f(&e.code, &[1, bits(10.0), bits(20.0)]);
            let got_false = run_f(&e.code, &[0, bits(10.0), bits(20.0)]);
            assert_eq!(
                f32::from_bits(got_true),
                10.0,
                "dst=f{d} if_true=f{t} if_false=f{f}: cond!=0 must select if_true"
            );
            assert_eq!(
                f32::from_bits(got_false),
                20.0,
                "dst=f{d} if_true=f{t} if_false=f{f}: cond==0 must select if_false"
            );
        }
    }

    #[test]
    fn itof_converts_both_signednesses() {
        for (signed, arg, want) in [
            (true, -3i32 as u32, -3.0f32),
            (true, 7, 7.0),
            (false, 0xFFFF_FFFF, 4294967296.0),
            (false, 7, 7.0),
        ] {
            let pool = [v(0), v(1)];
            let vinsts = [
                VInst::IToF {
                    dst: v(1),
                    src: v(0),
                    signed,
                    src_op: NONE,
                },
                VInst::Rfr {
                    dst: v(0),
                    src: v(1),
                    src_op: NONE,
                },
                VInst::Ret {
                    vals: slice(0, 1),
                    src_op: NONE,
                },
            ];
            let out = alloc_output(&vinsts, &pool, &[(0, ireg(2)), (1, freg(0))], vec![], 0);
            let e = emit(&vinsts, &pool, &out, frame(0));
            assert_eq!(f32::from_bits(run_f(&e.code, &[arg])), want);
        }
    }

    // --- spill slots and the `lsi` range fallback --------------------------

    /// `wfr` into a float vreg the allocator put on the stack, then reload it
    /// through an edit and hand it back. Returns the emitted function and the
    /// slot's byte offset from SP.
    fn spill_round_trip(spills: u32, slot: u8, outgoing: u32) -> (IsaEmitOutput, i32) {
        let pool = [v(0), v(1), v(2)];
        let vinsts = [
            // Def straight into a spill slot — the def path's scratch
            // materialization followed by `ssi`.
            VInst::Wfr {
                dst: v(1),
                src: v(0),
                src_op: NONE,
            },
            // Reloaded by an edit into f0, then read back out.
            VInst::FAluRR {
                op: FAluRROp::Mov,
                dst: v(2),
                src: v(1),
                src_op: NONE,
            },
            VInst::Rfr {
                dst: v(0),
                src: v(2),
                src_op: NONE,
            },
            VInst::Ret {
                vals: slice(0, 1),
                src_op: NONE,
            },
        ];
        let mut out = alloc_output(
            &vinsts,
            &pool,
            &[(0, ireg(2)), (1, Alloc::Stack(slot)), (2, freg(1))],
            vec![(
                EditPoint::Before(1),
                Edit::Move {
                    from: Alloc::Stack(slot),
                    to: freg(0),
                },
            )],
            spills,
        );
        // The edit reloads v1 into f0 before instruction 1, so that
        // instruction's use operand names f0, not the slot — exactly what the
        // allocator emits (`alloc_use` never leaves a use on the stack).
        let base = out.inst_alloc_offsets[1] as usize;
        out.allocs[base + 1] = freg(0);
        let f = frame_with_outgoing(spills, outgoing);
        let off = f.spill_offset_from_sp(slot as u32).expect("slot offset");
        (emit(&vinsts, &pool, &out, f), off)
    }

    fn disassemble(code: &[u8]) -> String {
        let mut s = String::new();
        let mut pc = 0usize;
        while pc < code.len() {
            let end = (pc + 3).min(code.len());
            let Ok((inst, len)) = lp_xt_inst::decode(&code[pc..end]) else {
                pc += 1;
                continue;
            };
            s.push_str(&lp_xt_inst::disasm::format_inst(&inst, pc as u32));
            s.push('\n');
            pc += len;
        }
        s
    }

    /// A float value round-tripped through a spill slot whose byte offset
    /// `lsi`/`ssi` can encode directly.
    #[test]
    fn a_float_spill_in_range_uses_lsi_and_ssi_directly() {
        let (e, off) = spill_round_trip(1, 0, 0);
        assert!(imm::is_legal(ImmOp::FpLsiOffset, off));
        let text = disassemble(&e.code);
        assert!(
            text.contains("ssi") && text.contains("lsi"),
            "expected a direct ssi/lsi pair:\n{text}"
        );
        assert_eq!(f32::from_bits(run_f(&e.code, &[bits(12.5)])), 12.5);
    }

    /// The silent-corruption hazard, pinned. `lp-xt-inst`'s encoder computes
    /// `(offset / 4) & 0xff` with no range check, so a slot past 1020 bytes
    /// encodes as a *different slot* unless the emitter takes the address
    /// fallback. The value assertion alone would not catch it — a wrong slot
    /// can hold the right bytes by coincidence in a small test — so the
    /// encoding is asserted too.
    #[test]
    fn a_float_spill_past_lsi_range_takes_the_scratch_fallback() {
        // Slot 255 is the highest an `Alloc::Stack` can name (1020 bytes on
        // its own); a 64-byte outgoing-argument area below it pushes the slot
        // to 1084, past `lsi`'s reach.
        let (e, off) = spill_round_trip(256, 255, 64);
        assert!(
            off > 1020,
            "the test must actually leave lsi range (offset {off})"
        );
        assert!(
            !imm::is_legal(ImmOp::FpLsiOffset, off),
            "offset {off} was expected to be illegal for lsi/ssi"
        );
        let text = disassemble(&e.code);
        // Every ssi/lsi must name offset 0 off the scratch AR, never a
        // truncated immediate field.
        let mut fp_accesses = 0;
        for line in text.lines() {
            if line.contains("ssi") || line.contains("lsi") {
                fp_accesses += 1;
                assert!(
                    line.contains("a8, 0"),
                    "float access did not go through the scratch at offset 0: {line}"
                );
            }
        }
        assert_eq!(fp_accesses, 2, "expected one ssi and one lsi:\n{text}");
        assert_eq!(f32::from_bits(run_f(&e.code, &[bits(12.5)])), 12.5);
    }

    /// Float instructions must render in a disassembly — it is the first thing
    /// anyone reads when a pipeline test fails.
    #[test]
    fn float_instructions_disassemble() {
        let e = binop_code(FAluOp::Mul);
        let text = disassemble(&e.code);
        for want in ["wfr", "mul.s", "rfr"] {
            assert!(text.contains(want), "missing {want} in:\n{text}");
        }
    }

    // --- the class gate ----------------------------------------------------

    /// The integer gate must keep rejecting a float allocation. If it ever
    /// learns to unwrap one, an integer instruction can name `f3` while
    /// meaning `a3`, and the failure is a wrong number rather than a crash.
    #[test]
    fn the_integer_gate_still_rejects_a_float_allocation() {
        let pool = [v(0)];
        let vinsts = [
            VInst::IConst32 {
                dst: v(0),
                val: 1,
                src_op: NONE,
            },
            VInst::Ret {
                vals: slice(0, 1),
                src_op: NONE,
            },
        ];
        let out = alloc_output(&vinsts, &pool, &[(0, freg(3))], vec![], 0);
        let symbols = ModuleSymbols::default();
        emit_function(&vinsts, &pool, &out, frame(0), &symbols, false, true)
            .expect_err("an integer instruction must not accept a float register");
    }

    /// And the float gate rejects an integer allocation — the other direction.
    #[test]
    fn the_float_gate_rejects_an_integer_allocation() {
        assert!(EmitContext::fhw(PackedPReg::int(3)).is_err());
        assert!(EmitContext::fhw(PackedPReg::new(PReg::float(3))).is_ok());
    }
}
