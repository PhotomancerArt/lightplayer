//! The seam a **translated core** plugs into.
//!
//! A translated core is something that has turned the guest's own program
//! into host-executable code — for the ESP32-C6 that is `lp-emu-jit`,
//! translating RV32 to WebAssembly and letting the browser's engine run it.
//! This module is the hart's half of that arrangement and nothing else: it
//! knows a core can be installed, where it may be entered, and what it hands
//! back. It contains no translator and depends on none.
//!
//! **Nothing plugs into it yet.** There is no Xtensa translator; this phase
//! (M1 P5) mirrors the RV32 seam so that when one opens it finds the same
//! shape rather than a design argument, and so the block cache and the
//! translator name one slot type ([`crate::block::XtSlot`]) rather than two.
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
//! At exactly one place: the top of the slice loop in
//! [`XtHart::run_slice`](super::XtHart::run_slice), where the interpreter
//! would otherwise fetch the next instruction — and where, once the block
//! cache lands, it would look a block up. That is a point at which the hart's
//! architectural state is complete and consistent, and it is *before* the
//! block cache, so a translated entry will cost one table read and one
//! compare on the interpreted path and nothing else.
//!
//! A hart with **no** core installed does not even pay that: `run_slice`
//! branches once per slice on `self.core.is_some()` and runs the loop it has
//! always run.
//!
//! The polling points in this crate's module docs do not move because a core
//! is installed. A core leaves at exactly the boundaries the interpreter would
//! have stopped at, and reports an after-store exit so the hart can take
//! polling point (c) itself.
//!
//! # Why a core is handed the whole hart
//!
//! The RV32 seam handed a core a small `EntryCx` at first — the register file,
//! the pc and the two counters — on the reasoning that a core which refuses
//! should not be able to reach anything else. That had to widen, because of
//! the **escape hatch** (M7 JD10): translated code that meets an instruction
//! it does not understand calls back in to have the interpreter run exactly
//! that one instruction, and "exactly as the interpreter would" means traps,
//! special registers and interrupt delivery. That is
//! [`XtHart::step_one`](super::XtHart::step_one), and it needs the hart. This
//! seam mirrors the widened shape, not the one the phase brief sketched — see
//! the PR body.
//!
//! On Xtensa the widening also settles a question `EntryCx` could not have
//! answered cheaply: **`a3` is not a register**. It is
//! `AR[(WindowBase * 4 + 3) mod 64]`, so a core that runs guest code has to
//! leave the physical AR file, `WindowBase` and `WindowStart` exactly where
//! the interpreter will look for them — and there is no `x0`-reads-zero
//! invariant to preserve in exchange. Handing over
//! [`XtHart::cpu_mut`](super::XtHart::cpu_mut) makes all three one object with
//! one owner.
//!
//! A core cannot borrow the hart while the hart owns the core, so
//! [`XtHart::run_slice`](super::XtHart::run_slice) **lifts the core out of the
//! hart** for the length of a slice, exactly as the RV32 hart does and for the
//! same reason. It goes back on every exit path. An invalidation asked for
//! while a core is lifted out is recorded and applied when it goes back.
//!
//! # The three invalidation events
//!
//! RV32's design is two-event translation: boot, and `fence.i`. **Xtensa needs
//! three.**
//!
//! 1. **Boot** — nothing is translated until something translates it.
//! 2. **`isync`**, the Xtensa analogue of `fence.i`: the guest has published
//!    instructions. `lp-xt-emu`'s executors treat `isync` as a no-op barrier,
//!    so the hart intercepts it (`exec::is_hart_owned`) purely to raise this
//!    event; the instruction's class, flow and trace are unchanged.
//! 3. **A write to `LBEG`, `LEND` or `LCOUNT`** — this one has no RV32
//!    counterpart and it is the one most likely to be dropped. A translated
//!    block may have inlined the `LEND` it saw, and `restore_context` rewrites
//!    all three on **every context switch** (xtensa-lx-rt 0.22.0
//!    `src/exception/asm.rs`, inside the exception vector region). The hart
//!    therefore invalidates on any `wsr`/`xsr` to those three registers. The
//!    mechanism is cheap and the failure it prevents is silent.
//!
//! Two things are deliberately **left to whoever wires the cache**, and are
//! named here so they are decisions rather than omissions:
//!
//! - **The `LOOP` instruction** also writes all three registers, and it is
//!   hot. Invalidating on every `loop` would be correct and slow. A translator
//!   that inlines a loop-back sees the `loop` in its own block and can account
//!   for it; the `wsr` case is the one it cannot see.
//! - **The store path.** RV32 gets its correctness from the guest's own fence;
//!   the classic's JIT writes code into a fixed SRAM0 region and was measured
//!   on silicon to need no barrier at all. Whether Xtensa also needs
//!   store-address invalidation is the speed ladder's question, not this
//!   phase's.
//!
//! # Timers, and the one Xtensa polling point a core cannot take
//!
//! Poll point (e) — a `CCOMPARE` match — is inside the core, not on the bus,
//! and the interpreter tests it after every retire. A translated stay cannot,
//! so the hart latches and polls once when the stay ends. A core that wants to
//! be exact about *when* a timer interrupt is delivered must leave at or
//! before [`XtHart::next_timer_cycle`](super::XtHart::next_timer_cycle);
//! refusing is always available and always safe.

