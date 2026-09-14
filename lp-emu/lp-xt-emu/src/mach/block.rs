//! The Xtensa side of the block cache: the classification, and the decoder
//! that turns a run of guest instructions into [`XtSlot`]s.
//!
//! [`lp_emu_core::block`] owns the table, the arena, the budget rule and
//! invalidation and knows nothing about Xtensa. [`crate::block`] owns the
//! slot. This is the third piece: **which** instructions may live in a block,
//! **where** a block ends, and how the bytes at a `pc` become slots.
//!
//! # Where a block ends, and what never enters one
//!
//! A block ends **after** a control transfer and **before** anything the hart
//! has to see arrive from a fresh fetch. A plain store stays *inside* a block
//! (M5 MD2, measured on RV32: ending at stores costs 28–31 % of the mean
//! block length and produces 40–45 % more blocks) — and on this chip that is
//! safe because the store-address invalidation contract (M7 XD3) drains at
//! polling point (c), which is the instruction boundary immediately after the
//! store and before the next fetch.
//!
//! Classification is **conservative in the safe direction**. Calling a body
//! instruction a terminator only shortens a block; calling a control transfer
//! a body instruction would be a bug. So every instruction that can move the
//! `pc` by itself, rotate the window, change `PS`, write a special register
//! or arm a debug facility is a terminator, and anything not positively
//! recognised is refused. Refusing is always exact: the caller falls back to
//! [`crate::mach::XtHart::step_one`], which has always run it.
//!
//! The block executor also re-derives each slot's next `pc` from the retire
//! and leaves the block the moment it is not the `pc` the decoder expected. In
//! a correct build that fires exactly at a taken terminator, a window
//! exception, a trap, a loop-back or an interrupt; it is there so a
//! classification mistake is a **slower** block rather than a wrong one.
//!
//! # Why the fetch here is the same fetch the interpreter makes
//!
//! [`crate::mach::XtHart::step_once`] reads three bytes with
//! [`Bus::fetch_bytes`] at any alignment and hands them to
//! [`lp_xt_inst::decode`]; so does this. A block is only ever built over a bus
//! that claims [`Bus::fetch_is_pure`], so reading those bytes early charges
//! nothing and traps nothing. Under `--features bench` the bus's own fetch
//! counter still counts a decode-ahead fetch, exactly as the RV32 side's does
//! — a diagnostic build counts fetches, not retired instructions.

use lp_emu_core::Bus;
use lp_xt_inst::{AluRs, Inst, NullaryNarrowOp, NullaryOp};

use crate::block::XtSlot;
use crate::cpu::Cpu;
use crate::mach::window;

extern crate alloc;
use alloc::vec::Vec;

/// The most slots one block may hold.
///
/// A block that fits its budget runs without a per-slot deadline compare, so
/// one block must not be able to dwarf a slice: the classic's slice cap is
/// 8,192 cycles (1,024 under `--strict-bus`) and its default window is 256.
/// RV32 measured a mean realised block length of 4.69–6.97 with 83 % of
/// instructions in blocks of 16 or fewer; 64 is far past the distribution and
/// still bounded. Kept equal to RV32's `MAX_BLOCK_SLOTS` so the two
/// architectures' arenas are sized against the same number.
pub(super) const MAX_BLOCK_SLOTS: usize = 64;

/// What the decoder decided about one instruction.
pub(super) enum Class {
    /// May live inside a block and never changes `pc` by itself.
    Body(XtSlot),
    /// May live inside a block, and is the last thing in it.
    Terminator(XtSlot),
    /// Must not be cached. The block ends *before* it, and if the block would
    /// then be empty the address is not cacheable at all.
    Refused,
}

