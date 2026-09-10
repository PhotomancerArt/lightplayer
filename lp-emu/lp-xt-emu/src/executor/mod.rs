//! Per-group instruction executors, mirroring the lp-riscv-emu split
//! (arith / imm / load_store / branch / jump / call / window / misc). Each
//! module is an `impl Exec` block; this file only routes a decoded
//! [`Inst`] to the right group.
//!
//! Semantics come from the Xtensa ISA Reference Manual; no QEMU/binutils source
//! was used (see the repo license ADR).

use lp_emu_core::InstClass;
use lp_emu_core::bus::Bus;
use lp_xt_inst::{
    AluRrr, AtomicLsOp, FpLsiOp, FpLsxOp, FpRrOp, FpRrrOp, Inst, NullaryNarrowOp, NullaryOp,
    WindowLsOp,
};

use crate::cpu::Cpu;
use crate::emu::Flow;
use crate::error::{Trap, TrapKind};
use crate::fp_policy::FpPolicy;
use crate::trace::{TraceEvent, Tracer};

/// The per-instruction execution context: everything an executor touches,
/// borrowed rather than owned.
///
/// The executors used to be `impl Emulator` blocks reading `self.cpu`,
/// `self.mem` and `self.fp_policy`. They are the same code, over the same
/// field names — the fields are references now, so every body is unchanged.
/// What changes is who can build one: the user-mode [`crate::Emulator`] builds
/// it from its own `Memory`, once per instruction in its `step`, and a
/// machine-mode hart will build it from the machine's bus the same way (M1
/// P3). Neither owns the other.
///
/// **The view is generic; `Emulator` is not.** That is the whole point of
/// making this a borrow view rather than an `Emulator<B>`: no public signature
/// gains a type parameter, and `SyscallHandler::syscall(&mut self, cpu: &mut
/// Cpu, mem: &mut Memory)` — a user-mode-only concept — stays exactly as
/// written (milestone ruling R6).
///
/// `pub(crate)`, and it stays that way: `Emulator::run_loop` dispatches once
/// into `run_loop_with` **specifically** so both tracer instantiations are
/// codegen'd inside this crate (M6 measured 25 % when that discipline lapsed).
/// A public generic that `lp-xt-elf` could instantiate would reopen exactly
/// that hole.
pub(crate) struct Exec<'a, B: Bus> {
    pub(crate) cpu: &'a mut Cpu,
    pub(crate) mem: &'a mut B,
    pub(crate) fp_policy: &'a mut FpPolicy,
}

/// The trap a machine-mode-only instruction raises in the **user-mode** runner.
///
/// `lp-xt-inst` decodes the whole LX6/LX7 firmware ISA as of M1 P1; this runner
/// implements the user-mode subset and nothing else. A word outside that subset
/// used to fail at *decode* with `EXC_ILLEGAL_INSTRUCTION` (`emu.rs`'s `step`);
/// now it decodes and fails here, with the same cause — so nothing about the
/// observable behaviour of a user-mode payload changes, and the corpus goldens
/// stay byte-identical. Machine-mode semantics for these are M1 P3's.
///
/// `pc` is left 0 for the run loop to fill in, matching the other executors.
pub(crate) fn machine_mode_only() -> Trap {
    Trap {
        kind: TrapKind::Exception,
        cause: crate::error::EXC_ILLEGAL_INSTRUCTION,
        pc: 0,
        vaddr: 0,
    }
}

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
        // Machine-mode-only families (M1 P1 decode, M1 P3 semantics). The
        // user-mode runner traps on all of them; the bucket is still the
        // honest cost shape so a measured model has somewhere to land.
        Inst::AtomicLs(AtomicLsOp::L32ai, ..) => InstClass::Load,
        Inst::Loop(..)
        | Inst::Rf(..)
        | Inst::Rfi(..)
        | Inst::Rsil(..)
        | Inst::Waiti(..)
        | Inst::Rotw(..)
        | Inst::Break(..)
        | Inst::BreakN(..)
        | Inst::Tlb(..)
        | Inst::TlbInv(..)
        | Inst::ExtReg(..)
        | Inst::MacLd(..)
        | Inst::MacLoad(..) => InstClass::System,
        Inst::Mac(..) => InstClass::Mul,
        Inst::Clamps(..) | Inst::BoolLogic(..) | Inst::BoolAll(..) => InstClass::Alu,
        Inst::WindowLs(WindowLsOp::L32e, ..) => InstClass::Load,
        Inst::WindowLs(..) => InstClass::Store,
        Inst::AtomicLs(..) => InstClass::Store,
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
            | FpRrOp::MksadjS
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

