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
//!
//! **Keep this in step with `lp_xt_emu::mach::block::classify`.** P05's sweep
//! extends the restatement in two ways that the cache's classification does
//! not need and does not have: [`edges`] derives each terminator's static
//! targets, and [`Decoded::lbeg`] marks the one instruction the decoder cannot
//! see as a terminator — the last of a zero-overhead loop body, which the
//! *walk* finds by decoding the `loop` that names its `LEND`. P10 documents
//! the duplication.
//!
//! # The width of a refused instruction
//!
//! [`Decode::Undecodable`] carries the width of an instruction the translator
//! refuses but the decoder decoded — a `wsr`, an `isync`, a `break`. That
//! width is exact, because the decoder produced it, and the sweep uses it to
//! **step over** the refused instruction to the next block start. It is the
//! P05 brief's `refused_width`, living on the enum arm P04 already gave it
//! rather than as a field on [`Decoded`]. [`Decode::Refused`] — bytes the
//! decoder does not decode at all — carries only the density rule's guess,
//! and the sweep does **not** step over those: on Xtensa the length of an
//! unknown encoding is not knowable from its first byte (`op0 = 14/15` are
//! reserved formats), so a refused word ends the walk there and the next seed
//! carries on (JD7: never guess a width).

use lp_emu_core::InstClass;
use lp_emu_jit::blocks::DecodedInst;
use lp_xt_inst::{AluRs, Inst, NullaryNarrowOp, NullaryOp};

/// The decoder this crate's decoded form is built on, re-exported so a
/// machine driver that matches on [`Decoded::inst`] names the same crate at
/// the same version rather than taking a second dependency edge.
pub use lp_xt_inst;

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
    /// `Some(LBEG)` when the address after this instruction is a `LEND` some
    /// `loop`/`loopnez`/`loopgtz` in the walk named (study §2.2, XD9).
    ///
    /// The hart's loop-back — `if LCOUNT != 0 && next == LEND { pc = LBEG }` —
    /// is invisible to the decoder: no branch is encoded at `LEND`. The
    /// **sweep** sets this when it has decoded the `loop` that names the
    /// address, and sets [`control`](Self::control) with it, so the block ends
    /// here with a static back-edge to `LBEG` that P06's emitter can turn into
    /// a counter compare. This phase's emitter only sees `control` and exits;
    /// the interpreter has already done the loop-back, so the exit pc is the
    /// right one either way.
    ///
    /// Never set by [`decode`]: it is a property of the *address*, known only
    /// once the walk has seen the loop.
    pub lbeg: Option<u32>,
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
    /// steps to look for the next block start — exact, because the decoder
    /// gave it — and `inst` is what was refused, so the walk can decline to
    /// step over the one refused instruction nothing follows
    /// ([`ends_the_walk`]).
    Undecodable { inst: Inst, width: u8 },
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
        return Decode::Undecodable { inst, width };
    }
    Decode::Ok(Decoded {
        inst,
        width,
        lbeg: None,
        class: lp_xt_emu::block::cost_bound(&inst),
        group: lp_xt_emu::block::ar_group_bound(&inst),
        control: terminator(&inst),
    })
}

/// The static edges one instruction at `pc` names — what the sweep follows.
///
/// `lp-xt-inst` stores every pc-relative field **raw** so that
/// `encode(decode(w)) == w` holds independent of pc, which means the absolute
/// targets are derived here and nowhere else in this crate: the emitter that
/// resolves a branch in-module (P06) asks this same function, so the walk and
/// the emitted compare cannot disagree about where an edge goes.
///
/// The formulas are the RM's, and `tests/discover.rs` checks each arm against
/// [`lp_xt_inst::disasm::format_inst`], which resolves the same targets for
/// the disassembler:
///
/// - every branch and `j`: `pc + 4 + offset` (the `+4` is the architectural
///   base, not the instruction's width — a 2-byte `beqz.n` uses it too);
/// - `call0/4/8/12`: `(pc & !3) + (offset << 2) + 4`;
/// - `loop`/`loopnez`/`loopgtz`: `LEND = pc + 4 + imm8`
///   ([`lp_xt_inst::disasm::loop_end`], the one formula kept in the decoder
///   crate because the hart needs it too), and `LBEG = pc + 3`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edges {
    /// Nothing static: an indirect jump, a return, an exception return, or a
    /// body instruction.
    None,
    /// `j`: one target, no fall-through.
    Jump(u32),
    /// A conditional branch: the target and the fall-through.
    Branch { target: u32, next: u32 },
    /// A call. `target` is `None` for `callx*`, which names no callee; `ret`
    /// is the address the callee returns to and is always a block start.
    Call { target: Option<u32>, ret: u32 },
    /// A zero-overhead loop: `body` is `LBEG` (the instruction after the
    /// `loop`), `end` is `LEND`.
    Loop { body: u32, end: u32 },
    /// `entry` and the other terminators that fall straight through: the
    /// next instruction is a start.
    Next(u32),
}

/// The static edges the instruction at `pc` names. See [`Edges`].
#[must_use]
pub fn edges(pc: u32, d: &Decoded) -> Edges {
    let next = pc.wrapping_add(u32::from(d.width));
    let branch = |off: i32| pc.wrapping_add(4).wrapping_add(off as u32);
    match d.inst {
        Inst::J(off) => Edges::Jump(branch(off)),
        Inst::BranchRr(_, _, _, off)
        | Inst::BranchRi(_, _, _, off)
        | Inst::BranchRiu(_, _, _, off)
        | Inst::BranchZ(_, _, off)
        | Inst::BranchBiI(_, _, _, off)
        | Inst::BranchBool(_, _, off) => Edges::Branch {
            target: branch(off),
            next,
        },
        Inst::BranchZN(_, _, imm6) => Edges::Branch {
            target: branch(imm6 as i32),
            next,
        },
        Inst::Call(_, words) => Edges::Call {
            target: Some((pc & !3).wrapping_add((words as u32) << 2).wrapping_add(4)),
            ret: next,
        },
        Inst::Callx(..) => Edges::Call {
            target: None,
            ret: next,
        },
        Inst::Loop(_, _, imm8) => Edges::Loop {
            body: next,
            end: lp_xt_inst::disasm::loop_end(pc, imm8),
        },
        // `entry`, `rotw`, `movsp`, `rsil`, `waiti`: the block ends after
        // them (they rotate or change what the next instruction means) and
        // control goes straight on.
        Inst::Entry(..)
        | Inst::Rotw(_)
        | Inst::Rs(AluRs::Movsp, ..)
        | Inst::Rsil(..)
        | Inst::Waiti(_) => Edges::Next(next),
        _ => Edges::None,
    }
}

/// A refused instruction the sweep does **not** step over: `ill` and `ill.n`.
///
/// Every other refused instruction is followed by code — a `wsr` retires and
/// control goes on, a `break` is stepped past when the debugger resumes — so
/// the address after it is a real block start (rule 7). `ill` is not: it is
/// the compiler's trap, the instruction after it is reached by an edge or a
/// symbol if at all, and — the reason this exists — **`00 00 00` decodes as
/// `ill`**. A walk that stepped over it would march through every zeroed
/// word of executable RAM three bytes at a time, one empty start per word:
/// measured on the classic at boot, 33,000 starts in SRAM0's unwritten half
/// from one zero-sized label. Harmless (JD7) and pure waste.
#[must_use]
pub fn ends_the_walk(inst: &Inst) -> bool {
    matches!(
        inst,
        Inst::Nullary(NullaryOp::Ill) | Inst::NullaryN(NullaryNarrowOp::IllN)
    )
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