/// Classify one decoded instruction.
///
/// The three sets, and why each member is where it is:
///
/// **Refused** — never inside a block, and an address whose first instruction
/// is one of these is not cacheable:
///
/// - `break` / `break.n` and `syscall`: the hart hands the slice back at
///   these without retiring or charging them, and a machine may re-present
///   the same `pc`. A slot that the executor may decline to consume has no
///   business inside a run of slots.
/// - `l32ai` / `s32ri` / `s32c1i` ([`Inst::AtomicLs`]): the synchronising
///   accesses. `s32c1i` is the guest's only atomic and it carries the
///   side-band; the Xtensa twin of RV32 refusing the A extension.
/// - `l32e` / `s32e` ([`Inst::WindowLs`]): the window handlers' own
///   spill/reload, which run with `PS.EXCM` set and a rotated window.
/// - the TLB ops, `rer` / `wer` and MAC16: decoded, barely modelled, and each
///   one is rare enough that caching it would buy nothing it could lose.
/// - `ill` / `ill.n`: they never retire.
///
/// **Terminator** — in the block, and last:
///
/// - every control transfer: `j`, `jx`, `call*`, `callx*`, every conditional
///   branch form, and `bt`/`bf`.
/// - `loop` / `loopnez` / `loopgtz`: they write `LBEG`/`LEND`/`LCOUNT`, and
///   the loop-back test in [`crate::mach::XtHart::step`] reads `LEND` on the
///   *next* instruction — so a `loop` ends its block and the next block
///   starts at `LBEG`.
/// - `entry`, `retw`, `retw.n`, `ret`, `ret.n`, `rotw`, `movsp`: they rotate
///   the window or return through it.
/// - `rfe` / `rfde` / `rfwo` / `rfwu` and `rfi`: exception returns.
/// - `waiti` and `rsil`: they change `PS.INTLEVEL`, and `waiti` ends the
///   slice.
/// - `isync`: the whole-cache invalidation event (see [`super::translated`]'s
///   three events).
/// - **any** `rsr`/`wsr`/`xsr` and `rur`/`wur`. A write to `PS`,
///   `WINDOWBASE`, `WINDOWSTART`, `INTENABLE`, `LBEG`/`LEND`/`LCOUNT`,
///   `IBREAKENABLE` or a `DBREAK` register changes what the *next*
///   instruction means, so it must be last. A *read* could be a body slot for
///   the registers the hart reads without effect — but not for all of them
///   (`xsr` is a read and a write, and `rsr.ccount` is live), so the whole
///   family is a terminator. It costs a little block length and it is the
///   "when in doubt" answer the classification rule asks for.
///
/// **Body** — the rest: the ALU, the immediates, `l32r`, the plain loads and
/// stores, the FP families, `movt`/`movf`, and the three hart-owned families
/// that have no effect beyond their destination register (`clamps`, the
/// Boolean logic and the Boolean reductions). A hart-owned body slot is run
/// through `exec_priv` by the block executor exactly as
/// [`crate::mach::XtHart::step`] would, because the block executor *is*
/// `step`.
pub(super) fn classify(inst: Inst, len: u8) -> Class {
    let slot = || XtSlot::new(inst, len);
    match inst {
        // --- refused ------------------------------------------------------
        Inst::Break(..)
        | Inst::BreakN(_)
        | Inst::Nullary(NullaryOp::Syscall)
        | Inst::Nullary(NullaryOp::Ill)
        | Inst::NullaryN(NullaryNarrowOp::IllN)
        | Inst::AtomicLs(..)
        | Inst::WindowLs(..)
        | Inst::Tlb(..)
        | Inst::TlbInv(..)
        | Inst::ExtReg(..)
        | Inst::Mac(..)
        | Inst::MacLd(..)
        | Inst::MacLoad(..) => Class::Refused,

        // --- terminators --------------------------------------------------
        Inst::J(_)
        | Inst::Jx(_)
        | Inst::Call(..)
        | Inst::Callx(..)
        | Inst::BranchRr(..)
        | Inst::BranchRi(..)
        | Inst::BranchRiu(..)
        | Inst::BranchZ(..)
        | Inst::BranchBiI(..)
        | Inst::BranchZN(..)
        | Inst::BranchBool(..)
        | Inst::Loop(..)
        | Inst::Entry(..)
        | Inst::Rotw(_)
        | Inst::Rs(AluRs::Movsp, ..)
        | Inst::Rf(_)
        | Inst::Rfi(_)
        | Inst::Rsil(..)
        | Inst::Waiti(_)
        | Inst::Sr(..)
        | Inst::Ur(..)
        | Inst::Nullary(NullaryOp::Isync)
        | Inst::Nullary(NullaryOp::Ret)
        | Inst::Nullary(NullaryOp::Retw)
        | Inst::NullaryN(NullaryNarrowOp::RetN)
        | Inst::NullaryN(NullaryNarrowOp::RetwN) => Class::Terminator(slot()),

        // --- body ---------------------------------------------------------
        _ => Class::Body(slot()),
    }
}

