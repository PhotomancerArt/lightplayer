//! Register windows as the hardware does them: the per-instruction overflow
//! check, RETW's underflow check, MOVSP's alloca check, and the window
//! exception returns.
//!
//! Semantics are the Xtensa ISA Reference Manual's (2010 edition):
//! §4.7.1.3 "Window Overflow Check" for `WindowCheck`, §4.7.1.4 "Call, Entry,
//! and Return Mechanism" for CALLn/ENTRY/RETW, the ENTRY, RETW, MOVSP, ROTW,
//! RFWO and RFWU instruction pages for the rest. No QEMU/binutils/GCC source
//! was consulted (AGENTS.md license rule).

use lp_xt_inst::{CallOp, CallxOp, Inst, MacSrc, MacY};

use crate::cpu::{Cpu, NUM_BASES};

/// How window overflow and underflow are handled.
///
/// The user-mode runner keeps the **direct** model: `executor/window.rs`
/// spills and reloads to the ABI save areas itself, with a host-side
/// `Cpu::call_stack` shadow that hardware does not have. It is the FP/JIT
/// oracle, its silicon replays must stay byte-identical, and Q3 puts it out of
/// this milestone's reach.
///
/// Machine mode raises the **real** exception and runs the guest's own
/// `_WindowOverflow{4,8,12}` / `_WindowUnderflow{4,8,12}` handlers, because
/// the direct model gives wrong answers for firmware that reads another
/// frame's save area — `docs/adr/2026-07-30-xtensa-backtrace-window-spill.md`
/// measured a 25-deep chain reporting 19 identical PCs.
///
/// This is a **selector, not a fork**: there is one `ENTRY`/`RETW`
/// implementation and it branches on this, so the two models cannot drift
/// apart silently. Under `Exception`, `Cpu::call_stack` is not maintained —
/// it is a user-mode artefact, and the executors `debug_assert!` it empty.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowPolicy {
    /// Model the effect: spill/reload directly (user mode, unchanged).
    Direct,
    /// Raise `WindowOverflow`/`WindowUnderflow` and vector (machine mode).
    Exception,
}

/// What the window machinery decided about an instruction before it runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowEvent {
    /// A window overflow: `n` groups (1..=3) up from the current base is a
    /// live older frame that the instruction would touch. `WindowBase` has
    /// been moved to that frame (`m` in the RM), `PS.OWB` holds the old base,
    /// and `vector_inc` is the handler to run — 1, 2 or 3 for
    /// `_WindowOverflow4/8/12`.
    Overflow { vector_inc: u8 },
    /// RETW into a caller whose registers are not resident. `WindowBase` has
    /// already been decremented by `n` (the RM's "minor exception to the
    /// rule"), `PS.OWB` holds the returning frame's base, and the handler is
    /// `_WindowUnderflow{4n}`.
    Underflow { n: u8 },
    /// MOVSP with the caller's registers not present: `AllocaCause`.
    Alloca,
    /// ENTRY/RETW under conditions the RM calls undefined. This hart takes
    /// the RM's own suggested implementation and raises an illegal
    /// instruction (ruling R3, the loud option): ENTRY with `PS.WOE = 0`,
    /// `PS.EXCM = 1` or `s > 3`; RETW with `PS.WOE = 0`, `PS.EXCM = 1`,
    /// `a0[31:30] = 0`, or a WindowStart bit set closer than the caller's.
    Illegal,
}

