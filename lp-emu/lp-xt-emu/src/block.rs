//! The Xtensa block-cache slot: one pre-decoded instruction.
//!
//! This module defines the **type** [`lp_emu_core::block::BlockCache`] is
//! parameterised by on this core, and nothing else. The wiring — the table,
//! the arena, the classification, invalidation and the block executor — lives
//! in [`crate::mach::block`] and [`crate::mach::XtHart`], and this file exists
//! so the cache and the Xtensa emulator plan's translator seam name **one**
//! slot rather than two (ruling R2; this plan opened its PR first, so the type
//! lives here).
//!
//! ⚠️ **Where the wiring went, corrected** (M7 XD2). This doc used to say the
//! cache would be wired into `crate::Emulator::run_loop` — the user-mode
//! runner. It is not: plan three's Q3 keeps that runner untouched, because it
//! is the FP and JIT oracle and its replays have to stay byte-identical. The
//! machine path, [`crate::mach::XtHart::run_slice`], is where the cache went,
//! mirroring the RV32 hart's `run_blocks` with the translated-core entry check
//! above the lookup.
//!
//! # Why the slot caches a decoded `Inst`
//!
//! Where the RV32 slot caches `(word, handler)` and keeps its fused decode
//! (`lp-riscv-emu/src/mach/block.rs`), Xtensa caches the **decoded [`Inst`]**
//! and skips [`lp_xt_inst::decode`] outright: the executors already run from
//! an `Inst`, so the slot is a genuine drop-in rather than a second dispatch
//! layer.
//!
//! # Not architectural state
//!
//! A slot is a memo of a decode. Nothing observable may depend on whether one
//! exists — see [`lp_emu_core::block`]'s module docs, which is the contract
//! the whole layer lives under and the reason the interpreter stays a usable
//! differential oracle.

use lp_emu_core::InstClass;
use lp_xt_inst::Inst;

use crate::emu::Flow;
use crate::executor::inst_class;
use crate::mach::window::ar_group;

/// One pre-decoded Xtensa instruction.
///
/// `class` is taken from the executors' own classification at decode time and
/// is fed to `CycleModel::cycles_for`, so this still works if a measured
/// Xtensa cycle model ever lands. Today the default is
/// [`lp_emu_core::CycleModel::InstructionCount`], where every class costs 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct XtSlot {
    /// What the instruction is. The executors take exactly this.
    pub inst: Inst,
    /// Bytes it occupies: 2 or 3 (Xtensa's narrow and base encodings). The
    /// block's byte span is the sum, and that span is what an invalidation
    /// compares against.
    pub len: u8,
    /// An **upper bound** on the cost class this slot can charge — see
    /// [`cost_bound`] for why it is a bound and not the exact class.
    pub class: InstClass,
    /// An **upper bound** on the 4-register group this slot's address-register
    /// operands reach — see [`ar_group_bound`] for the one instruction where
    /// it is a bound rather than the exact answer.
    pub group: u8,
    /// The maximum [`group`](Self::group) over the whole block — **valid on
    /// the block's first slot only**, and meaningless on any other.
    ///
    /// This is the per-block window precondition's input (M7 XD5). The plan
    /// forbids an Xtensa field on the arch-neutral
    /// [`lp_emu_core::block::Block`], so the answer rides the arena's first
    /// slot: it is the one slot the block executor reads before it runs
    /// anything, so the byte is already in cache when it is wanted.
    /// [`XtSlot::new`] leaves it equal to `group` — the right answer for a
    /// one-slot block — and `mach::block::decode_block` raises it to the
    /// block's maximum as it fills the arena.
    pub block_group: u8,
}

impl XtSlot {
    /// Pre-decode one instruction into a slot. `len` is the length
    /// [`lp_xt_inst::decode`] returned for it.
    #[inline]
    #[must_use]
    pub fn new(inst: Inst, len: u8) -> Self {
        let group = ar_group_bound(&inst);
        Self {
            inst,
            len,
            class: cost_bound(&inst),
            group,
            block_group: group,
        }
    }
}

impl lp_emu_core::block::Slot for XtSlot {
    #[inline]
    fn width(&self) -> u8 {
        self.len
    }

