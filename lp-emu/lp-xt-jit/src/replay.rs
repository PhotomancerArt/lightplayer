//! The Xtensa record/replay shape.
//!
//! `lp-emu-jit`'s record became parameterised over the register count and the
//! first architectural number in XD6's pass, so the entry record, the outcome,
//! the memory-granule diff and the first-difference comparison are all the
//! ABI's and none of them is rewritten here. What Xtensa adds is two things:
//!
//! - the file is **64 registers starting at 0**, not 31 starting at 1. There
//!   is no hardwired-zero register to skip, and the file recorded is the
//!   *physical* `AR[0..64]` rather than the window — a record of "a3" would
//!   mean different registers in two entries with different `WindowBase`;
//! - the window, loop and shift state ([`Window`]) is architectural in a way
//!   RV32 has no equivalent of. It is recorded beside the file because a
//!   replay that seeds `AR` and not `WindowBase` has not seeded the machine.
//!
//! Nothing in this phase writes a record: the escape-everything module leaves
//! every instruction to the hart, so there is nothing a differential replay
//! could disagree about. The shape lands now because P06's emitted arms are
//! the first thing that can, and a record format invented alongside the bug it
//! is meant to catch is not an oracle.

use lp_emu_jit::replay;

/// How many registers an Xtensa record carries: the physical `AR` file.
pub const RECORDED_REGS: usize = 64;

/// The architectural number of the first recorded register.
///
/// Zero: `AR[0]` is an ordinary register. RV32 records from 1 because `x0` is
/// hardwired and recording it would be recording a constant.
pub const RECORDED_FIRST: u8 = 0;

/// `AR[0]`..`AR[63]`, in physical order.
pub type Regs = replay::Regs<RECORDED_REGS, RECORDED_FIRST>;

/// What one entry into translated code produced.
pub type EntryOutcome = replay::EntryOutcome<RECORDED_REGS, RECORDED_FIRST>;

/// One entry into translated code, recorded.
pub type EntryRecord = replay::EntryRecord<RECORDED_REGS, RECORDED_FIRST>;

/// A whole run's worth of entries, in the order they happened.
pub type ReplayRecord = replay::ReplayRecord<RECORDED_REGS, RECORDED_FIRST>;

/// The architectural state past the register file that a stay can move.
///
/// The same words, in the same order, as the exchange area's extras
/// ([`crate::extra`]) minus the dirty mask, which is the emitter's bookkeeping
/// and not the machine's state. A replay seeds these and compares them.
///
/// `PS` is carried **whole** rather than as `CALLINC` alone: the exchange area
/// needs only the two bits a stay rotates by, but a record is an oracle and an
/// `INTLEVEL` or `EXCM` that moved inside a stay is exactly the kind of
/// divergence it exists to catch.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Window {
    /// `WindowBase`, in units of four registers.
    pub window_base: u8,
    /// `WindowStart`, one bit per four-register frame.
    pub window_start: u16,
    /// `SAR`.
    pub sar: u32,
    /// `LBEG`.
    pub lbeg: u32,
    /// `LEND`.
    pub lend: u32,
    /// `LCOUNT`.
    pub lcount: u32,
    /// `PS`, whole.
    pub ps: u32,
}

/// The first field of [`Window`] that differs, named, for a report that says
/// *what* diverged rather than that something did.
#[must_use]
pub fn first_window_difference(mine: &Window, theirs: &Window) -> Option<(&'static str, u32, u32)> {
    let pairs: [(&'static str, u32, u32); 7] = [
        (
            "WindowBase",
            u32::from(mine.window_base),
            u32::from(theirs.window_base),
        ),
        (
            "WindowStart",
            u32::from(mine.window_start),
            u32::from(theirs.window_start),
        ),
        ("SAR", mine.sar, theirs.sar),
        ("LBEG", mine.lbeg, theirs.lbeg),
        ("LEND", mine.lend, theirs.lend),
        ("LCOUNT", mine.lcount, theirs.lcount),
        ("PS", mine.ps, theirs.ps),
    ];
    pairs
        .into_iter()
        .find(|(_, a, b)| a != b)
        .map(|(name, a, b)| (name, a, b))
}
