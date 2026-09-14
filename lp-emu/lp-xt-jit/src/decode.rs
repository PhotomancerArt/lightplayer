//! The decoded form this crate emits from: `lp-xt-inst`'s `Inst` plus the
//! three questions `lp-emu-jit` asks of it.
//!
//! **There is no second decoder here** and there is no agreement test, which
//! is the one place this crate is cheaper than the RV32 side. `lp-emu-jit`
//! carries its own RV32 decoder because `lp-riscv-inst` was AGPL when M7 P3
//! was written, and it pays `tests/decoder_agreement.rs` for the privilege.
//! `lp-xt-inst` is MIT (#763), so [`lp_xt_inst::decode`] *is* the decoder and
//! the only thing this module adds is the classification.
//!
//! # The classification is P01's, restated
//!
//! [`lp_xt_emu::mach`]'s `block::classify` is `pub(super)` — the block cache's
//! own — so the three-way split below is written a second time here. That is a
//! real duplication and it is the cheapest of the options: the alternative is
//! publishing a machine-internal classification as an API, and the two answer
//! slightly different questions anyway (the cache asks what it may *memoise*,
//! this asks where a translated block may *end*).
//!
//! The split:
//!
//! - **Body** — the ALU, the immediates, `l32r`, the plain loads and stores,
//!   the FP families. A block runs through them.
//! - **Terminator** ([`Decoded::is_control`]) — the branches, the calls, the
//!   returns, `loop`, `entry`, `rotw`, `movsp`, the exception returns, `waiti`
//!   and `rsil`. The block ends **after** one.
//! - **Undecodable-for-the-translator** — `break`, `syscall`, `ill`, the
//!   atomics, the window load/stores, the TLB families, the external
//!   registers, MAC16, **and** the whole `rsr`/`wsr`/`xsr`/`rur`/`wur` family
//!   and `isync`. The block ends **before** one and the interpreter runs it,
//!   which is the RV32 side's `BlockEnd::Undecodable` shape.
//!
//! The `Sr`/`Ur` writes and `isync` are terminators for the *cache* and
//! undecodable for the *translator* because of what each is allowed to assume:
//! the cache re-reads `LBEG`/`LEND`/`LCOUNT` live on every instruction, and a
//! translated stay would have folded them in. Ending before them costs a block
//! boundary and removes the whole question.

use lp_emu_core::InstClass;
use lp_emu_jit::blocks::DecodedInst;
use lp_xt_inst::{AluRs, Inst, NullaryNarrowOp, NullaryOp};

/// One decoded Xtensa instruction, as this crate's emitter sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Decoded {
    /// The instruction itself. The emitter matches on this; the escape path
    /// never looks at it.
    pub inst: Inst,
    /// 2 or 3 bytes — [`lp_xt_inst::base_inst_len`]'s answer, kept because the
    /// block walk steps by it and re-deriving it would mean re-reading the
    /// first byte.
    pub width: u8,
    /// The cost class the budget check charges, as an **upper bound**: a
    /// branch is charged as taken.
    ///
    /// `lp_xt_emu::block::cost_bound` is the only public way to this answer —
    /// `executor::inst_class`, which takes the resolved control flow, is
    /// `pub(crate)`. The bound is the right answer for a *block* budget
    /// anyway: the block-entry compare asks what the block can cost at most,
    /// and the block cache computes its `max_cycles` from the same bound.
    pub class: InstClass,
    /// The highest four-register address group this instruction names, as a
    /// decode-time upper bound (`lp_xt_emu::block::ar_group_bound`).
    ///
    /// Carried now and unused until P06, where a block's maximum decides
    /// whether the window check can be hoisted over the whole block (XD5).
    pub group: u8,
    /// Does the block end **after** this instruction? See the module docs.
    pub control: bool,
}

impl DecodedInst for Decoded {
    /// **Byte granularity**, against RV32's two.
    ///
    /// Xtensa instructions are two or three bytes at any alignment, so all
    /// four `pc mod 4` residues are live in equal measure and a table indexed
    /// by anything coarser than the byte would miss most starts. It is the
    /// same answer, for the same reason, that
    /// `lp_xt_emu::mach::translated::ENTRY_TABLE_BITS` gives for the hart's
    /// own entry table.
    const SLOT_SHIFT: u32 = 0;

    fn width(&self) -> u8 {
        self.width
    }

    fn class(&self) -> InstClass {
        self.class
    }

    fn is_control(&self) -> bool {
        self.control
    }
}

/// What a decode attempt found at one address.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decode {
    /// A decoded instruction the translator will put in a block.
    Ok(Decoded),
    /// A decoded instruction the **translator** refuses: the block ends
    /// before it and the interpreter runs it. `width` is how far the walk
    /// steps to look for the next block start.
    Undecodable { width: u8 },
    /// The bytes do not decode at all. The walk cannot even step over it
    /// reliably, so `width` is the density rule's answer from the first byte —
    /// a walk that finds nothing rather than a walk that is wrong.
    Refused { width: u8 },
}

/// Decode one instruction from `bytes` (1 to 3 of them, as
/// [`lp_emu_core::Bus::fetch_bytes`] hands them back).
///
/// Never panics and never guesses: a truncated or unsupported encoding comes
/// back as [`Decode::Refused`] carrying the length the density rule gives, and
/// the caller ends the block there.
#[must_use]
pub fn decode(bytes: &[u8]) -> Decode {
    let density_width = match bytes.first() {
        Some(&b) => lp_xt_inst::base_inst_len(b) as u8,
        // Nothing was fetched at all. Three is the longest an instruction can
        // be, so stepping by it cannot land inside one this call could have
        // decoded.
        None => 3,
    };
    let Ok((inst, len)) = lp_xt_inst::decode(bytes) else {
        return Decode::Refused {
            width: density_width,
        };
    };
    let width = len as u8;
    if undecodable(&inst) {
        return Decode::Undecodable { width };
    }
    Decode::Ok(Decoded {
        inst,
        width,
        class: lp_xt_emu::block::cost_bound(&inst),
        group: lp_xt_emu::block::ar_group_bound(&inst),
        control: terminator(&inst),
    })
}

/// The block ends **before** this instruction and the interpreter runs it.
///
/// P01's `Class::Refused` list, plus the `Sr`/`Ur` family and `isync` — see
/// the module docs for why those three move across.
fn undecodable(inst: &Inst) -> bool {
    matches!(
        inst,
        Inst::Break(..)
            | Inst::BreakN(_)
            | Inst::Nullary(NullaryOp::Syscall)
            | Inst::Nullary(NullaryOp::Ill)
            | Inst::Nullary(NullaryOp::Isync)
            | Inst::NullaryN(NullaryNarrowOp::IllN)
            | Inst::AtomicLs(..)
            | Inst::WindowLs(..)
            | Inst::Tlb(..)
            | Inst::TlbInv(..)
            | Inst::ExtReg(..)
            | Inst::Mac(..)
            | Inst::MacLd(..)
            | Inst::MacLoad(..)
            | Inst::Sr(..)
            | Inst::Ur(..)
    )
}

/// The block ends **after** this instruction: it decides the next pc, rotates
/// the window, or changes what the next instruction means.
///
/// P01's `Class::Terminator` list minus the three that [`undecodable`] takes.
fn terminator(inst: &Inst) -> bool {
    matches!(
        inst,
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
            | Inst::Nullary(NullaryOp::Ret)
            | Inst::Nullary(NullaryOp::Retw)
            | Inst::NullaryN(NullaryNarrowOp::RetN)
            | Inst::NullaryN(NullaryNarrowOp::RetwN)
    )
}
