//! Per-group instruction executors, mirroring the lp-riscv-emu split
//! (arith / imm / load_store / branch / jump / call / window / misc). Each
//! module is an `impl Emulator` block; this file only routes a decoded
//! [`Inst`] to the right group.
//!
//! Semantics come from the Xtensa ISA Reference Manual; no QEMU/binutils source
//! was used (see the repo license ADR).

use lp_emu_core::InstClass;
use lp_xt_inst::{AluRrr, FpLsiOp, FpLsxOp, FpRrOp, FpRrrOp, Inst, NullaryNarrowOp, NullaryOp};

use crate::emu::{Emulator, Flow};
use crate::error::Trap;
use crate::trace::Tracer;

/// Map a retired instruction (plus its control-flow outcome) onto
/// [`lp_emu_core::InstClass`]'s cost buckets.
///
/// The bucket names are rv32-flavoured; this mapping reads them by *cost
/// shape*, not mnemonic: windowed calls land in the call buckets, `l32r` is a
/// load, `entry`/`retw` window rotations count as ALU-cheap unless a measured
/// Xtensa cycle model says otherwise. Until such a model exists the default
/// [`lp_emu_core::CycleModel::InstructionCount`] charges 1 per class anyway —
/// this mapping is the seam a measured model will refine, not a claim of
/// silicon-accurate weights.
pub(crate) fn inst_class(inst: &Inst, flow: &Flow) -> InstClass {
    match inst {
        Inst::Rrr(op, ..) => match op {
            AluRrr::Mull | AluRrr::Muluh | AluRrr::Mulsh | AluRrr::Mul16u | AluRrr::Mul16s => {
                InstClass::Mul
            }
            AluRrr::Quou | AluRrr::Quos | AluRrr::Remu | AluRrr::Rems => InstClass::DivRem,
            _ => InstClass::Alu,
        },
        Inst::Rt(..)
        | Inst::Rs(..)
        | Inst::ShiftSet(..)
        | Inst::Ssai(..)
        | Inst::Slli(..)
        | Inst::Srli(..)
        | Inst::Srai(..)
        | Inst::Extui(..)
        | Inst::Sext(..)
        | Inst::AddN(..)
        | Inst::MovN(..)
        | Inst::Movi(..)
        | Inst::MoviN(..)
        | Inst::Addi(..)
        | Inst::AddiN(..)
        | Inst::Addmi(..)
        | Inst::Entry(..) => InstClass::Alu,
        Inst::Load(..) | Inst::L32iN(..) | Inst::L32r(..) => InstClass::Load,
        Inst::Store(..) | Inst::S32iN(..) => InstClass::Store,
        Inst::BranchRr(..)
        | Inst::BranchRi(..)
        | Inst::BranchRiu(..)
        | Inst::BranchZ(..)
        | Inst::BranchBiI(..)
        | Inst::BranchZN(..) => match flow {
            Flow::Jump(_) => InstClass::BranchTaken,
            _ => InstClass::BranchNotTaken,
        },
        Inst::J(..) => InstClass::JalTail,
        Inst::Jx(..) => InstClass::JalrIndirect,
        Inst::Call(..) => InstClass::JalCall,
        Inst::Callx(..) => InstClass::JalrCall,
        Inst::Nullary(NullaryOp::Ret | NullaryOp::Retw)
        | Inst::NullaryN(NullaryNarrowOp::RetN | NullaryNarrowOp::RetwN) => InstClass::JalrReturn,
        Inst::Nullary(NullaryOp::Nop) | Inst::NullaryN(NullaryNarrowOp::NopN) => InstClass::Alu,
        Inst::Nullary(_) | Inst::NullaryN(_) => InstClass::System,

        // Float buckets (M6 D8). They carry no measured cost — see the
        // `InstClass` doc comments — and exist so a future measured Xtensa model
        // has somewhere to land instead of being folded into `Alu`.
        Inst::FpLsi(FpLsiOp::Lsi | FpLsiOp::Lsip, ..) => InstClass::Load,
        Inst::FpLsi(FpLsiOp::Ssi | FpLsiOp::Ssip, ..) => InstClass::Store,
        Inst::FpLsx(FpLsxOp::Lsx | FpLsxOp::Lsxp, ..) => InstClass::Load,
        Inst::FpLsx(FpLsxOp::Ssx | FpLsxOp::Ssxp, ..) => InstClass::Store,
        Inst::FpRrr(FpRrrOp::MaddS | FpRrrOp::MsubS | FpRrrOp::MaddnS | FpRrrOp::DivnS, ..) => {
            InstClass::FloatMulAdd
        }
        Inst::FpRrr(..) => InstClass::FloatArith,
        Inst::FpRr(
            FpRrOp::Recip0S
            | FpRrOp::Sqrt0S
            | FpRrOp::Rsqrt0S
            | FpRrOp::Div0S
            | FpRrOp::Nexp01S
            | FpRrOp::MkdadjS
            | FpRrOp::AddexpS
            | FpRrOp::AddexpmS,
            ..,
        ) => InstClass::FloatEstimate,
        // `mov.s`/`abs.s`/`neg.s` are bit operations, not arithmetic.
        Inst::FpRr(..) | Inst::FpMovAr(..) | Inst::FpMovBr(..) | Inst::ConstS(..) => {
            InstClass::FloatArith
        }
        Inst::FpCmp(..) => InstClass::FloatCompare,
        Inst::FpToInt(..) | Inst::IntToFp(..) => InstClass::FloatConvert,
        // Register-file transfers and the Boolean-option AR move are integer
        // data movement, whatever file they read.
        Inst::Rfr(..) | Inst::Wfr(..) | Inst::MovBool(..) => InstClass::Alu,
        Inst::BranchBool(..) => match flow {
            Flow::Jump(_) => InstClass::BranchTaken,
            _ => InstClass::BranchNotTaken,
        },
        Inst::Sr(..) | Inst::Ur(..) => InstClass::System,
    }
}