impl<B: Bus> Exec<'_, B> {
    // --- small shared helpers used by the executor modules ---

    /// Write windowed register `a{i}` and emit a trace event.
    pub(crate) fn wreg<T: Tracer + ?Sized>(&mut self, i: u8, v: u32, tracer: &mut T) {
        let phys = self.cpu.set_a(i, v);
        tracer.event(TraceEvent::RegWrite {
            index: i,
            phys,
            value: v,
        });
    }

    /// Read windowed register `a{i}`.
    #[inline]
    pub(crate) fn rreg(&self, i: u8) -> u32 {
        self.cpu.a(i)
    }

    /// Write float register `f{i}` (raw bits) and emit a trace event.
    ///
    /// Executors go through here rather than touching `cpu.fr` so that every FP
    /// write is on one traced path — P6 bisects numeric divergences off this
    /// trace, and an intermediate you cannot see is a bad day.
    pub(crate) fn wfreg<T: Tracer + ?Sized>(&mut self, i: u8, bits: u32, tracer: &mut T) {
        self.cpu.set_f(i, bits);
        tracer.event(TraceEvent::FRegWrite { index: i, bits });
    }

    /// Read float register `f{i}` (raw bits).
    #[inline]
    pub(crate) fn rfreg(&self, i: u8) -> u32 {
        self.cpu.f(i)
    }

    /// Write boolean register `b{i}` and emit a trace event.
    pub(crate) fn wbreg<T: Tracer + ?Sized>(&mut self, i: u8, v: bool, tracer: &mut T) {
        self.cpu.set_b(i, v);
        tracer.event(TraceEvent::BRegWrite { index: i, value: v });
    }

    /// Execute one decoded instruction. `pc`/`len` describe the current
    /// instruction; the returned [`Flow`] tells the run loop how to advance.
    ///
    /// Kept for callers that do not need the cost class — the
    /// single-instruction harness path; the run loop uses
    /// [`execute_classed`](Self::execute_classed).
    pub(crate) fn execute<T: Tracer + ?Sized>(
        &mut self,
        inst: &Inst,
        pc: u32,
        tracer: &mut T,
    ) -> Result<Flow, Trap> {
        self.execute_classed(inst, pc, tracer).map(|(flow, _)| flow)
    }

    /// Execute one decoded instruction and report its [`InstClass`] alongside
    /// the [`Flow`].
    ///
    /// The routing match already discriminates the `Inst` down to the group
    /// that knows the cost bucket, so the bucket comes back from here rather
    /// than from a second full walk of the same 51-variant enum on every
    /// retired instruction ([`inst_class`], E-xtensa 1.4/1.7). `inst_class`
    /// stays as the standalone mapping for tests and tools, and the
    /// `debug_assert` below is what keeps the two from drifting: every test in
    /// the crate, and every corpus run under a debug build, checks the arms
    /// against it.
    pub(crate) fn execute_classed<T: Tracer + ?Sized>(
        &mut self,
        inst: &Inst,
        pc: u32,
        tracer: &mut T,
    ) -> Result<(Flow, InstClass), Trap> {
        let (flow, class) = match inst {
            // --- arithmetic / logical / shift (register + register-immediate shifts) ---
            Inst::Rrr(op, ..) => (
                self.exec_arith(inst, tracer)?,
                match op {
                    AluRrr::Mull
                    | AluRrr::Muluh
                    | AluRrr::Mulsh
                    | AluRrr::Mul16u
                    | AluRrr::Mul16s => InstClass::Mul,
                    AluRrr::Quou | AluRrr::Quos | AluRrr::Remu | AluRrr::Rems => InstClass::DivRem,
                    _ => InstClass::Alu,
                },
            ),
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
            | Inst::MovN(..) => (self.exec_arith(inst, tracer)?, InstClass::Alu),

            // --- immediate / move ---
            Inst::Movi(..)
            | Inst::MoviN(..)
            | Inst::Addi(..)
            | Inst::AddiN(..)
            | Inst::Addmi(..) => (self.exec_imm(inst, tracer)?, InstClass::Alu),

            // --- load / store (incl. l32r literal load) ---
            Inst::Load(..) | Inst::L32iN(..) | Inst::L32r(..) => {
                (self.exec_load_store(inst, pc, tracer)?, InstClass::Load)
            }
            Inst::Store(..) | Inst::S32iN(..) => {
                (self.exec_load_store(inst, pc, tracer)?, InstClass::Store)
            }

            // --- conditional branches ---
            Inst::BranchRr(..)
            | Inst::BranchRi(..)
            | Inst::BranchRiu(..)
            | Inst::BranchZ(..)
            | Inst::BranchBiI(..)
            | Inst::BranchZN(..) => {
                let flow = self.exec_branch(inst, pc)?;
                (flow, branch_class(&flow))
            }

            // --- unconditional jumps ---
            Inst::J(..) => (self.exec_jump(inst, pc)?, InstClass::JalTail),
            Inst::Jx(..) => (self.exec_jump(inst, pc)?, InstClass::JalrIndirect),

            // --- calls ---
            Inst::Call(..) => (self.exec_call(inst, pc, tracer)?, InstClass::JalCall),
            Inst::Callx(..) => (self.exec_call(inst, pc, tracer)?, InstClass::JalrCall),

            // --- window management ---
            Inst::Entry(..) => (self.exec_entry(inst, tracer)?, InstClass::Alu),
            Inst::Nullary(NullaryOp::Retw) | Inst::NullaryN(NullaryNarrowOp::RetwN) => {
                (self.exec_retw(tracer)?, InstClass::JalrReturn)
            }
            Inst::Nullary(NullaryOp::Ret) | Inst::NullaryN(NullaryNarrowOp::RetN) => {
                (self.exec_ret(), InstClass::JalrReturn)
            }

            // --- misc / barriers / nops / illegal ---
            Inst::Nullary(NullaryOp::Nop) | Inst::NullaryN(NullaryNarrowOp::NopN) => {
                (self.exec_misc(inst, pc)?, InstClass::Alu)
            }
            Inst::Nullary(_) | Inst::NullaryN(_) => (self.exec_misc(inst, pc)?, InstClass::System),

            // --- floating point, boolean, special registers ---
            //
            // One group in the executor, many cost buckets: compiled shaders
            // are float-heavy, so this arm resolves its own bucket rather than
            // deferring to a second walk. See `inst_class` for what each
            // bucket means.
            Inst::FpLsi(op, ..) => (
                self.exec_float(inst, pc, tracer)?,
                match op {
                    FpLsiOp::Lsi | FpLsiOp::Lsip => InstClass::Load,
                    FpLsiOp::Ssi | FpLsiOp::Ssip => InstClass::Store,
                },
            ),
            Inst::FpLsx(op, ..) => (
                self.exec_float(inst, pc, tracer)?,
                match op {
                    FpLsxOp::Lsx | FpLsxOp::Lsxp => InstClass::Load,
                    FpLsxOp::Ssx | FpLsxOp::Ssxp => InstClass::Store,
                },
            ),
            Inst::FpRrr(op, ..) => (
                self.exec_float(inst, pc, tracer)?,
                match op {
                    FpRrrOp::MaddS | FpRrrOp::MsubS | FpRrrOp::MaddnS | FpRrrOp::DivnS => {
                        InstClass::FloatMulAdd
                    }
                    _ => InstClass::FloatArith,
                },
            ),
            Inst::FpRr(op, ..) => (
                self.exec_float(inst, pc, tracer)?,
                match op {
                    FpRrOp::Recip0S
                    | FpRrOp::Sqrt0S
                    | FpRrOp::Rsqrt0S
                    | FpRrOp::Div0S
                    | FpRrOp::Nexp01S
                    | FpRrOp::MksadjS
                    | FpRrOp::MkdadjS
                    | FpRrOp::AddexpS
                    | FpRrOp::AddexpmS => InstClass::FloatEstimate,
                    // `mov.s`/`abs.s`/`neg.s` are bit operations, not arithmetic.
                    _ => InstClass::FloatArith,
                },
            ),
            Inst::FpMovAr(..) | Inst::FpMovBr(..) | Inst::ConstS(..) => {
                (self.exec_float(inst, pc, tracer)?, InstClass::FloatArith)
            }
            Inst::FpCmp(..) => (self.exec_float(inst, pc, tracer)?, InstClass::FloatCompare),
            Inst::FpToInt(..) | Inst::IntToFp(..) => {
                (self.exec_float(inst, pc, tracer)?, InstClass::FloatConvert)
            }
            // Register-file transfers and the Boolean-option AR move are
            // integer data movement, whatever file they read.
            Inst::Rfr(..) | Inst::Wfr(..) | Inst::MovBool(..) => {
                (self.exec_float(inst, pc, tracer)?, InstClass::Alu)
            }
            Inst::BranchBool(..) => {
                let flow = self.exec_float(inst, pc, tracer)?;
                (flow, branch_class(&flow))
            }
            Inst::Sr(..) | Inst::Ur(..) => (self.exec_float(inst, pc, tracer)?, InstClass::System),

            // --- machine-mode-only families ---
            //
            // `lp-xt-inst` decodes these (M1 P1) so the privileged hart can;
            // the user-mode runner has no state for them and says so loudly
            // rather than doing nothing. See `machine_mode_only`.
            Inst::AtomicLs(..)
            | Inst::Loop(..)
            | Inst::Rf(..)
            | Inst::Rfi(..)
            | Inst::Rsil(..)
            | Inst::Waiti(..)
            | Inst::Rotw(..)
            | Inst::WindowLs(..)
            | Inst::Break(..)
            | Inst::BreakN(..)
            | Inst::Clamps(..)
            | Inst::BoolLogic(..)
            | Inst::BoolAll(..)
            | Inst::Tlb(..)
            | Inst::TlbInv(..)
            | Inst::ExtReg(..)
            | Inst::Mac(..)
            | Inst::MacLd(..)
            | Inst::MacLoad(..) => return Err(machine_mode_only()),
        };
        debug_assert_eq!(
            class,
            inst_class(inst, &flow),
            "execute_classed and inst_class disagree on {inst:?}"
        );
        Ok((flow, class))
    }
}

/// The branch cost bucket for an outcome: taken branches and not-taken ones
/// are different costs on real silicon, and [`Flow::Jump`] is what "taken"
/// looks like here.
#[inline]
fn branch_class(flow: &Flow) -> InstClass {
    match flow {
        Flow::Jump(_) => InstClass::BranchTaken,
        _ => InstClass::BranchNotTaken,
    }
}