    #[inline]
    fn cost_bound(&self) -> InstClass {
        self.class
    }
}

/// The **upper bound** on what one instruction can charge, taken at decode
/// time.
///
/// [`inst_class`] takes the control-flow outcome as well as the instruction,
/// because a conditional branch's class depends on whether it was taken. At
/// decode time the outcome is unknown, so this asks for the *taken* answer:
/// `BranchTaken` is the more expensive of the pair under every cycle model
/// that distinguishes them, which is what makes it the bound.
///
/// Coarse is right here, exactly as it is on the RV32 side
/// (`lp_emu_core::block::Slot::cost_bound`): the bound gates only the
/// whole-block budget test, never a charge — the cycles actually charged
/// always come from the class the executor *returns* — so a bound that is too
/// generous costs a sliver of fast-path coverage near a slice deadline and can
/// never miscount a cycle. A bound that were too *small* would be a
/// correctness bug, which is why this defers to the executors' own
/// classification rather than keeping a second copy of it that can drift.
#[inline]
#[must_use]
pub fn cost_bound(inst: &Inst) -> InstClass {
    inst_class(inst, &Flow::Jump(0))
}

/// The **upper bound** on the 4-register group one instruction's address
/// registers reach, taken at decode time (M7 XD5).
///
/// [`crate::mach::window::ar_group`] is exact but takes `PS.CALLINC`, which is
/// live state, so a decode-time answer cannot always be the exact one. It is
/// exact for every instruction but **`ENTRY`**, whose group is
/// `max(group(as), PS.CALLINC)`: this returns 3 for `ENTRY`, the largest a
/// 2-bit `CALLINC` can contribute, and therefore an answer no live `CALLINC`
/// can exceed.
///
/// A bound in this direction is the safe one, exactly as [`cost_bound`]'s is.
/// The per-block precondition asks "can any slot in this block overflow?", and
/// a group that is too *large* can only make the answer "maybe" when the exact
/// answer was "no" — a block that runs slot by slot with the check it has
/// always had. A group that were too *small* would skip a check that should
/// have fired, which is a wrong answer, and that is why `ENTRY` rounds up
/// rather than storing `group(as)` and hoping.
///
/// The cost of rounding `ENTRY` up is a block that could have hoisted on a
/// `CALLINC` of 1 or 2 running slot by slot instead. `ENTRY` is a block
/// terminator, so it is one slot of one block, and its own check is the
/// interpreter's own either way.
#[inline]
#[must_use]
pub fn ar_group_bound(inst: &Inst) -> u8 {
    match inst {
        // `ar_group(Entry, callinc) = max(group(as), callinc & 3) <= 3`.
        Inst::Entry(..) => 3,
        // Every other arm ignores `ps_callinc`, so any value gives the exact
        // answer.
        _ => ar_group(inst, 0),
    }
}

/// How many blocks ran with the window overflow check **hoisted** — decided
/// once at block entry — and how many ran it slot by slot (M7 XD5).
///
/// A diagnostic, and nothing else: both paths retire the same instructions in
/// the same order with the same architectural state, so this pair says only
/// how much of a run took the cheap route. It is not architectural state, it
/// is absent from a snapshot, a clone starts at zero, and it is masked out of
/// every compared transcript along with the rest of the `blocks:` line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WindowHoistStats {
    /// Blocks entered with the per-slot overflow check skipped for the whole
    /// block, because no `WindowStart` bit was within reach of the block's
    /// maximum group (or because `PS.WOE` was clear / `PS.EXCM` set, where the
    /// per-instruction check is skipped anyway).
    pub hoisted: u64,
    /// Blocks entered with the per-slot check left exactly as the interpreter
    /// has always run it, because a bit **was** within reach.
    pub slotwise: u64,
}

