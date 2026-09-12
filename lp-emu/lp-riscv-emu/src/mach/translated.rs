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

use alloc::collections::BTreeMap;

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
        ///
        /// **This is the fallback, not the usual path (M7b P2).** A core that
        /// can reach the hart runs the polling point itself, inside the stay,
        /// through [`MachineHart::resample_external`] — so it never has to
        /// leave for one, and it reports `false` here. The arm stays because a
        /// core that *cannot* — a host without a hart to poll on — is still a
        /// correct core, and because this is the contract the two answers are
        /// compared against.
        after_store: bool,
    },
    /// It ran, and the last thing it did **ended the slice**.
    ///
    /// P1 had no variant for this because P1 had no escape hatch. A core that
    /// calls [`MachineHart::step_one`] is running arbitrary guest
    /// instructions, and some of those end a slice rather than retiring into
    /// the next one: a `wfi`, an `ebreak`, a bus yield, a double fault. The
    /// interpreter's own loop returns those from `step_once` and a translated
    /// core does not get to swallow them, so it hands the [`SliceEnd`] back
    /// untouched and the hart returns it.
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

    /// The **short** form of the same numbers, for a run that asked for
    /// nothing (M7 P7).
    ///
    /// Since the wasm build's core *is* the translator, a default run is no
    /// longer an experiment somebody opted into and can be expected to read a
    /// 30-field diagnostic. What it needs is one line: what got built, what
    /// share of the run ran inside it, and how often it left — because an
    /// escape hatch that is allowed to be non-zero (JD10) is not allowed to be
    /// unmeasured.
    ///
    /// It is a *view* of [`TranslatedCore::report`]'s own fields and not a
    /// second set of counters — there is one metrics path and this is its
    /// summary. `None`, the default, means "nothing worth a line", which is
    /// every test double in this crate.
    ///
    /// `retired_total` is the hart's own `minstret`, handed in for the same
    /// reason [`TranslatedCore::retired`] is a bare number: the coverage share
    /// is a fraction whose denominator the core does not have.
    fn summary(&self, retired_total: u64) -> Option<String> {
        let _ = retired_total;
        None
    }

    /// Guest instructions retired **inside** translated code.
    ///
    /// The numerator of the coverage number M7's P4 has a bar under (JD6);
    /// the denominator is the hart's own `minstret`. A number rather than a
    /// line in [`TranslatedCore::report`] because the machine has to divide
    /// by something only it knows.
    fn retired(&self) -> u64;

    /// A way back to the concrete core, for the machine crate that built it.
    ///
    /// This crate owns the seam and contains no translator, so there is
    /// nothing it can usefully do with the answer — and that is the point.
    /// M7b P1's incremental translation installs an **additional module**
    /// beside the ones a core already holds, which is a conversation between
    /// the machine crate and its own core about a shape this crate has no
    /// business knowing. The alternative was a `fn extend(…)` on this trait
    /// spelled in terms of block sets and wasm modules, which would put the
    /// translator's vocabulary in the hart.
    ///
    /// The default is `None`, so a core that has no such conversation — every
    /// test double in this crate — implements nothing.
    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        None
    }
}

/// A boxed [`TranslatedCore`].
pub type BoxedCore<B> = Box<dyn TranslatedCore<B>>;

/// `log2` of the granularity the entry index's page map works at: one entry
/// per 16 KiB of guest address space, the same page as the translator's
/// permission table, so one shift serves both.
pub const ENTRY_PAGE_SHIFT: u32 = 14;
/// Entries in the page map: the whole 32-bit space, so a wild pc needs no
/// bounds compare.
pub const ENTRY_PAGES: usize = 1 << (32 - ENTRY_PAGE_SHIFT);
/// `u64` words per page: one bit per 2-byte-aligned address in it.
const WORDS_PER_PAGE: usize = (1 << ENTRY_PAGE_SHIFT) / 2 / 64;

/// "May the hart enter translated code at this pc?", answered **exactly**, in
/// two loads.
///
/// # Why this is not a filter any more
///
/// It was one: 64 K slots, direct-mapped by `pc >> 1`, first claim wins,
/// sized in M7 P4 against the ~37,600 distinct block starts a render image
/// executes. A collision cost an interpreted block, which was a fair trade
/// while a module held two thousand of them.
///
/// P5 installs the **whole image** — 155,608 blocks on `render-basic` — and
/// 155,608 pcs into 65,536 slots is not a filter, it is a 2.4× oversubscribed
/// one that leaves under two fifths of the module's blocks reachable at all.
/// Coverage measured 58.87 % with every block installed, and the exit census
/// said why: the hart kept leaving translated code at a pc that *was* a block
/// start and then could not get back in. So the index is exact.
///
/// # The shape
///
/// A page map over the whole 32-bit space at [`ENTRY_PAGE_SHIFT`], holding
/// the word offset of that page's bitmap; a bitmap per 16 KiB page that holds
/// at least one entry, one bit per 2-byte-aligned address; and **one shared
/// all-zero bitmap** every other page points at, so a pc anywhere in the 4 GiB
/// space is answered by the same two loads with no bounds check and no branch
/// of its own.
///
/// 1 MiB for the page map plus 1 KiB per populated page — about 1.6 MiB for a
/// render image, against 16 MiB for the 4-byte-per-slot table that would
/// answer the same question directly.
///
/// Two-byte granularity is not an economy: **48.99 % of real block starts sit
/// at 2 mod 4**, so a four-byte index would collide half the image with
/// itself.
#[derive(Clone, Debug, Default)]
pub struct EntryIndex {
    /// Word offset into [`Self::bits`] of each page's bitmap.
    pages: Vec<u32>,
    bits: Vec<u64>,
}

impl EntryIndex {
    /// Build the index from a core's entry pcs.
    #[must_use]
    pub fn build(entries: &[u32]) -> Self {
        let mut pages = alloc::vec![0u32; ENTRY_PAGES];
        // Word 0 is the shared "nothing starts on this page" bitmap, so every
        // page map entry has something valid to point at before any page is
        // placed.
        let mut bits = alloc::vec![0u64; WORDS_PER_PAGE];
        let mut placed: BTreeMap<u32, u32> = BTreeMap::new();
        for &pc in entries {
            let page = pc >> ENTRY_PAGE_SHIFT;
            let at = *placed.entry(page).or_insert_with(|| {
                let at = bits.len() as u32;
                bits.resize(bits.len() + WORDS_PER_PAGE, 0);
                pages[page as usize] = at;
                at
            }) as usize;
            let bit = ((pc & ((1 << ENTRY_PAGE_SHIFT) - 1)) >> 1) as usize;
            bits[at + (bit >> 6)] |= 1u64 << (bit & 63);
        }
        Self { pages, bits }
    }

    /// Whether the hart may enter at `pc`. Exact: no false positives and no
    /// false negatives.
    #[inline(always)]
    #[must_use]
    pub fn contains(&self, pc: u32) -> bool {
        if self.pages.is_empty() {
            return false;
        }
        let at = self.pages[(pc >> ENTRY_PAGE_SHIFT) as usize] as usize;
        let bit = ((pc & ((1 << ENTRY_PAGE_SHIFT) - 1)) >> 1) as usize;
        self.bits[at + (bit >> 6)] >> (bit & 63) & 1 != 0
    }

    /// What the index costs, in bytes. Reported, because it is per machine and
    /// a phone pays it.
    #[must_use]
    pub fn bytes(&self) -> usize {
        self.pages.len() * 4 + self.bits.len() * 8
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pages.is_empty()
    }
}