/// The highest 4-register group (0..=3) of the address registers an
/// instruction references, for the RM's `WindowCheck(wr, ws, wt)` — where
/// `ref()` is true for exactly the `Reg` operands below. FP, Boolean and
/// MAC16 register operands are not address registers and do not count.
///
/// CALLn/CALLXn check their `a[4n]` (RM §4.7.1.4: `WindowCheck(00, 00, n)`)
/// and ENTRY checks the new frame's group (`WindowCheck(00, PS.CALLINC, 00)`);
/// both are folded in here so the hart has one check to run.
///
/// Exhaustive on purpose: a new `Inst` variant fails to compile rather than
/// silently escaping the overflow check.
pub fn ar_group(inst: &Inst, ps_callinc: u8) -> u8 {
    let g = |r: lp_xt_inst::Reg| r.num() >> 2;
    match *inst {
        Inst::Rrr(_, rd, rs, rt) => g(rd).max(g(rs)).max(g(rt)),
        Inst::Rt(_, rd, rt) => g(rd).max(g(rt)),
        Inst::Rs(_, rd, rs) => g(rd).max(g(rs)),
        Inst::ShiftSet(_, rs) => g(rs),
        Inst::Ssai(_) => 0,
        Inst::Slli(rd, rs, _) => g(rd).max(g(rs)),
        Inst::Srli(rd, rt, _) | Inst::Srai(rd, rt, _) => g(rd).max(g(rt)),
        Inst::Extui(rd, rt, _, _) => g(rd).max(g(rt)),
        Inst::Sext(rd, rs, _) | Inst::Clamps(rd, rs, _) => g(rd).max(g(rs)),
        Inst::MovN(rt, rs) => g(rt).max(g(rs)),
        Inst::AddN(rd, rs, rt) => g(rd).max(g(rs)).max(g(rt)),
        Inst::AddiN(rd, rs, _) => g(rd).max(g(rs)),
        Inst::Addi(rt, rs, _) | Inst::Addmi(rt, rs, _) => g(rt).max(g(rs)),
        Inst::Movi(rt, _) | Inst::MoviN(rt, _) => g(rt),
        Inst::Load(_, rt, rs, _) | Inst::Store(_, rt, rs, _) => g(rt).max(g(rs)),
        Inst::L32iN(rt, rs, _) | Inst::S32iN(rt, rs, _) => g(rt).max(g(rs)),
        Inst::AtomicLs(_, at, r#as, _) => g(at).max(g(r#as)),
        Inst::L32r(rt, _) => g(rt),
        Inst::BranchRr(_, rs, rt, _) => g(rs).max(g(rt)),
        Inst::BranchRi(_, rs, _, _) | Inst::BranchRiu(_, rs, _, _) => g(rs),
        Inst::BranchZ(_, rs, _) => g(rs),
        Inst::BranchBiI(_, rs, _, _) => g(rs),
        Inst::BranchZN(_, rs, _) => g(rs),
        Inst::Loop(_, r#as, _) => g(r#as),
        Inst::J(_) => 0,
        Inst::Jx(rs) => g(rs),
        Inst::Call(op, _) => match op {
            CallOp::Call0 => 0,
            CallOp::Call4 => 1,
            CallOp::Call8 => 2,
            CallOp::Call12 => 3,
        },
        Inst::Callx(op, rs) => g(rs).max(match op {
            CallxOp::Callx0 => 0,
            CallxOp::Callx4 => 1,
            CallxOp::Callx8 => 2,
            CallxOp::Callx12 => 3,
        }),
        Inst::Entry(rs, _) => g(rs).max(ps_callinc & 3),
        Inst::Rf(_) | Inst::Rfi(_) | Inst::Waiti(_) | Inst::Rotw(_) => 0,
        Inst::Rsil(at, _) => g(at),
        Inst::Break(..) | Inst::BreakN(_) => 0,
        Inst::WindowLs(_, at, r#as, _) => g(at).max(g(r#as)),
        // RETW reads a0; its underflow check is its own (`retw_check`).
        Inst::Nullary(_) | Inst::NullaryN(_) => 0,

        Inst::FpRrr(..) | Inst::FpRr(..) | Inst::ConstS(..) => 0,
        Inst::Rfr(ar, _) => g(ar),
        Inst::Wfr(_, r#as) => g(r#as),
        Inst::FpMovAr(_, _, _, at) => g(at),
        Inst::FpMovBr(..) | Inst::FpCmp(..) => 0,
        Inst::FpToInt(_, ar, _, _) => g(ar),
        Inst::IntToFp(_, _, r#as, _) => g(r#as),
        Inst::FpLsx(_, _, r#as, at) => g(r#as).max(g(at)),
        Inst::FpLsi(_, _, r#as, _) => g(r#as),

        Inst::MovBool(_, ar, r#as, _) => g(ar).max(g(r#as)),
        Inst::BranchBool(..) | Inst::BoolLogic(..) | Inst::BoolAll(..) => 0,

        Inst::Tlb(_, at, r#as) | Inst::ExtReg(_, at, r#as) => g(at).max(g(r#as)),
        Inst::TlbInv(_, r#as) => g(r#as),

        Inst::Mac(_, _, src) => match src {
            MacSrc::Aa(r#as, at) => g(r#as).max(g(at)),
            MacSrc::Ad(r#as, _) => g(r#as),
            MacSrc::Da(_, at) => g(at),
            MacSrc::Dd(..) => 0,
        },
        Inst::MacLd(_, _, _, r#as, _, y) => g(r#as).max(match y {
            MacY::Ar(at) => g(at),
            MacY::Mr(_) => 0,
        }),
        Inst::MacLoad(_, _, r#as) => g(r#as),

        Inst::Sr(_, _, at) | Inst::Ur(_, _, at) => g(at),
    }
}

/// The RM's `WindowCheck` (§4.7.1.3), run before an instruction that
/// references a register in group `group` (0..=3) of the current window while
/// `CWOE = 1`.
///
/// Returns the overflow to take, having already moved `WindowBase` to the
/// frame that must be spilled — the RM's `m` — and saved the old base in the
/// caller-supplied `owb` slot. The vector is chosen by the position of the
/// next `WindowStart` bit above `m`: that is the call increment the victim's
/// callee was entered with, which is exactly how many of the victim's
/// registers (`a0..a3`, `..a7`, `..a11`) the handler must spill, and why the
/// handlers find the callee's stack pointer in `a5`/`a9`/`a13`.
///
/// `None` when nothing needs spilling. A single instruction may overflow more
/// than once (the RM's a4..a7 / a8..a15 example): the hart re-executes it
/// after `rfwo` and this check runs again.
pub fn overflow_check(cpu: &mut Cpu, group: u8, owb: &mut u8) -> Option<WindowEvent> {
    if group == 0 {
        return None;
    }
    let base = cpu.window_base;
    let set = |k: u8| cpu.window_start & (1u16 << ((base + k) % NUM_BASES)) != 0;
    // First set bit within reach, in order — n = 1, 2, 3.
    let n = if set(1) {
        1
    } else if group >= 2 && set(2) {
        2
    } else if group >= 3 && set(3) {
        3
    } else {
        return None;
    };
    let m = (base + n) % NUM_BASES;
    let set_at = |k: u8| cpu.window_start & (1u16 << ((m + k) % NUM_BASES)) != 0;
    let vector_inc = if set_at(1) {
        1
    } else if set_at(2) {
        2
    } else {
        3
    };
    *owb = base;
    cpu.window_base = m;
    Some(WindowEvent::Overflow { vector_inc })
}

/// RETW's own checks (RM §4.7.1.4 and the RETW page), run before the
/// executor rotates. `woe`/`excm` are the live PS bits.
///
/// `None` means the return may complete. `Underflow` has already decremented
/// `WindowBase` and saved the returning frame's base into `owb`, as the RM
/// specifies. `Illegal` is the RM's "undefined operation — may raise an
/// illegal instruction exception", taken literally (R3).
pub fn retw_check(cpu: &mut Cpu, woe: bool, excm: bool, owb: &mut u8) -> Option<WindowEvent> {
    let n = ((cpu.a(0) >> 30) & 3) as u8;
    let base = cpu.window_base;
    let below = |k: u8| cpu.window_start & (1u16 << ((base + NUM_BASES - k) % NUM_BASES)) != 0;
    let m = if below(1) {
        1
    } else if below(2) {
        2
    } else if below(3) {
        3
    } else {
        0
    };
    if n == 0 || (m != 0 && m != n) || !woe || excm {
        return Some(WindowEvent::Illegal);
    }
    let caller = (base + NUM_BASES - n) % NUM_BASES;
    if cpu.window_start & (1u16 << caller) != 0 {
        return None;
    }
    *owb = base;
    cpu.window_base = caller;
    Some(WindowEvent::Underflow { n })
}

/// ENTRY's legality (the ENTRY page): undefined when `s > 3`, `PS.WOE = 0`
/// or `PS.EXCM = 1`; this hart raises an illegal instruction (R3).
pub fn entry_check(s: u8, woe: bool, excm: bool) -> Option<WindowEvent> {
    (s > 3 || !woe || excm).then_some(WindowEvent::Illegal)
}

/// MOVSP's check (the MOVSP page): `AllocaCause` unless one of the three
/// WindowStart bits below the current base is set — the caller's registers
/// are present.
pub fn movsp_check(cpu: &Cpu) -> Option<WindowEvent> {
    let base = cpu.window_base;
    let below = |k: u8| cpu.window_start & (1u16 << ((base + NUM_BASES - k) % NUM_BASES)) != 0;
    (!(below(1) || below(2) || below(3))).then_some(WindowEvent::Alloca)
}

/// `ROTW imm4`: `WindowBase <- WindowBase + imm4` (the ROTW page). Privileged;
/// this hart models no rings, so it always runs.
pub fn rotw(cpu: &mut Cpu, imm: i8) {
    let delta = (imm as i16).rem_euclid(i16::from(NUM_BASES)) as u8;
    cpu.window_base = (cpu.window_base + delta) % NUM_BASES;
}

/// The window half of `RFWO` / `RFWU` (their pages): clear (overflow) or set
/// (underflow) the WindowStart bit of the frame the handler worked on, then
/// restore `WindowBase` from `PS.OWB`. The PS/PC half is the hart's.
pub fn rfw(cpu: &mut Cpu, owb: u8, resident: bool) {
    let bit = 1u16 << cpu.window_base;
    if resident {
        cpu.window_start |= bit;
    } else {
        cpu.window_start &= !bit;
    }
    cpu.window_base = owb % NUM_BASES;
}