impl WindowHoistStats {
    /// The share of blocks that took the hoisted path, 0.0 when none ran.
    #[must_use]
    pub fn hoisted_share(&self) -> f64 {
        let total = self.hoisted + self.slotwise;
        if total == 0 {
            0.0
        } else {
            self.hoisted as f64 / total as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use lp_emu_core::block::Slot as _;
    use lp_xt_inst::{BrRr, Inst, Reg};

    use super::*;

    fn decode_one(bytes: &[u8]) -> (Inst, usize) {
        lp_xt_inst::decode(bytes).expect("decodes")
    }

    /// A slot reports the width the decoder produced — 2 for a narrow
    /// encoding, 3 for a base one — and a conditional branch's `cost_bound` is
    /// the **taken** class, the upper bound, not the not-taken one.
    #[test]
    fn slot_width_and_cost_bound() {
        // `nop.n`: a 2-byte narrow encoding.
        let narrow = lp_xt_inst::encode(&Inst::NullaryN(lp_xt_inst::NullaryNarrowOp::NopN));
        let (inst, len) = decode_one(&narrow);
        assert_eq!(len, 2, "nop.n is a narrow encoding");
        let slot = XtSlot::new(inst, len as u8);
        assert_eq!(slot.width(), 2);

        // `nop`: a 3-byte base encoding.
        let base = lp_xt_inst::encode(&Inst::Nullary(lp_xt_inst::NullaryOp::Nop));
        let (inst, len) = decode_one(&base);
        assert_eq!(len, 3, "nop is a base encoding");
        let slot = XtSlot::new(inst, len as u8);
        assert_eq!(slot.width(), 3);
        assert_eq!(slot.cost_bound(), InstClass::Alu);

        // A conditional branch: the bound is the taken class.
        let br = Inst::BranchRr(BrRr::Beq, Reg::new(2), Reg::new(3), 4);
        let (inst, len) = decode_one(&lp_xt_inst::encode(&br));
        let slot = XtSlot::new(inst, len as u8);
        assert_eq!(
            slot.cost_bound(),
            InstClass::BranchTaken,
            "the bound must be the taken class; BranchNotTaken would be too small"
        );
        assert_ne!(slot.cost_bound(), InstClass::BranchNotTaken);
    }

    /// [`ar_group_bound`] is the exact [`ar_group`] for every instruction that
    /// does not read `PS.CALLINC`, and rounds the one that does — `ENTRY` — up
    /// to 3, the largest a 2-bit `CALLINC` can make it.
    ///
    /// The sweep over all four `CALLINC` values is the claim the hoist rests
    /// on: no live `CALLINC` can push a slot's group above the bound the block
    /// was folded from.
    #[test]
    fn the_decode_time_group_is_an_upper_bound_on_the_live_one() {
        use lp_xt_inst::{AluRrr, CallOp, LoadOp};

        for inst in [
            Inst::Nullary(lp_xt_inst::NullaryOp::Nop),
            Inst::Rrr(AluRrr::Or, Reg::new(2), Reg::new(3), Reg::new(4)),
            Inst::Rrr(AluRrr::Or, Reg::new(13), Reg::new(3), Reg::new(4)),
            Inst::Load(LoadOp::L32i, Reg::new(9), Reg::new(1), 0),
            Inst::Call(CallOp::Call12, 4),
            Inst::BranchRr(BrRr::Beq, Reg::new(2), Reg::new(3), 4),
        ] {
            for callinc in 0..4u8 {
                assert_eq!(
                    ar_group_bound(&inst),
                    ar_group(&inst, callinc),
                    "{inst:?} does not read CALLINC, so the bound is exact"
                );
            }
        }

        let entry = Inst::Entry(Reg::new(1), 16);
        assert_eq!(ar_group_bound(&entry), 3, "ENTRY rounds up");
        for callinc in 0..4u8 {
            assert!(
                ar_group(&entry, callinc) <= ar_group_bound(&entry),
                "CALLINC {callinc} must not reach past the bound"
            );
        }
    }

    /// The share is 0.0 on a run that entered no block, and the ratio
    /// otherwise.
    #[test]
    fn the_hoisted_share_is_zero_before_any_block_runs() {
        assert_eq!(WindowHoistStats::default().hoisted_share(), 0.0);
        let stats = WindowHoistStats {
            hoisted: 3,
            slotwise: 1,
        };
        assert!((stats.hoisted_share() - 0.75).abs() < f64::EPSILON);
    }
}
