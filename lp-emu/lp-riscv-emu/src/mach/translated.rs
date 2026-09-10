//! The seam a **translated core** plugs into.
//!
//! A translated core is something that has turned the guest's own program
//! into host-executable code — for the ESP32-C6 that is `lp-emu-jit`,
//! translating RV32 to WebAssembly and letting the browser's engine run it.
//! This module is the hart's half of that arrangement and nothing else: it
//! knows a core can be installed, where it may be entered, and what it hands
//! back. It contains no translator and depends on none.
//!
//! # Not architectural state
//!
//! Nothing observable may depend on whether a block was translated. A
//! translated core is absent from snapshots, a cloned hart starts without
//! one, and running with no core installed — or with the core switched off —
//! must produce a byte-identical everything. That is the same contract
//! [`lp_emu_core::block::BlockCache`] lives under, for the same reason, and
//! it is what makes the interpreter a usable differential oracle.
//!
//! # Where a core may be entered
//!
//! At exactly one place: the top of the block-dispatch loop in
//! [`MachineHart::run_slice`](super::MachineHart::run_slice), where the
//! interpreter would otherwise look a block up. That is a point at which the
//! hart's architectural state is complete and consistent, and it is *before*
//! the block cache, so a translated entry costs one table read and one
//! compare on the interpreted path and nothing else.
//!
//! The four polling points in this crate's module docs do not move because a
//! core is installed. A core leaves at exactly the boundaries the interpreter
//! would have stopped at, and reports an after-store exit so the hart can
//! take polling point (c) itself.
//!
//! # Why a core is handed the whole hart
//!
//! P1 handed a core a small `EntryCx` — the register file, the pc and the two
//! counters — on the reasoning that a core which refuses should not be able to
//! reach anything else. P3 had to widen that, because of the **escape hatch**
//! (M7 JD10): translated code that meets an instruction it does not understand
//! calls back in to have the interpreter run exactly that one instruction, and
//! "exactly as the interpreter would" means traps, CSRs and interrupt
//! delivery. That is [`MachineHart::step_one`], and it needs the hart.
//!
//! A core cannot borrow the hart while the hart owns the core, so
//! [`MachineHart::run_slice`](super::MachineHart::run_slice)'s cached loop
//! **lifts the core out of the hart** for the length of a slice, exactly as it
//! already lifts the block cache out and for the same reason. Both go back on
//! every exit path. An invalidation asked for while a core is lifted out is
//! recorded and applied when it goes back — the same shape the block cache's
//! deferred flush uses, and `fence.i` is why both exist.
//!
//! What [`RunOutcome`] promises is unchanged by that widening: a core reports
//! where it left, what it charged and whether the exit was an after-store one,
//! and a core that refuses has still changed nothing. The one new obligation
//! is that a core which *does* run must leave the hart's own `pc`, `mcycle`
//! and `minstret` agreeing with what it reports, because the escape hatch will
//! have moved them.

extern crate alloc;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

use lp_emu_core::Bus;

use super::MachineHart;

/// What a translated core did.
pub enum RunOutcome {
    /// It ran, and left at `pc` with these counters.
    Ran {
        pc: u32,
        cycle_count: u64,
        instruction_count: u64,
        /// The last instruction retired was an MMIO store whose side-band or
        /// yield the hart must now observe — polling point (c). The hart does
        /// exactly what it does after an interpreted store: `take_sideband()`
        /// then `resample_external`, `take_yield()` then end the slice.
        after_store: bool,
    },
    /// Nothing ran and nothing changed. The interpreter continues at the
    /// entry `pc` as if no core were installed.
    ///
    /// This is not a failure. A core refuses whenever it cannot be exact —
    /// a load watchpoint is armed, more than one store watchpoint is, a
    /// side-band is already pending, the entry is not one it holds — and
    /// refusing is always safe.
    Refused,
}

/// A core that can execute guest instructions in the hart's place.
pub trait TranslatedCore<B: Bus> {
    /// Run from `hart.pc()` until the core decides to leave.
    ///
    /// `end` is the slice deadline, absolute. Not one instruction may
    /// **start** at or past it: that is the interpreter's rule (M5 MD3) and a
    /// translated core is held to the same one, per block, against the block's
    /// own maximum cost.
    ///
    /// The hart is handed over whole so the core can reach
    /// [`MachineHart::step_one`] — see this module's docs. The core is *not*
    /// installed on the hart it is given: the hart lifted it out before the
    /// call and puts it back afterwards, so `hart.has_translated_core()` reads
    /// `false` here and re-entering translated code from inside `run` is not
    /// possible by construction.
    fn run(&mut self, hart: &mut MachineHart<B>, bus: &mut B, end: u64) -> RunOutcome;

    /// Guest code may have changed: a `fence.i`, or a host-side write of
    /// guest code. Whatever the core holds for the affected addresses is no
    /// longer valid.
    ///
    /// `None` means "everything"; `Some((lo, hi))` is a half-open guest
    /// address range. A core that cannot track ranges may treat the range
    /// form as the whole — invalidating too much is slow, invalidating too
    /// little is wrong.
    fn invalidate(&mut self, range: Option<(u32, u32)>);

    /// One line for `--jit-report`: what the core translated, how much of the
    /// run it covered, and how often it left for the interpreter.
    fn report(&self) -> String;
}

/// `log2` of the hart's entry table, which is direct-mapped by `pc >> 1`
/// (RVC means block starts are 2-byte aligned, and 48.99 % of them sit at
/// 2 mod 4 — indexing by `pc >> 2` would collide half the image with itself).
///
/// 16 bits is 64 K slots against the ~37,600 distinct block starts a render
/// image executes. The table is a filter, not a map: a slot holds the one pc
/// that claimed it, so a hit means "ask the core", a miss means "do not", and
/// a collision costs an interpreted block rather than a wrong answer.
pub const ENTRY_TABLE_BITS: u32 = 16;

/// The number of slots in the entry table.
pub const ENTRY_TABLE_SLOTS: usize = 1 << ENTRY_TABLE_BITS;

/// A boxed [`TranslatedCore`].
pub type BoxedCore<B> = Box<dyn TranslatedCore<B>>;

/// The slot `pc` maps to.
#[inline(always)]
pub const fn entry_slot(pc: u32) -> usize {
    (pc >> 1) as usize & (ENTRY_TABLE_SLOTS - 1)
}

/// Build the hart's entry table from a core's entry pcs.
///
/// `0` means "no entry": pc 0 is not a legal entry on any machine this hart
/// runs, so it costs nothing to spend it as the empty marker. First claim
/// wins on a collision.
#[must_use]
pub fn entry_table(entries: &[u32]) -> Vec<u32> {
    let mut table = alloc::vec![0u32; ENTRY_TABLE_SLOTS];
    for &pc in entries {
        let slot = entry_slot(pc);
        if table[slot] == 0 {
            table[slot] = pc;
        }
    }
    table
}