/// Decode the block starting at `pc` into `out`.
///
/// Stops **after** a terminator, **before** a refused instruction, at
/// [`MAX_BLOCK_SLOTS`], at a fetch that faults, at a word the decoder refuses
/// or truncates, and at an address that would wrap the address space. An
/// empty `out` means "this address is not cacheable", which the caller turns
/// into a single [`crate::mach::XtHart::step_once`] — the fetch error, the
/// illegal instruction and the `break` are then delivered by the interpreter
/// at exactly the `pc` it always delivered them at.
/// It also fills in the block's **maximum address-register group** on the
/// first slot's [`XtSlot::block_group`] — the input to
/// [`window_check_hoistable`] and the whole of M7 XD5's per-block precondition.
/// Every early return goes through [`finish_groups`] so a block that ended at
/// a fetch error, a refused encoding or the slot cap carries the same answer a
/// complete one would.
pub(super) fn decode_block<B: Bus>(bus: &mut B, pc: u32, out: &mut Vec<XtSlot>) {
    let mut at = pc;
    let mut bytes = [0u8; 3];
    for _ in 0..MAX_BLOCK_SLOTS {
        let Ok(got) = bus.fetch_bytes(at, &mut bytes) else {
            return finish_groups(out);
        };
        // A `Truncated` or `Unsupported` decode ends the block *before* the
        // instruction — refused, never guessed. The interpreter raises the
        // fetch error or the illegal instruction from its own fetch.
        let Ok((inst, len)) = lp_xt_inst::decode(&bytes[..got]) else {
            return finish_groups(out);
        };
        match classify(inst, len as u8) {
            Class::Body(slot) => {
                out.push(slot);
                match at.checked_add(len as u32) {
                    Some(next) => at = next,
                    None => return finish_groups(out),
                }
            }
            Class::Terminator(slot) => {
                out.push(slot);
                return finish_groups(out);
            }
            Class::Refused => return finish_groups(out),
        }
    }
    finish_groups(out);
}

/// Record the block's maximum [`XtSlot::group`] on its first slot.
///
/// Once per decode, never per execution: the block is decoded once and run
/// thousands of times (P01 measured a 99.85 % hit rate on the render loop), so
/// the fold belongs here and the block executor's job is one byte load.
fn finish_groups(out: &mut [XtSlot]) {
    let max = out.iter().map(|s| s.group).max().unwrap_or(0);
    if let Some(first) = out.first_mut() {
        first.block_group = max;
    }
}

/// **The per-block window precondition** (M7 XD5): may this whole block run
/// with the per-instruction overflow check skipped?
///
/// The RM's `WindowCheck` (§4.7.1.3) answers from `(WindowBase, WindowStart,
/// group)` and nothing else. `group` is a property of the instruction, known
/// at decode time and bounded above per block by
/// [`XtSlot::block_group`]; `WindowBase` and `WindowStart` are hart state that
/// **nothing inside a block can move** — every instruction that writes either
/// (`ENTRY`, `RETW`, `ROTW`, `RFWO`/`RFWU`, `wsr`/`xsr` to `WINDOWBASE` or
/// `WINDOWSTART`) is a [`Class::Terminator`], the last slot of its block, and
/// `L32E`/`S32E` are [`Class::Refused`]. An exception moves them, and an
/// exception moves the `pc` off the straight line, which leaves the block. So
/// the answer taken once at block entry is the answer at every slot, and
/// [`window::overflow_in_reach`] is monotone in `group`, which is why the
/// block's maximum is the only group worth asking about.
///
/// `PS.WOE` and `PS.EXCM` are read here for the same reason and hold for the
/// same one: `wsr.ps`, `rsil`, `waiti`, `rfe`/`rfi`/`rfde` and the `CALLn`
/// family are all terminators, and interrupt or exception entry leaves the
/// block. Under `!woe || excm` [`crate::mach::XtHart::step`] skips the check
/// per instruction anyway, so the block hoists trivially.
///
/// A `true` here means "no slot in this block can overflow"; a `false` means
/// "one might", and the block then runs with exactly the check it has always
/// run, at exactly the slot that would have raised it.
pub(super) fn window_check_hoistable(cpu: &Cpu, woe: bool, excm: bool, slots: &[XtSlot]) -> bool {
    if !woe || excm {
        return true;
    }
    let Some(first) = slots.first() else {
        return true;
    };
    window::overflow_in_reach(cpu.window_base, cpu.window_start, first.block_group).is_none()
}

#[cfg(test)]
mod tests {
    use lp_xt_inst::{
        AluRrr, BrRr, BrZ, CallOp, CallxOp, LoadOp, LoopOp, Reg, SpecialReg, SrOp, StoreOp,
    };

    use super::*;

