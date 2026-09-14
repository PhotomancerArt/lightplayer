//! The Xtensa half of the emulator's wasm translator.
//!
//! `lp-emu-jit` is the **ABI**: the exchange protocol, the four imports
//! (`mmio_load`, `mmio_store`, `step_one`, `poll`), the module shape and its
//! selector, the permission and indirect-target tables, the record/replay
//! shapes, and the two hosts. This crate supplies the two things that are
//! Xtensa's — a decoded form and a body emitter — and gets the rest. That is
//! XD7: **wasm is the IR**, the two architectures share an ABI and not an
//! intermediate representation, because neither one's hard part (windows here,
//! `fence.i` there) survives a register-machine IR without the same special
//! casing that made the IR look attractive.
//!
//! # What this phase emits
//!
//! Every instruction **escapes**. The module is the prologue, the dispatcher
//! loop, the per-block budget compare and a `step_one` call per guest
//! instruction, and it emits no guest semantics at all — the RV32 side's
//! [`Emit::NOTHING`](lp_emu_jit::translate::Emit::NOTHING) build, which that
//! side keeps as a test precisely because it is the one translation that is
//! *certainly* right. It is slower than the interpreter and it is not a
//! product path; what it proves is the **seam**: that a stay entered here
//! leaves the machine byte-for-byte where the interpreter would have.
//!
//! The guest semantics (M7 P06), the real discovery sweep (P05), the
//! publish-by-store event (P07) and the wasm build (P08) come later. The one
//! thing that must not move when they land is what this crate proves now.
//!
//! # The register model, and why nothing crosses
//!
//! The exchange area reserves the physical `AR[0..64]` file and the window,
//! loop and dirty-mask words ([`LAYOUT`], XD8's data half) because
//! [`translate`]'s successor will cache the current window in locals and write
//! back on a dirty mask. **This phase writes none of them.** With every
//! instruction escaping, the guest state never leaves the hart: the driver's
//! host overrides [`HostOps::step_one_wide`](lp_emu_jit::host::HostOps::step_one_wide)
//! and runs `XtHart::step_one` on the hart itself, so there is no register file
//! to marshal and no way for a marshalling bug to exist yet.
//!
//! That is also why `a3` is not a register here. On Xtensa `a3` is
//! `AR[(WindowBase * 4 + 3) mod 64]`, and a translator that cached "a3" across
//! an `ENTRY` would be wrong in a way no RV32 shape warns about. The exchange
//! area holds the **physical** file for exactly that reason.

#![no_std]

extern crate alloc;

pub mod blocks;
pub mod decode;
pub mod discover;
pub mod replay;
pub mod translate;

use lp_emu_jit::host::ExchangeLayout;

/// The Xtensa exchange layout — XD8's data half.
///
/// `regs_words = 64` is the physical `AR[0..64]` file (see the module docs for
/// why it is the physical file and not sixteen window registers), and the eight
/// extra words are the state a stay would otherwise have to ask the hart for on
/// every instruction: the window pair, `SAR`, the three loop registers,
/// `PS.CALLINC`, and the dirty mask an exit writes back on.
pub const LAYOUT: ExchangeLayout = ExchangeLayout {
    regs_words: 64,
    extra_words: EXTRA_WORDS,
};

/// How many words past the AR file the layout reserves. See [`extra`].
pub const EXTRA_WORDS: u32 = 8;

/// The extra words, by name.
///
/// A **wire format**, like the protocol's own fields: the emitted module folds
/// these in as constants and the driver reads the same bytes back, so the
/// numbers are hand-assigned and the driver and the emitter name them from
/// here rather than each counting for itself.
pub mod extra {
    /// `WindowBase`, in units of four registers (0..16).
    pub const WINDOW_BASE: u32 = 0;
    /// `WindowStart`, one bit per four-register frame.
    pub const WINDOW_START: u32 = 1;
    /// `SAR`.
    pub const SAR: u32 = 2;
    /// `LBEG`.
    pub const LBEG: u32 = 3;
    /// `LEND`.
    pub const LEND: u32 = 4;
    /// `LCOUNT`.
    pub const LCOUNT: u32 = 5;
    /// `PS.CALLINC`, the two bits `ENTRY` rotates by.
    pub const PS_CALLINC: u32 = 6;
    /// The four-register groups a stay wrote, one bit per group (P06).
    ///
    /// Reserved and unwritten in this phase: with every instruction escaping,
    /// the hart owns the register file and there is nothing to write back.
    pub const DIRTY: u32 = 7;
}

/// Where extra word `i` sits in the exchange area. See [`extra`].
#[must_use]
pub const fn extra(i: u32) -> u64 {
    LAYOUT.extra(i)
}
