//! The Xtensa block-cache slot: one pre-decoded instruction.
//!
//! This module defines the **type** [`lp_emu_core::block::BlockCache`] would
//! be parameterised by on this core, and nothing else. Wiring the cache into
//! [`crate::Emulator::run_loop`] — the table, the arena, invalidation through
//! `Memory`'s write path, the A/B — belongs to the speed ladder's "the Xtensa
//! core joins the block cache" phase, and this file exists so that phase and
//! the Xtensa emulator plan's translator seam name **one** slot rather than
//! two (ruling R2; this plan opened its PR first, so the type lives here).
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
}

impl XtSlot {
    /// Pre-decode one instruction into a slot. `len` is the length
    /// [`lp_xt_inst::decode`] returned for it.
    #[inline]
    #[must_use]
    pub fn new(inst: Inst, len: u8) -> Self {
        Self {
            inst,
            len,
            class: cost_bound(&inst),
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
}