use lp_emu_core::Bus;

use super::XtHart;

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
        /// then re-read [`Bus::pending_cpu_interrupt`], `take_yield()` then
        /// end the slice.
        after_store: bool,
    },
    /// It ran, and the last thing it did **ended the slice**.
    ///
    /// A core that calls [`XtHart::step_one`](super::XtHart::step_one) is
    /// running arbitrary guest instructions, and some of those end a slice
    /// rather than retiring into the next one: a `waiti`, a `break`, a bus
    /// yield, a double fault. The interpreter's own loop returns those and a
    /// translated core does not get to swallow them, so it hands the
    /// [`SliceEnd`](super::SliceEnd) back untouched and the hart returns it.
    ///
    /// `pc` and the counters are applied exactly as for [`RunOutcome::Ran`]
    /// first, so the machine resumes where the interpreter would have.
    Ended {
        pc: u32,
        cycle_count: u64,
        instruction_count: u64,
        end: super::SliceEnd,
    },
    /// Nothing ran and nothing changed. The interpreter continues at the
    /// entry `pc` as if no core were installed.
    ///
    /// This is not a failure. A core refuses whenever it cannot be exact —
    /// a load watchpoint is armed, more than one store watchpoint is, a
    /// side-band is already pending, a `CCOMPARE` match falls inside the
    /// stay, the entry is not one it holds — and refusing is always safe.
    Refused,
}

/// A core that can execute guest instructions in the hart's place.
pub trait TranslatedCore<B: Bus> {
    /// Run from `hart.pc()` until the core decides to leave.
    ///
    /// `end` is the slice deadline, absolute. Not one instruction may
    /// **start** at or past it: that is the interpreter's rule and a
    /// translated core is held to the same one, per block, against the block's
    /// own maximum cost.
    ///
    /// The hart is handed over whole so the core can reach
    /// [`XtHart::step_one`](super::XtHart::step_one) — see this module's docs.
    /// The core is *not* installed on the hart it is given: the hart lifted it
    /// out before the call and puts it back afterwards, so
    /// `hart.has_translated_core()` reads `false` here and re-entering
    /// translated code from inside `run` is not possible by construction.
    ///
    /// **No tracer reaches here.** `XtHart::run_slice_traced` is how the
    /// fixtures' goldens are captured, and a core running host code cannot
    /// emit an instruction-by-instruction trace; the escape hatch runs against
    /// [`crate::NoopTracer`]. A run whose trace must be the interpreter's runs
    /// with no core installed, which is the same `--interpreter` posture the
    /// whole seam is built around.
    fn run(&mut self, hart: &mut XtHart<B>, bus: &mut B, end: u64) -> RunOutcome;