    fn class_of(inst: Inst) -> &'static str {
        let (decoded, len) = lp_xt_inst::decode(&lp_xt_inst::encode(&inst)).expect("decodes");
        assert_eq!(decoded, inst, "round-trip");
        match classify(decoded, len as u8) {
            Class::Body(_) => "body",
            Class::Terminator(_) => "terminator",
            Class::Refused => "refused",
        }
    }

    #[test]
    fn every_control_transfer_is_a_terminator() {
        assert_eq!(class_of(Inst::J(4)), "terminator", "j");
        assert_eq!(class_of(Inst::Jx(Reg::new(3))), "terminator", "jx");
        assert_eq!(
            class_of(Inst::Call(CallOp::Call8, 4)),
            "terminator",
            "call8"
        );
        assert_eq!(
            class_of(Inst::Callx(CallxOp::Callx8, Reg::new(3))),
            "terminator",
            "callx8"
        );
        assert_eq!(
            class_of(Inst::BranchRr(BrRr::Beq, Reg::new(2), Reg::new(3), 4)),
            "terminator",
            "beq"
        );
        assert_eq!(
            class_of(Inst::BranchZ(BrZ::Beqz, Reg::new(2), 4)),
            "terminator",
            "beqz"
        );
        assert_eq!(
            class_of(Inst::BranchZN(true, Reg::new(2), 4)),
            "terminator",
            "bnez.n"
        );
        assert_eq!(class_of(Inst::Nullary(NullaryOp::Ret)), "terminator", "ret");
        assert_eq!(
            class_of(Inst::NullaryN(NullaryNarrowOp::RetwN)),
            "terminator",
            "retw.n"
        );
    }

    #[test]
    fn the_window_and_special_register_families_are_terminators() {
        assert_eq!(
            class_of(Inst::Entry(Reg::new(1), 32)),
            "terminator",
            "entry"
        );
        assert_eq!(class_of(Inst::Rotw(1)), "terminator", "rotw");
        assert_eq!(
            class_of(Inst::Rs(AluRs::Movsp, Reg::new(1), Reg::new(2))),
            "terminator",
            "movsp"
        );
        assert_eq!(
            class_of(Inst::Sr(SrOp::Wsr, SpecialReg::Ps, Reg::new(2))),
            "terminator",
            "wsr.ps"
        );
        assert_eq!(
            class_of(Inst::Sr(SrOp::Rsr, SpecialReg::Ccount, Reg::new(2))),
            "terminator",
            "rsr.ccount — a read is a terminator too; `when in doubt` is the rule"
        );
        assert_eq!(
            class_of(Inst::Sr(SrOp::Wsr, SpecialReg::Lend, Reg::new(2))),
            "terminator",
            "wsr.lend"
        );
        assert_eq!(class_of(Inst::Waiti(0)), "terminator", "waiti");
        assert_eq!(class_of(Inst::Rsil(Reg::new(2), 3)), "terminator", "rsil");
        assert_eq!(
            class_of(Inst::Nullary(NullaryOp::Isync)),
            "terminator",
            "isync"
        );
        assert_eq!(
            class_of(Inst::Loop(LoopOp::Loopnez, Reg::new(2), 4)),
            "terminator",
            "loopnez — it writes LBEG/LEND/LCOUNT"
        );
    }

    #[test]
    fn the_hart_owned_and_unmodelled_families_are_refused() {
        assert_eq!(class_of(Inst::Break(1, 2)), "refused", "break");
        assert_eq!(class_of(Inst::BreakN(1)), "refused", "break.n");
        assert_eq!(
            class_of(Inst::Nullary(NullaryOp::Syscall)),
            "refused",
            "syscall"
        );
        assert_eq!(
            class_of(Inst::Nullary(NullaryOp::Ill)),
            "refused",
            "ill never retires"
        );
        assert_eq!(
            class_of(Inst::AtomicLs(
                lp_xt_inst::AtomicLsOp::S32c1i,
                Reg::new(2),
                Reg::new(3),
                0
            )),
            "refused",
            "s32c1i"
        );
        assert_eq!(
            class_of(Inst::WindowLs(
                lp_xt_inst::WindowLsOp::S32e,
                Reg::new(2),
                Reg::new(3),
                -4
            )),
            "refused",
            "s32e"
        );
        assert_eq!(
            class_of(Inst::ExtReg(false, Reg::new(2), Reg::new(3))),
            "refused",
            "rer"
        );
    }

    #[test]
    fn the_ordinary_arithmetic_and_memory_encodings_are_block_bodies() {
        assert_eq!(class_of(Inst::Nullary(NullaryOp::Nop)), "body", "nop");
        assert_eq!(
            class_of(Inst::NullaryN(NullaryNarrowOp::NopN)),
            "body",
            "nop.n"
        );
        assert_eq!(
            class_of(Inst::Nullary(NullaryOp::Memw)),
            "body",
            "memw is hot and is never an event (XD3)"
        );
        assert_eq!(
            class_of(Inst::Rrr(
                AluRrr::Add,
                Reg::new(2),
                Reg::new(3),
                Reg::new(4)
            )),
            "body",
            "add"
        );
        assert_eq!(
            class_of(Inst::Load(LoadOp::L32i, Reg::new(2), Reg::new(3), 0)),
            "body",
            "l32i"
        );
        assert_eq!(
            class_of(Inst::Store(StoreOp::S32i, Reg::new(2), Reg::new(3), 0)),
            "body",
            "s32i — a plain store stays inside a block (M5 MD2, XD3)"
        );
        assert_eq!(class_of(Inst::L32r(Reg::new(2), 0xFFFF)), "body", "l32r");
        assert_eq!(
            class_of(Inst::Clamps(Reg::new(2), Reg::new(3), 7)),
            "body",
            "clamps is hart-owned but has no effect beyond its destination"
        );
    }

    /// The block's maximum group lands on the **first** slot, whatever slot it
    /// came from — the one byte the block executor reads before it runs
    /// anything (M7 XD5).
    #[test]
    fn the_first_slot_carries_the_blocks_maximum_group() {
        let mut out = Vec::new();
        for inst in [
            Inst::Nullary(NullaryOp::Nop),
            Inst::Rrr(AluRrr::Or, Reg::new(2), Reg::new(3), Reg::new(4)),
            Inst::Rrr(AluRrr::Or, Reg::new(11), Reg::new(3), Reg::new(4)),
            Inst::Nullary(NullaryOp::Nop),
        ] {
            let (decoded, len) = lp_xt_inst::decode(&lp_xt_inst::encode(&inst)).expect("decodes");
            out.push(XtSlot::new(decoded, len as u8));
        }
        assert_eq!(out[0].group, 0, "the first slot's own group is 0");
        finish_groups(&mut out);
        assert_eq!(out[0].block_group, 2, "a2 in the third slot reaches group 2");

        // An empty block — a refused first instruction — folds to nothing and
        // must not panic.
        let mut empty: Vec<XtSlot> = Vec::new();
        finish_groups(&mut empty);
        assert!(empty.is_empty());
    }

    /// The precondition, read against the three states the block executor can
    /// be in: no bit in reach, a bit in reach, and the check already off.
    #[test]
    fn the_precondition_answers_from_window_base_window_start_and_the_block_group() {
        let (decoded, len) = lp_xt_inst::decode(&lp_xt_inst::encode(&Inst::Rrr(
            AluRrr::Or,
            Reg::new(8),
            Reg::new(2),
            Reg::new(2),
        )))
        .expect("decodes");
        let mut slots = alloc::vec![XtSlot::new(decoded, len as u8)];
        finish_groups(&mut slots);
        assert_eq!(slots[0].block_group, 2);

        let mut cpu = Cpu::default();
        cpu.window_base = 0;
        // Only the current frame: nothing within reach of group 2.
        cpu.window_start = 0b1;
        assert!(window_check_hoistable(&cpu, true, false, &slots));
        // A frame two above: within reach of group 2.
        cpu.window_start = 0b101;
        assert!(!window_check_hoistable(&cpu, true, false, &slots));
        // …and the per-instruction check is skipped anyway under either of
        // these, so the block hoists trivially.
        assert!(window_check_hoistable(&cpu, false, false, &slots));
        assert!(window_check_hoistable(&cpu, true, true, &slots));
        // An empty block runs nothing.
        assert!(window_check_hoistable(&cpu, true, false, &[]));
    }

    #[test]
    fn a_slot_carries_the_width_the_decoder_produced() {
        let (inst, len) =
            lp_xt_inst::decode(&lp_xt_inst::encode(&Inst::NullaryN(NullaryNarrowOp::NopN)))
                .expect("decodes");
        let Class::Body(slot) = classify(inst, len as u8) else {
            panic!("nop.n is a body slot")
        };
        assert_eq!(slot.len, 2);
        let (inst, len) = lp_xt_inst::decode(&lp_xt_inst::encode(&Inst::Nullary(NullaryOp::Nop)))
            .expect("decodes");
        let Class::Body(slot) = classify(inst, len as u8) else {
            panic!("nop is a body slot")
        };
        assert_eq!(slot.len, 3);
    }
}