mod arith;
mod branch;
mod call;
mod float;
mod float_math;
mod imm;
mod jump;
mod load_store;
mod misc;
mod window;

impl Emulator {
    /// Execute one decoded instruction. `pc`/`len` describe the current
    /// instruction; the returned [`Flow`] tells the run loop how to advance.
    pub(crate) fn execute(
        &mut self,
        inst: &Inst,
        pc: u32,
        tracer: &mut dyn Tracer,
    ) -> Result<Flow, Trap> {
        match inst {
            // --- arithmetic / logical / shift (register + register-immediate shifts) ---
            Inst::Rrr(..)
            | Inst::Rt(..)
            | Inst::Rs(..)
            | Inst::ShiftSet(..)
            | Inst::Ssai(..)
            | Inst::Slli(..)
            | Inst::Srli(..)
            | Inst::Srai(..)
            | Inst::Extui(..)
            | Inst::Sext(..)
            | Inst::AddN(..)
            | Inst::MovN(..) => self.exec_arith(inst, tracer),

            // --- immediate / move ---
            Inst::Movi(..)
            | Inst::MoviN(..)
            | Inst::Addi(..)
            | Inst::AddiN(..)
            | Inst::Addmi(..) => self.exec_imm(inst, tracer),

            // --- load / store (incl. l32r literal load) ---
            Inst::Load(..)
            | Inst::Store(..)
            | Inst::L32iN(..)
            | Inst::S32iN(..)
            | Inst::L32r(..) => self.exec_load_store(inst, pc, tracer),

            // --- conditional branches ---
            Inst::BranchRr(..)
            | Inst::BranchRi(..)
            | Inst::BranchRiu(..)
            | Inst::BranchZ(..)
            | Inst::BranchBiI(..)
            | Inst::BranchZN(..) => self.exec_branch(inst, pc),

            // --- unconditional jumps ---
            Inst::J(..) | Inst::Jx(..) => self.exec_jump(inst, pc),

            // --- calls ---
            Inst::Call(..) | Inst::Callx(..) => self.exec_call(inst, pc, tracer),

            // --- window management ---
            Inst::Entry(..) => self.exec_entry(inst, tracer),
            Inst::Nullary(NullaryOp::Retw) | Inst::NullaryN(NullaryNarrowOp::RetwN) => {
                self.exec_retw(tracer)
            }
            Inst::Nullary(NullaryOp::Ret) | Inst::NullaryN(NullaryNarrowOp::RetN) => {
                Ok(self.exec_ret())
            }

            // --- misc / barriers / nops / illegal ---
            Inst::Nullary(_) | Inst::NullaryN(_) => self.exec_misc(inst, pc),

            // --- floating point, boolean, special registers ---
            Inst::FpRrr(..)
            | Inst::FpRr(..)
            | Inst::ConstS(..)
            | Inst::Rfr(..)
            | Inst::Wfr(..)
            | Inst::FpMovAr(..)
            | Inst::FpMovBr(..)
            | Inst::FpCmp(..)
            | Inst::FpToInt(..)
            | Inst::IntToFp(..)
            | Inst::FpLsx(..)
            | Inst::FpLsi(..)
            | Inst::MovBool(..)
            | Inst::BranchBool(..)
            | Inst::Sr(..)
            | Inst::Ur(..) => self.exec_float(inst, pc, tracer),
        }
    }
}