    /// Guest code may have changed: an `isync`, a write to `LBEG`/`LEND`/
    /// `LCOUNT`, or a host-side write of guest code. Whatever the core holds
    /// for the affected addresses is no longer valid.
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

/// `log2` of the hart's entry table, which is direct-mapped **by byte**.
///
/// The RV32 table indexes by `pc >> 1` and justifies it with a measurement:
/// RVC makes block starts 2-byte aligned and 48.99 % of them sit at 2 mod 4,
/// so `pc >> 2` would collide half that image with itself. **The Xtensa answer
/// is the opposite one.** Instructions are 2 or 3 bytes at any alignment, and
/// over 721,066 instructions of the firmware image all four residues are live
/// in equal measure — 25.37 / 25.02 / 24.74 / 24.87 % at 0/1/2/3 mod 4, with
/// branch and call targets distributed the same way (the ISA study's §2.3). A
/// copied `pc >> 1` would therefore collide *half of everything*, so this
/// table indexes by the byte address.
///
/// 16 bits is 64 K slots. The table is a filter, not a map: a slot holds the
/// one pc that claimed it, so a hit means "ask the core", a miss means "do
/// not", and a collision costs an interpreted block rather than a wrong
/// answer.
pub const ENTRY_TABLE_BITS: u32 = 16;

/// The number of slots in the entry table.
pub const ENTRY_TABLE_SLOTS: usize = 1 << ENTRY_TABLE_BITS;

/// A boxed [`TranslatedCore`].
pub type BoxedCore<B> = Box<dyn TranslatedCore<B>>;

/// The slot `pc` maps to — by byte; see [`ENTRY_TABLE_BITS`].
#[inline(always)]
pub const fn entry_slot(pc: u32) -> usize {
    pc as usize & (ENTRY_TABLE_SLOTS - 1)
}

/// Build the hart's entry table from a core's entry pcs.
///
/// `0` means "no entry": pc 0 is not a legal entry on any machine this hart
/// runs — the reset vector is `0x4000_0400` and every image this emulator
/// loads sits far above zero — so it costs nothing to spend it as the empty
/// marker. First claim wins on a collision.
#[must_use]
pub fn entry_table(entries: &[u32]) -> Vec<u32> {
    let mut table = vec![0u32; ENTRY_TABLE_SLOTS];
    for &pc in entries {
        let slot = entry_slot(pc);
        if table[slot] == 0 {
            table[slot] = pc;
        }
    }
    table
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The trap this phase exists to avoid: four consecutive pcs must map to
    /// four **distinct** slots. `pc >> 1` — the RV32 rule — collides `p` with
    /// `p + 1` and `p + 2` with `p + 3`, which on Xtensa is half of every
    /// entry the table holds.
    #[test]
    fn entry_slot_all_residues_distinct() {
        let p = 0x4037_8123u32;
        let slots = [
            entry_slot(p),
            entry_slot(p + 1),
            entry_slot(p + 2),
            entry_slot(p + 3),
        ];
        for i in 0..slots.len() {
            for j in (i + 1)..slots.len() {
                assert_ne!(
                    slots[i], slots[j],
                    "pc+{i} and pc+{j} share a slot — the table is not byte-indexed"
                );
            }
        }
        // And the mapping is the byte address itself, masked.
        assert_eq!(slots[0], p as usize & (ENTRY_TABLE_SLOTS - 1));
    }

    /// Collisions keep the first pc that claimed the slot, and `0` means "no
    /// entry".
    #[test]
    fn entry_table_first_claim_wins() {
        let first = 0x4000_1000u32;
        // Same low 16 bits, so the same slot.
        let colliding = first + (ENTRY_TABLE_SLOTS as u32);
        assert_eq!(entry_slot(first), entry_slot(colliding));

        let table = entry_table(&[first, colliding]);
        assert_eq!(table[entry_slot(first)], first, "first claim wins");
        assert_eq!(
            table.iter().filter(|&&pc| pc != 0).count(),
            1,
            "the colliding entry claimed nothing else"
        );
        // Everything else is the empty marker.
        assert_eq!(table[entry_slot(first) ^ 1], 0);
        assert_eq!(table.len(), ENTRY_TABLE_SLOTS);
    }
}
