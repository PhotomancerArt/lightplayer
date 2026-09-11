//! `XtHart` — one Xtensa (LX6/LX7) hart with the privileged machinery.
//!
//! This is the privileged half of the emulator: architectural state for a
//! single hart (the 64-entry physical AR file with `WindowBase`/`WindowStart`,
//! `PS` as a real register, the special-register file, three `CCOMPARE`
//! timers, two `DBREAK` and two `IBREAK` slots, cycle and instruction
//! counters) plus the exception, window-exception, interrupt, `waiti` and
//! return-instruction behaviour the Xtensa ISA Reference Manual defines. It
//! runs instruction **slices** against a [`Bus`], reusing the crate's
//! user-mode executors for every unprivileged instruction and handling the
//! privileged and machine-only families itself (`exec.rs`).
//!
//! It is **arch-only**. No MMIO, no SoC knowledge. The classic's interrupt
//! matrix, timer groups, UARTs and friends live above it, in the machine
//! crates; all this hart knows about them is one bitmask — "the CPU
//! interrupt lines currently asserted" — handed to it through
//! [`XtHart::set_external_mask`] (ruling R5: M2 generalises the bus side of
//! this to the same shape). Each line's *level* and *type* are core
//! configuration and arrive through [`CoreConfig`], never as constants here.
//!
//! The fetch is [`Bus::fetch_bytes`] and nothing else: Xtensa instructions
//! are 2 or 3 bytes at any alignment, so there is no instruction *word* for
//! [`Bus::fetch_instruction`] to return, and this hart never calls it.
//!
//! # Where interrupts are polled
//!
//! There is deliberately **no per-instruction privilege check**. Pending
//! interrupt state is examined at exactly these points:
//!
//! - **(a)** on entry to [`XtHart::run_slice`];
//! - **(b)** after the instructions that can turn delivery on: `rfe`, `rfi`,
//!   `rfwo`, `rfwu` (each clears `PS.EXCM`, which lowers `CINTLEVEL`),
//!   `waiti`, `rsil`, and any `wsr`/`xsr` to **PS**, **INTENABLE** or
//!   **INTSET** (the last raises a software interrupt, which is delivery
//!   turning on from the other side);
//! - **(c)** after a Store-, Atomic- or System-class instruction whose bus
//!   reports [`Bus::take_sideband`] `== true`: the hart re-reads
//!   [`Bus::pending_cpu_interrupt`], **replaces** its asserted-line mask with
//!   it, and polls — which is how an MMIO store that raises a peripheral
//!   line is delivered before the next instruction retires, and how one
//!   that lowers a line stops being pending in the same breath. Until M2
//!   generalises that bus method to a bitmask, `Some(n)` is read as "line
//!   `n` asserted and nothing else" — right for a single-source bus and
//!   enough for the RAM-only fixtures;
//! - **(d)** whenever the owning machine calls [`XtHart::poll_interrupts`]
//!   at a scheduler event;
//! - **(e)** — Xtensa only — when an internal `CCOMPARE` timer matches
//!   `CCOUNT`. The timers are inside the core (RM §4.4.6), not on the bus,
//!   so no side-band can announce them; the hart raises the line itself and
//!   polls in the same instruction. This is the one addition to the RV32
//!   list, and it is the C6's SYSTIMER side-band moved inside the core.
//!
//! Nothing else in the slice loop looks at interrupt state.
//!
//! # The reset-state contract
//!
//! [`XtHart::new`] leaves **`PS = 0x0000_001F`** — `INTLEVEL = 15`,
//! `EXCM = 1`, `WOE = 0`, `UM = 0`, `CALLINC = 0` — the architectural reset
//! value (RM Table 5-139 with the Interrupt Option). `VECBASE` and `pc` are
//! the [`CoreConfig`]'s `reset_vecbase` / `reset_pc`: on both the LX6 and
//! the LX7 those are `0x4000_0000` and `0x4000_0400` (`_ResetVector` in the
//! mask ROM; `XCHAL_VECBASE_RESET_VADDR` / `XCHAL_RESET_VECTOR_VADDR`), but
//! they are the chip's numbers, so the machine supplies them. `CCOUNT` and
//! `CCOMPARE0..2` are architecturally undefined at reset and are 0 here.
//!
//! **The machine, not the hart, seeds the "as the bootloader left it" PS**
//! for a direct load, exactly as the C6 machine seeds `mstatus = 0x1888`:
//! [`XtHart::set_ps_raw`]`(`[`sr::PS_BOOT`]`)` = `PS_WOE | PS_UM =
//! 0x0004_0020`. The reason is concrete — `xtensa-lx-rt`'s app entry begins
//! with `entry a1, 0x10` and uses `call4`/`callx4` at once, which needs
//! `PS.WOE = 1` and `PS.EXCM = 0`. A hart left at `0x1F` raises an illegal
//! instruction on the app's first instruction (ruling R3: the RM calls
//! `ENTRY` with `PS.WOE = 0` undefined and names raising as the debugging
//! aid; this hart raises). That is the Xtensa twin of "the firmware will
//! idle forever in `wfi`".
//!
//! # Cycles, and what is *not* counted
//!
//! `cycle_count` advances by [`lp_emu_core::CycleModel::cycles_for`] for
//! every instruction the hart *attempts*, including one that traps — an
//! instruction that faults still cost a fetch and a decode. `instruction_count`
//! advances only for instructions that **retire**. Exception entry, window
//! exceptions, interrupt entry and `rfe`/`rfi`/`rfwo`/`rfwu` carry no extra
//! cost: there is no measurement behind a number for those, and an invented
//! one would be indistinguishable from a measured one six months from now.
//! `CCOUNT` is this counter seen through a writable offset (`timer.rs`).
//!
//! # What is deliberately absent
//!
//! - **Rings.** `PS.RING` is stored and ignored: `CRING` is always 0, so no
//!   `PrivilegedCause` (8) is ever raised and every privileged instruction
//!   runs. The firmware never leaves ring 0.
//! - **The block cache.** [`XtHart::invalidate_block_range`] and
//!   [`XtHart::invalidate_blocks`] exist for the contract and reach only the
//!   translated seam ([`translated`]) today; the pre-decoded cache itself is
//!   the speed ladder's.
//! - **A translator.** [`translated`] is the seam one would plug into and
//!   nothing is plugged into it: with no core installed the hart runs the loop
//!   it has always run, and everything observable is byte-identical.
//! - **ICOUNT** counting, `LITBASE`-relative `l32r`, and any cache or
//!   region-protection *behaviour* (the TLB attributes are accepted and
//!   remembered, RM §4.6.3.2).
//!
//! External registers are **no longer** in that list: `rer`/`wer` run, against
//! the sparse store in [`extreg`] where an unwritten address reads 0. That
//! zero is the honest reading of the one external register the firmware
//! touches — `XDM_OCD_DCR_SET`, whose bit 0 asks "is a debugger attached?" —
//! and the store, not a model of the debug module, is what this hart claims.

pub mod breakpoint;
mod exec;
pub mod extreg;
pub mod interrupt;
pub mod mac16;
pub mod sr;
pub mod timer;
pub mod translated;
pub mod trap;
pub mod window;

use core::marker::PhantomData;

use lp_emu_core::{Bus, CycleModel, InstClass, MemoryAccessKind, MemoryError};
use lp_xt_inst::{AluRs, DecodeError, Inst, NullaryNarrowOp, NullaryOp};

use crate::cpu::Cpu;
use crate::emu::Flow;
use crate::error::{TRAP_CAUSE_WATCHPOINT, Trap, TrapKind};
use crate::executor::Exec;
use crate::fp_policy::FpPolicy;
use crate::trace::{NoopTracer, TraceEvent, Tracer};
use breakpoint::BreakUnit;
use extreg::ExternalRegs;
use interrupt::{IntLine, InterruptUnit, Take};
use mac16::Mac16;
use sr::{
    PS_CALLINC_MASK, PS_CALLINC_SHIFT, PS_EXCM, PS_INTLEVEL_MASK, PS_OWB_MASK, PS_OWB_SHIFT,
    PS_RESET, PS_UM, PS_WOE, PS_WRITE_MASK, SrFile,
};
use timer::Timers;
use trap::{
    DEBUGLEVEL, NMI_LEVEL, NUM_INTERRUPTS, NUM_TIMERS, VECOFS_DOUBLE, VECOFS_KERNEL, VECOFS_USER,
    cause, debugcause, is_vector_entry, level_vecofs, overflow_vecofs, underflow_vecofs,
};
use window::{WindowEvent, WindowPolicy};

use exec::Priv;

/// Why a slice stopped. The five variants RV32's `SliceEnd` has, by design.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SliceEnd {
    /// The cycle budget ran out. `pc` points at the next instruction.
    BudgetExhausted,
    /// `waiti` retired and no takeable interrupt was pending. `pc` already
    /// points *past* the `waiti`, so the machine can jump guest time to the
    /// next scheduler event and poll — the deterministic idle skip. The
    /// variant keeps RV32's name on purpose.
    Wfi,
    /// `break` or `break.n` at `pc`, which has **not** been advanced.
    ///
    /// The machine gets first refusal, because a ROM hook table claims
    /// certain PCs. A machine that does not claim `pc` must call
    /// [`XtHart::deliver_breakpoint`] to give the guest the architectural
    /// answer.
    Ebreak { pc: u32 },
    /// A peripheral asked the machine to take over before the next
    /// instruction ([`Bus::take_yield`]). `pc` already points at the next
    /// instruction, so the machine acts and resumes.
    BusYield,
    /// The hart cannot continue.
    Fault(HartFault),
}

/// A condition the hart has no architectural answer for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HartFault {
    /// A trap was raised and fetching the handler faulted too — a double
    /// fault. Real silicon would keep looping; stopping says so instead.
    TrapVectorFetch { vector: u32 },
    /// The executors returned an error with no privileged-architecture
    /// counterpart. Reaching this is a bug in this hart or in the executors,
    /// not in the guest.
    UnmappedExecutorError { pc: u32 },
    /// **Only with [`XtHart::set_strict_unsupported`]`(true)`.** The word at
    /// `pc` is one this emulator does not implement — today that is an
    /// encoding the decoder refuses, and nothing else: the hart's own
    /// declined set (`exec::is_unimplemented`) is empty now that `rer`/`wer`
    /// have a model.
    /// The architectural answer is an illegal-instruction exception, and
    /// that is what the default gives; the strict stop exists for bring-up,
    /// where a guest that reaches its own illegal-instruction handler is a
    /// silence and a named stop with the word is the honest test (ruling
    /// DD16). This variant is the one addition to RV32's `HartFault`; noted
    /// for M5's ADR.
    UnsupportedInstruction { pc: u32, word: u32, len: u8 },
}

/// What one instruction did to the slice loop.
enum StepOutcome {
    Continue,
    End(SliceEnd),
}

/// The core-configuration a machine builds a hart from: the chip's numbers,
/// none of which belong inside the hart as constants.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CoreConfig {
    /// `_ResetVector`: where the hart starts (`XCHAL_RESET_VECTOR_VADDR`).
    pub reset_pc: u32,
    /// `VECBASE` at reset (`XCHAL_VECBASE_RESET_VADDR`).
    pub reset_vecbase: u32,
    /// What `rsr.prid` returns. On the classic the two cores answer
    /// `0xCDCD` and `0xABAB` and esp-hal tells them apart by bit 13
    /// (`esp-hal-1.1.1/src/system.rs:302-311`) — so this is not a hart index.
    pub prid: u32,
    /// The 32 CPU interrupt lines: each one's fixed level and type.
    pub interrupts: [IntLine; NUM_INTERRUPTS],
}

/// One Xtensa hart in machine mode.
///
/// Generic over the bus rather than boxing it: the slice loop monomorphizes,
/// so a `Bus` whose `take_sideband` is the trait default costs nothing per
/// store. `Clone` and `Debug` are written out rather than derived: a derive
/// would add a `B: Clone` / `B: Debug` bound for the sake of a
/// `PhantomData<fn(&mut B)>`, which needs neither, and a machine could then
/// not snapshot a hart whose bus is not itself cloneable. Snapshotting the
/// hart is exactly the case that matters — the clone *is* the architectural
/// state.
pub struct XtHart<B: Bus> {
    /// The AR file, `WindowBase`/`WindowStart`, `SAR`, `PS.CALLINC`, the FP
    /// and Boolean files, `CPENABLE`, and `pc`. `call_stack` stays empty:
    /// it is the user-mode shadow (see [`WindowPolicy`]).
    cpu: Cpu,
    /// Every PS field except `CALLINC`, which lives in `cpu.ps_callinc`
    /// because the shared executors write it at every windowed call. One
    /// source of truth per field; [`XtHart::ps`] composes the register.
    ps: u32,
    sr: SrFile,
    /// What `rer`/`wer` reach: a sparse store, 0 everywhere nothing has been
    /// written. Architectural state in the sense that matters here — it is
    /// cloned with the hart and survives a snapshot — even though the space
    /// it stands for is mostly outside the core. See [`extreg`].
    ext: ExternalRegs,
    ints: InterruptUnit,
    timers: Timers,
    breaks: BreakUnit,
    mac: Mac16,
    /// The FP behaviour the executors need by signature. Machine mode runs
    /// the same executors and therefore the same policy.
    fp_policy: FpPolicy,
    instruction_count: u64,
    cycle_count: u64,
    cycle_model: CycleModel,
    hart_id: u32,
    prid: u32,
    /// Set by `waiti`, cleared by any takeable interrupt.
    waiti: bool,
    /// The `break` the last [`SliceEnd::Ebreak`] reported, so
    /// [`XtHart::deliver_breakpoint`] knows its width and form.
    pending_break: Option<PendingBreak>,
    strict_unsupported: bool,
    /// How many `isync` instructions the guest has retired — the Xtensa
    /// analogue of RV32's `fence_i_count`, and the same diagnostic: a run that
    /// published code and shows a zero here has a fence that is not reaching
    /// the machine.
    isync_count: u64,
    /// The installed translated core, if any, and the entry table that says
    /// where it may be entered (direct-mapped **by byte**, `0` = no entry).
    ///
    /// **Not architectural state**: absent from a snapshot, empty on a clone,
    /// and a run with no core installed must produce a byte-identical
    /// everything. See [`translated`].
    core: Option<translated::BoxedCore<B>>,
    core_entries: Vec<u32>,
    /// An invalidation asked for while the core was lifted out of the hart —
    /// see [`XtHart::drain_core_flush`]. `isync` and a `wsr` to `LEND` are why
    /// it exists.
    core_flush_pending: PendingInvalidate,
    _bus: PhantomData<fn(&mut B)>,
}

/// An invalidation held for a translated core that is not currently in the
/// hart.
///
/// Two ranges cannot be merged without losing precision in the *unsafe*
/// direction, so a second range widens the whole thing to
/// [`PendingInvalidate::All`]. Invalidating too much is slow; invalidating too
/// little is wrong.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PendingInvalidate {
    None,
    Range(u32, u32),
    All,
}

impl PendingInvalidate {
    fn add(self, range: Option<(u32, u32)>) -> Self {
        match (self, range) {
            (Self::All, _) | (_, None) => Self::All,
            (Self::None, Some((lo, hi))) => Self::Range(lo, hi),
            (Self::Range(..), Some(_)) => Self::All,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct PendingBreak {
    pc: u32,
    len: u8,
    narrow: bool,
}

impl<B: Bus> Clone for XtHart<B> {
    fn clone(&self) -> Self {
        Self {
            cpu: self.cpu.clone(),
            ps: self.ps,
            sr: self.sr.clone(),
            ext: self.ext.clone(),
            ints: self.ints.clone(),
            timers: self.timers.clone(),
            breaks: self.breaks.clone(),
            mac: self.mac.clone(),
            fp_policy: self.fp_policy.clone(),
            instruction_count: self.instruction_count,
            cycle_count: self.cycle_count,
            cycle_model: self.cycle_model,
            hart_id: self.hart_id,
            prid: self.prid,
            waiti: self.waiti,
            pending_break: self.pending_break,
            strict_unsupported: self.strict_unsupported,
            isync_count: self.isync_count,
            // Not copied: a translated core is not architectural state, and
            // it holds host code for guest bytes that the clone's bus may not
            // have. The snapshot path is exactly the case that matters.
            core: None,
            core_entries: Vec::new(),
            core_flush_pending: PendingInvalidate::None,
            _bus: PhantomData,
        }
    }
}

impl<B: Bus> core::fmt::Debug for XtHart<B> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("XtHart")
            .field("hart_id", &self.hart_id)
            .field("pc", &format_args!("{:#010x}", self.cpu.pc))
            .field("ps", &format_args!("{:#010x}", self.ps()))
            .field("window_base", &self.cpu.window_base)
            .field(
                "window_start",
                &format_args!("{:#06x}", self.cpu.window_start),
            )
            .field("sr", &self.sr)
            .field("ints", &self.ints)
            .field("timers", &self.timers)
            .field("breaks", &self.breaks)
            .field("instruction_count", &self.instruction_count)
            .field("cycle_count", &self.cycle_count)
            .field("cycle_model", &self.cycle_model)
            .field("waiti", &self.waiti)
            .finish()
    }
}

impl<B: Bus> XtHart<B> {
    /// A hart at reset: zeroed registers, `WindowBase = 0`, `WindowStart =
    /// 1` (frame 0 resident), `PS = 0x1F` (see the module docs), `pc` and
    /// `VECBASE` from `config`, `CPENABLE = 0`, `IBREAKENABLE = 0`, every
    /// undefined-at-reset register 0.
    #[must_use]
    pub fn new(hart_id: u32, config: CoreConfig) -> Self {
        let mut cpu = Cpu::new();
        cpu.pc = config.reset_pc;
        cpu.window_start = 1;
        Self {
            cpu,
            ps: PS_RESET,
            sr: SrFile::new(config.reset_vecbase),
            ext: ExternalRegs::new(),
            ints: InterruptUnit::new(config.interrupts),
            timers: Timers::new(),
            breaks: BreakUnit::new(),
            mac: Mac16::new(),
            fp_policy: FpPolicy::m6(),
            instruction_count: 0,
            cycle_count: 0,
            // No measured Xtensa cycle model exists; a cycle is an
            // instruction, as in the user-mode runner.
            cycle_model: CycleModel::InstructionCount,
            hart_id,
            prid: config.prid,
            waiti: false,
            pending_break: None,
            strict_unsupported: false,
            isync_count: 0,
            core: None,
            core_entries: Vec::new(),
            core_flush_pending: PendingInvalidate::None,
            _bus: PhantomData,
        }
    }

    // --- the translated core ----------------------------------------------

    /// Install a translated core and the guest pcs at which it may be
    /// entered, replacing any core already installed.
    ///
    /// See [`translated`] for what a core is and what it promises. Installing
    /// one changes no architectural state and no transcript; it is the
    /// mechanism, not the decision.
    pub fn set_translated_core(&mut self, core: translated::BoxedCore<B>, entries: &[u32]) {
        self.core_entries = translated::entry_table(entries);
        self.core = Some(core);
        // A freshly installed core knows exactly what it holds, so anything
        // recorded for the core it replaces is not its business.
        self.core_flush_pending = PendingInvalidate::None;
    }

    /// Remove the installed core, if any, and go back to interpreting.
    ///
    /// This is what `--interpreter` reaches: every build can turn the
    /// translator off entirely and must then produce an identical everything.
    pub fn clear_translated_core(&mut self) {
        self.core = None;
        self.core_entries = Vec::new();
        self.core_flush_pending = PendingInvalidate::None;
    }

    /// Is a translated core installed?
    #[inline]
    #[must_use]
    pub fn has_translated_core(&self) -> bool {
        self.core.is_some()
    }

    /// The installed core's own report line, if any.
    #[must_use]
    pub fn translated_core_report(&self) -> Option<String> {
        self.core.as_ref().map(|c| c.report())
    }

    // --- plain accessors ---------------------------------------------------

    #[inline]
    #[must_use]
    pub const fn hart_id(&self) -> u32 {
        self.hart_id
    }

    #[inline]
    #[must_use]
    pub const fn pc(&self) -> u32 {
        self.cpu.pc
    }

    #[inline]
    pub fn set_pc(&mut self, pc: u32) {
        self.cpu.pc = pc;
    }

    /// The register state the executors see: AR file, window, SAR, FP/BR
    /// files, CPENABLE.
    #[inline]
    #[must_use]
    pub const fn cpu(&self) -> &Cpu {
        &self.cpu
    }

    #[inline]
    pub fn cpu_mut(&mut self) -> &mut Cpu {
        &mut self.cpu
    }

    /// The composed `PS` register.
    #[inline]
    #[must_use]
    pub fn ps(&self) -> u32 {
        self.ps | (u32::from(self.cpu.ps_callinc & 3) << PS_CALLINC_SHIFT)
    }

    /// Write `PS` from *outside* the guest — the reset-vector seeding path
    /// ([`sr::PS_BOOT`] for a direct load) and state restore. The RM's write
    /// mask applies; nothing is polled as a side effect. The twin of RV32's
    /// `set_csr_raw`.
    pub fn set_ps_raw(&mut self, value: u32) {
        self.ps = value & PS_WRITE_MASK & !PS_CALLINC_MASK;
        self.cpu.ps_callinc = ((value & PS_CALLINC_MASK) >> PS_CALLINC_SHIFT) as u8;
    }

    /// Read-only view of the SR file, for state dumps and snapshotting.
    #[inline]
    #[must_use]
    pub const fn sr(&self) -> &SrFile {
        &self.sr
    }

    /// Write an SR from outside the guest — state restore. No side effects:
    /// the bus is not told about a DBREAK and nothing is polled.
    #[inline]
    pub fn sr_mut(&mut self) -> &mut SrFile {
        &mut self.sr
    }

    #[inline]
    #[must_use]
    pub const fn interrupts(&self) -> &InterruptUnit {
        &self.ints
    }

    /// The interrupt unit from outside the guest — state restore, and a
    /// machine seeding `INTENABLE`. Nothing is polled as a side effect.
    #[inline]
    pub fn interrupts_mut(&mut self) -> &mut InterruptUnit {
        &mut self.ints
    }

    #[inline]
    #[must_use]
    pub const fn timers(&self) -> &Timers {
        &self.timers
    }

    #[inline]
    #[must_use]
    pub const fn breakpoints(&self) -> &BreakUnit {
        &self.breaks
    }

    #[inline]
    #[must_use]
    pub const fn mac16(&self) -> &Mac16 {
        &self.mac
    }

    #[inline]
    #[must_use]
    pub const fn cycle_count(&self) -> u64 {
        self.cycle_count
    }

    #[inline]
    #[must_use]
    pub const fn instruction_count(&self) -> u64 {
        self.instruction_count
    }

    #[inline]
    #[must_use]
    pub const fn cycle_model(&self) -> CycleModel {
        self.cycle_model
    }

    #[inline]
    pub fn set_cycle_model(&mut self, model: CycleModel) {
        self.cycle_model = model;
    }

    /// Replace the FP policy (the machine hands the hart the constants P6
    /// measured, exactly as `Emulator::with_fp_policy` does).
    pub fn set_fp_policy(&mut self, policy: FpPolicy) {
        self.fp_policy = policy;
    }

    /// True while the hart is parked in `waiti`.
    #[inline]
    #[must_use]
    pub const fn is_waiti(&self) -> bool {
        self.waiti
    }

    /// Turn the bring-up stop on: an unimplemented instruction ends the
    /// slice with [`HartFault::UnsupportedInstruction`] instead of taking the
    /// architectural illegal-instruction exception. Default off.
    pub fn set_strict_unsupported(&mut self, on: bool) {
        self.strict_unsupported = on;
    }

    #[inline]
    #[must_use]
    pub const fn strict_unsupported(&self) -> bool {
        self.strict_unsupported
    }

    /// The external-register space `rer`/`wer` reach ([`extreg`]).
    ///
    /// A machine that has to answer for one of these addresses — a SoC that
    /// hangs something real off the ERI window — seeds it here rather than
    /// teaching the hart the chip.
    #[inline]
    #[must_use]
    pub const fn external_regs(&self) -> &ExternalRegs {
        &self.ext
    }

    /// Mutable access to the external-register space, for the same reason.
    #[inline]
    pub const fn external_regs_mut(&mut self) -> &mut ExternalRegs {
        &mut self.ext
    }

    /// Forget whatever has been pre-decoded or translated for `[lo, hi)`.
    ///
    /// There is no pre-decoded block cache on this hart yet — that is the
    /// speed ladder's — so this reaches only the translated core. A caller
    /// that writes guest code from the host side calls it; so does the hart
    /// itself, from the three invalidation events [`translated`] documents.
    #[inline]
    pub fn invalidate_block_range(&mut self, lo: u32, hi: u32) {
        match self.core.as_mut() {
            Some(core) => core.invalidate(Some((lo, hi))),
            None => self.core_flush_pending = self.core_flush_pending.add(Some((lo, hi))),
        }
    }

    /// Forget everything pre-decoded or translated — the whole-image form of
    /// [`XtHart::invalidate_block_range`].
    ///
    /// `None` reaching a core means "everything". A hart with no core
    /// installed records the request instead, because the core the hart is
    /// *about to be given back* (it is lifted out for the length of a slice)
    /// is the one that has to hear it.
    #[inline]
    pub fn invalidate_blocks(&mut self) {
        match self.core.as_mut() {
            Some(core) => core.invalidate(None),
            None => self.core_flush_pending = self.core_flush_pending.add(None),
        }
    }

    /// Apply an invalidation that was asked for while the translated core was
    /// lifted out of the hart.
    ///
    /// The escape hatch is the reason this exists: translated code called
    /// [`XtHart::step_one`], the instruction it ran was an `isync` or a `wsr`
    /// to `LEND`, and the core that has to forget what it translated was in
    /// the caller's hand at the time.
    fn drain_core_flush(&mut self, core: &mut translated::BoxedCore<B>) {
        match core::mem::replace(&mut self.core_flush_pending, PendingInvalidate::None) {
            PendingInvalidate::None => {}
            PendingInvalidate::Range(lo, hi) => core.invalidate(Some((lo, hi))),
            PendingInvalidate::All => core.invalidate(None),
        }
    }

    /// The `isync` hook: the guest has published instructions, so anything
    /// pre-decoded or translated for them is suspect.
    ///
    /// A whole-image invalidation rather than a range, exactly as RV32 does
    /// for `fence.i`: it fires a handful of times in a run, so range precision
    /// would buy nothing measurable and would cost correctness surface.
    #[inline]
    pub(crate) fn on_isync(&mut self) {
        self.isync_count += 1;
        self.invalidate_blocks();
    }

    /// How many `isync` instructions the guest has retired.
    #[inline]
    #[must_use]
    pub const fn isync_count(&self) -> u64 {
        self.isync_count
    }

    /// The earliest cycle at which a `CCOMPARE` timer matches, so a machine
    /// can end a slice or an idle skip there rather than overshoot.
    #[inline]
    #[must_use]
    pub fn next_timer_cycle(&self) -> Option<u64> {
        self.timers.next_match()
    }

    /// Jump guest time forward to `cycle` — the deterministic idle skip.
    /// Never moves backwards. A timer whose match lies inside the skip raises
    /// its line; the machine polls (point (d)) afterwards.
    pub fn advance_to_cycle(&mut self, cycle: u64) {
        if cycle > self.cycle_count {
            self.cycle_count = cycle;
            // Latched, not delivered: the skip is the machine's, and so is
            // the poll that follows it (point (d)). Delivering here would
            // move a trap out of the slice loop and into a clock write.
            self.latch_timers();
        }
    }

    // --- interrupt input ---------------------------------------------------

    /// Tell the hart which CPU interrupt lines the SoC asserts right now, as
    /// a bitmask over lines 0..32.
    ///
    /// Level lines follow the mask; edge lines and the NMI latch on its
    /// rising edges. Setting it delivers nothing; the machine polls. This is
    /// the seam ruling R5 names: M2's matrix produces exactly this mask, and
    /// [`Bus::pending_cpu_interrupt`] is read at poll point (c) as a
    /// one-line degenerate of it until M2 lands.
    ///
    /// M2 P2 landed the honest feed:
    /// `lp_emu_esp_common::SocBus::pending_cpu_interrupt_mask()` returns this
    /// mask directly, from `CpuIntMatrix::asserted`. The approximation at
    /// poll point (c) is retired when M3's Xtensa machine binds a hart to a
    /// `SocBus` and calls it; nothing here changes until then.
    #[inline]
    pub fn set_external_mask(&mut self, mask: u32) {
        self.ints.set_external(mask);
    }

    #[inline]
    #[must_use]
    pub const fn external_mask(&self) -> u32 {
        self.ints.external()
    }

    /// Examine pending interrupt state; deliver one if it is takeable.
    /// Returns `true` when an interrupt was taken.
    ///
    /// The RV32 hart separates *wake* (a pending enabled interrupt un-parks
    /// `wfi` regardless of `mstatus.MIE`) from *deliver* (needs `MIE`).
    /// Xtensa has no global enable: `PS.INTLEVEL` is both the mask and the
    /// thing `waiti` sets, so the analogue is one condition — a pending,
    /// `INTENABLE`d line at a level above `CINTLEVEL` un-parks the hart
    /// **and** is delivered, in the same call (RM §4.4.4.3: "after
    /// executing the interrupt handler, execution continues with the
    /// instruction following the WAITI").
    pub fn poll_interrupts(&mut self) -> bool {
        let Some(take) = self.ints.select(self.ps()) else {
            return false;
        };
        self.waiti = false;
        self.take_interrupt(take);
        true
    }

    /// Give the guest the architectural answer to a `break` the machine
    /// chose not to claim: the debug exception at `DEBUGLEVEL` with
    /// `DEBUGCAUSE.BI` (or `.BN` for `break.n`) and `EPC6` = the `break`
    /// itself — or, when `CINTLEVEL >= DEBUGLEVEL`, the RM's no-op: `pc`
    /// steps past it.
    pub fn deliver_breakpoint(&mut self, pc: u32) {
        let (len, narrow) = match self.pending_break.take() {
            Some(b) if b.pc == pc => (b.len, b.narrow),
            _ => (3, false),
        };
        // The retire the slice loop skipped when it handed the break out.
        self.instruction_count += 1;
        self.charge(InstClass::System);
        self.cpu.pc = pc;
        if InterruptUnit::cintlevel(self.ps()) < DEBUGLEVEL {
            let bits = if narrow {
                debugcause::BREAK_N
            } else {
                debugcause::BREAK
            };
            self.enter_debug(bits);
        } else {
            self.cpu.pc = pc.wrapping_add(u32::from(len));
        }
    }

    // --- the slice loop ----------------------------------------------------

    /// Run instructions until the cycle `budget` is spent or something ends
    /// the slice.
    ///
    /// `budget` is in **cycles** and is honoured to within one instruction's
    /// cost: the check happens before each fetch, so the last instruction of
    /// a slice may overshoot by its own cost and no more. Cycles consumed are
    /// `cycle_count()` differenced across the call.
    pub fn run_slice(&mut self, bus: &mut B, budget: u64) -> SliceEnd {
        self.run_slice_with(bus, budget, &mut NoopTracer)
    }

    /// As [`run_slice`](Self::run_slice), emitting [`TraceEvent`]s — the
    /// fixtures' goldens are traces. Dispatched once per slice so both
    /// tracer instantiations are codegen'd inside this crate (the opt-level
    /// containment rule `Emulator::run_loop` follows).
    pub fn run_slice_traced(
        &mut self,
        bus: &mut B,
        budget: u64,
        tracer: &mut dyn Tracer,
    ) -> SliceEnd {
        if tracer.discards_events() {
            self.run_slice_with(bus, budget, &mut NoopTracer)
        } else {
            self.run_slice_with(bus, budget, tracer)
        }
    }

    fn run_slice_with<T: Tracer + ?Sized>(
        &mut self,
        bus: &mut B,
        budget: u64,
        tracer: &mut T,
    ) -> SliceEnd {
        // (a) poll on entry.
        self.poll_interrupts();
        // The deadline as an absolute cycle; `saturating_add` keeps a
        // `u64::MAX` budget meaning "never".
        let end = self.cycle_count.saturating_add(budget);
        // One branch per slice, not per instruction: a hart with no
        // translated core runs the loop it has always run, and the seam costs
        // it nothing. See [`translated`].
        if self.core.is_some() {
            self.run_slice_cored(bus, end, tracer)
        } else {
            self.run_slice_stepping(bus, end, tracer)
        }
    }

    /// The interpreter's slice loop, unchanged and unconditional.
    #[inline]
    fn run_slice_stepping<T: Tracer + ?Sized>(
        &mut self,
        bus: &mut B,
        end: u64,
        tracer: &mut T,
    ) -> SliceEnd {
        loop {
            if self.cycle_count >= end {
                return SliceEnd::BudgetExhausted;
            }
            if let Some(over) = self.step_once(bus, tracer) {
                return over;
            }
        }
    }

    /// The same slice with a translated core installed.
    ///
    /// The core is **lifted out of `self`** for the whole slice, so the core
    /// and the hart's registers are two disjoint borrows rather than one and a
    /// core can be handed the hart it lives on (see [`translated`]'s module
    /// docs). It goes back on every exit path. An invalidation asked for while
    /// it was out is drained into it on the way in and on the way out.
    fn run_slice_cored<T: Tracer + ?Sized>(
        &mut self,
        bus: &mut B,
        end: u64,
        tracer: &mut T,
    ) -> SliceEnd {
        let mut core = self.core.take();
        if let Some(core) = core.as_mut() {
            self.drain_core_flush(core);
        }
        let out = self.run_entries(core.as_mut(), bus, end, tracer);
        if let Some(core) = core.as_mut() {
            self.drain_core_flush(core);
        }
        self.core = core;
        out
    }

    /// The slice loop with the translated-core entry check at its top.
    ///
    /// Structurally the RV32 hart's `run_blocks` with the block-cache half
    /// removed: there is no pre-decoded cache on this hart, so a pc the core
    /// does not claim falls through to exactly the `step_once` the
    /// single-stepping loop runs.
    fn run_entries<T: Tracer + ?Sized>(
        &mut self,
        mut core: Option<&mut translated::BoxedCore<B>>,
        bus: &mut B,
        end: u64,
        tracer: &mut T,
    ) -> SliceEnd {
        loop {
            if self.cycle_count >= end {
                return SliceEnd::BudgetExhausted;
            }
            let pc = self.cpu.pc;
            // The entry check: one table read and one compare, ahead of
            // anything else. Byte-indexed — see `translated::entry_slot`.
            if let Some(core) = core.as_mut()
                && self.core_entries[translated::entry_slot(pc)] == pc
            {
                // Read before the call: the escape hatch runs guest
                // instructions through the hart itself, so `self` may not
                // still hold the entry counter by the time the core returns.
                let entry_instret = self.instruction_count;
                let outcome = core.run(self, bus, end);
                // The escape hatch can have retired an `isync` or a `wsr` to
                // `LEND`, and the core it wanted invalidated was out here
                // rather than in the hart. Drain before it is consulted again.
                self.drain_core_flush(core);
                // The escape hatch ran something that ended the slice — a
                // `waiti`, a `break`, a bus yield, a fault. The interpreter's
                // own answer, handed straight back: a translated stay does not
                // get to swallow one, and the counters are applied first so
                // the machine resumes exactly where it would have.
                if let translated::RunOutcome::Ended {
                    pc: new_pc,
                    cycle_count,
                    instruction_count,
                    end: over,
                } = outcome
                {
                    self.cpu.pc = new_pc;
                    self.cycle_count = cycle_count;
                    self.instruction_count = instruction_count;
                    return over;
                }
                let mut progressed = false;
                if let translated::RunOutcome::Ran {
                    pc: new_pc,
                    cycle_count,
                    instruction_count,
                    after_store,
                } = outcome
                    // The no-progress guard. A core whose first block does not
                    // fit the remaining budget returns `Ran` having done
                    // nothing; without this the loop would ask it again
                    // forever. The interpreter runs the instruction instead,
                    // which is what happens when the budget is short.
                    && (new_pc != pc || instruction_count != entry_instret)
                {
                    self.cpu.pc = new_pc;
                    self.cycle_count = cycle_count;
                    self.instruction_count = instruction_count;
                    if after_store {
                        // Polling point (c), byte for byte what `step` does
                        // after an interpreted store.
                        if bus.take_sideband() {
                            self.resample_external(bus);
                        }
                        if bus.take_yield() {
                            return SliceEnd::BusYield;
                        }
                    }
                    // (e) a `CCOMPARE` match inside the stay. The interpreter
                    // tests this after every retire and a core cannot, so the
                    // hart latches and polls once the stay is over; a core
                    // that wants the interrupt delivered on the exact
                    // instruction leaves at `next_timer_cycle` or refuses.
                    self.tick_timers();
                    progressed = true;
                }
                if progressed {
                    continue;
                }
            }
            if let Some(over) = self.step_once(bus, tracer) {
                return over;
            }
        }
    }

    /// Run **exactly one** guest instruction at [`pc`](Self::pc), the way the
    /// interpreter always has, and report whether the slice is over.
    ///
    /// This is the escape hatch: the one thing a translated core calls when it
    /// meets an instruction it does not translate. It is the same `step_once`
    /// the single-stepping loop runs — not a second copy of it — so the trap,
    /// special-register, `waiti`, `break`, `isync` and polling-point behaviour
    /// a translated stay inherits is the interpreter's own by construction and
    /// cannot drift from it.
    ///
    /// Afterwards [`pc`](Self::pc), [`cycle_count`](Self::cycle_count),
    /// [`instruction_count`](Self::instruction_count) and the AR file are what
    /// the interpreter would have left. `None` means the caller may continue;
    /// `Some` is a slice end the caller must report back, because a translated
    /// core does not get to swallow a `waiti`, a `break` or a bus yield.
    ///
    /// No trace is emitted: see [`translated::TranslatedCore::run`].
    pub fn step_one(&mut self, bus: &mut B) -> Option<SliceEnd> {
        self.step_once(bus, &mut NoopTracer)
    }

    /// Set the cycle and instruction counters from outside the guest.
    ///
    /// A translated core keeps both in host registers for the length of a stay
    /// and has to hand them back before anything the guest can observe reads
    /// them — the escape hatch above, and every exit. Neither counter is polled
    /// as a side effect, so this is a plain write; the *monotonicity*
    /// [`advance_to_cycle`](Self::advance_to_cycle) protects is the scheduler's
    /// idle skip, which is a different job. Note that `CCOUNT` is
    /// [`cycle_count`](Self::cycle_count) seen through an offset, so moving it
    /// moves guest time.
    #[inline]
    pub fn set_counters(&mut self, cycle_count: u64, instruction_count: u64) {
        self.cycle_count = cycle_count;
        self.instruction_count = instruction_count;
    }

    /// One instruction: fetch it, decode it, run it, charge what the bus
    /// billed. `None` means "keep going"; `Some` ends the slice.
    #[inline(always)]
    fn step_once<T: Tracer + ?Sized>(&mut self, bus: &mut B, tracer: &mut T) -> Option<SliceEnd> {
        let pc = self.cpu.pc;
        bus.set_issuing(pc, self.cycle_count);

        // IBREAK: "when the processor is about to complete the execution of
        // the instruction fetched from IBREAKA[i] ... it raises an exception
        // instead" (RM §4.7.6.3). Masked at and above DEBUGLEVEL.
        if self.breaks.ibreakenable != 0
            && let Some(_slot) = self.breaks.ibreak_hit(pc)
            && InterruptUnit::cintlevel(self.ps()) < DEBUGLEVEL
        {
            self.enter_debug(debugcause::IBREAK);
            return None;
        }

        let mut bytes = [0u8; 3];
        let got = match bus.fetch_bytes(pc, &mut bytes) {
            Ok(n) => n,
            Err(e) => {
                // A fetch that faulted still went to memory, so whatever the
                // bus charged for it is charged here.
                self.charge_memory(bus);
                return match self.deliver_fetch_error(e, pc) {
                    Ok(()) => None,
                    Err(fault) => Some(SliceEnd::Fault(fault)),
                };
            }
        };
        let (inst, len) = match lp_xt_inst::decode(&bytes[..got]) {
            Ok(x) => x,
            Err(DecodeError::Truncated { got, .. }) => {
                // The instruction runs off the end of the mapping: the byte
                // that could not be fetched is the faulting address.
                let at = pc.wrapping_add(got as u32);
                self.charge_memory(bus);
                let e = MemoryError::InvalidAccess {
                    address: at,
                    size: 1,
                    kind: MemoryAccessKind::InstructionFetch,
                };
                return match self.deliver_fetch_error(e, pc) {
                    Ok(()) => None,
                    Err(fault) => Some(SliceEnd::Fault(fault)),
                };
            }
            Err(DecodeError::Unsupported { word, len }) => {
                self.charge(InstClass::System);
                self.charge_memory(bus);
                return self.deliver_unsupported(
                    pc,
                    word,
                    len as u8,
                    "the decoder refuses the word",
                );
            }
        };
        tracer.event(TraceEvent::Inst {
            pc,
            len,
            inst: &inst,
        });

        let outcome = self.step(bus, pc, &inst, len as u32, tracer);
        // Drained after the instruction, so the fetch and any load or store
        // it made are charged together, once.
        self.charge_memory(bus);
        match outcome {
            StepOutcome::Continue => None,
            StepOutcome::End(end) => Some(end),
        }
    }

    /// One decoded instruction: the window check, the loop-back, the hart's
    /// own families or the shared executors, then the retire.
    fn step<T: Tracer + ?Sized>(
        &mut self,
        bus: &mut B,
        pc: u32,
        inst: &Inst,
        len: u32,
        tracer: &mut T,
    ) -> StepOutcome {
        // --- the window machinery, before the instruction (RM §4.7.1.3) ---
        let woe = self.ps & PS_WOE != 0;
        let excm = self.ps & PS_EXCM != 0;
        let mut owb = 0u8;
        let event = if woe && !excm {
            let group = window::ar_group(inst, self.cpu.ps_callinc);
            window::overflow_check(&mut self.cpu, group, &mut owb)
        } else {
            None
        };
        let event = event.or_else(|| match *inst {
            Inst::Entry(rs, _) => window::entry_check(rs.num(), woe, excm),
            Inst::Nullary(NullaryOp::Retw) | Inst::NullaryN(NullaryNarrowOp::RetwN) => {
                window::retw_check(&mut self.cpu, woe, excm, &mut owb)
            }
            Inst::Rs(AluRs::Movsp, ..) => window::movsp_check(&self.cpu),
            _ => None,
        });
        if let Some(event) = event {
            self.charge(InstClass::System);
            self.window_exception(event, owb, pc);
            return StepOutcome::Continue;
        }

        // --- the loop-back (RM §3.5.4.1, §4.3.2.4) ---
        //
        // Computed on the *sequential* next pc, before the instruction runs,
        // and only while PS.EXCM is clear: a taken branch to LEND does not
        // loop back (it overrides `next` below), and a jump to LBEG happens
        // only from the instruction whose successor is LEND. The decrement
        // is undone if the instruction aborts with an exception — an aborted
        // instruction has no side effects, and the RM's model decrements
        // before `Inst()` only because it never aborts there.
        let seq = pc.wrapping_add(len);
        let mut next = seq;
        let looped = self.sr.lcount != 0 && !excm && seq == self.sr.lend;
        if looped {
            self.sr.lcount = self.sr.lcount.wrapping_sub(1);
            next = self.sr.lbeg;
        }

        // --- execute ---
        let executed = if exec::is_hart_owned(inst) {
            self.exec_priv(bus, inst, pc, len, tracer)
        } else {
            let mut view = Exec {
                cpu: &mut self.cpu,
                mem: bus,
                fp_policy: &mut self.fp_policy,
                window: WindowPolicy::Exception,
            };
            view.execute_classed(inst, pc, tracer)
                .map(|(flow, class)| Priv::Retire {
                    flow,
                    class,
                    poll: false,
                })
        };

        let (flow, class, poll) = match executed {
            Ok(Priv::Retire { flow, class, poll }) => (flow, class, poll),
            Ok(Priv::Waiti) => {
                self.instruction_count += 1;
                self.charge(InstClass::System);
                self.cpu.pc = next;
                self.waiti = true;
                // (b) an already-pending interrupt un-parks immediately.
                self.poll_interrupts();
                return if self.waiti {
                    StepOutcome::End(SliceEnd::Wfi)
                } else {
                    StepOutcome::Continue
                };
            }
            Ok(Priv::Break { narrow }) => {
                // Deliberately not charged and `pc` not advanced: the machine
                // gets the instruction back untouched and may re-present it.
                if looped {
                    self.sr.lcount = self.sr.lcount.wrapping_add(1);
                }
                self.pending_break = Some(PendingBreak {
                    pc,
                    len: len as u8,
                    narrow,
                });
                return StepOutcome::End(SliceEnd::Ebreak { pc });
            }
            Err(trap) => {
                if looped {
                    self.sr.lcount = self.sr.lcount.wrapping_add(1);
                }
                self.charge(InstClass::System);
                return match self.deliver_trap(trap, pc, inst, len) {
                    Ok(()) => StepOutcome::Continue,
                    Err(fault) => StepOutcome::End(SliceEnd::Fault(fault)),
                };
            }
        };

        // --- retire ---
        self.instruction_count += 1;
        self.charge(class);
        self.cpu.pc = match flow {
            Flow::Next => next,
            Flow::Jump(target) => target,
            Flow::Syscall => unreachable!("the hart intercepts syscall before the executors"),
        };

        // (b) an instruction that can turn delivery on.
        if poll {
            self.poll_interrupts();
        }
        // (c) an MMIO store may have changed interrupt state, and it may have
        // changed something only the machine can act on.
        if matches!(
            class,
            InstClass::Store | InstClass::Atomic | InstClass::System
        ) {
            if bus.take_sideband() {
                self.resample_external(bus);
            }
            if bus.take_yield() {
                return StepOutcome::End(SliceEnd::BusYield);
            }
        }
        // (e) an internal timer reached its compare.
        self.tick_timers();
        StepOutcome::Continue
    }

    /// Answer a raised bus side-band: re-read the single line the bus can
    /// name today and poll. M2 replaces the `Option<u8>` with the bitmask.
    #[inline]
    fn resample_external(&mut self, bus: &B) {
        let mask = bus.pending_cpu_interrupt().map_or(0, |n| 1u32 << (n & 31));
        self.ints.set_external(mask);
        self.poll_interrupts();
    }

    /// Raise the line of every timer whose compare the cycle counter has
    /// reached. Returns whether any did.
    #[inline]
    fn latch_timers(&mut self) -> bool {
        let fired = self.timers.advance(self.cycle_count);
        if fired == 0 {
            return false;
        }
        for i in 0..NUM_TIMERS {
            if fired & (1 << i) != 0 {
                self.ints.timer_fired(i);
            }
        }
        true
    }

    /// Poll point (e): a timer match inside the slice loop is delivered in
    /// the same instruction.
    #[inline]
    fn tick_timers(&mut self) {
        if self.latch_timers() {
            self.poll_interrupts();
        }
    }

    #[inline]
    fn charge(&mut self, class: InstClass) {
        self.cycle_count += u64::from(self.cycle_model.cycles_for(class));
    }

    /// Charge whatever the bus's memory system billed for this instruction's
    /// accesses ([`Bus::take_memory_cost`]).
    #[inline]
    fn charge_memory(&mut self, bus: &mut B) {
        let extra = bus.take_memory_cost();
        if extra != 0 {
            self.cycle_count += u64::from(extra);
        }
    }

    // --- exception entry (RM §4.4.1.10, §4.4.5.4, §4.7.6.3) ---------------

    /// Enter a general exception handler.
    ///
    /// The sequence is fixed by what the guest's own vectors do on arrival
    /// (xtensa-lx-rt 0.22.0 `src/exception/asm.rs:830-861`): the first thing
    /// `_UserExceptionVector` does is `wsr a0, EXCSAVE1` and `rsr a0,
    /// EXCCAUSE`, so EXCCAUSE must already hold the cause and `a0` must be
    /// untouched. Per the RM's `Exception(cause)` procedure:
    ///
    /// - `EPC1 <- pc` of the faulting instruction (not the next one)
    /// - `EXCCAUSE <- cause`
    /// - `EXCVADDR <- the faulting address` (fetch/load/store/alignment
    ///   causes only; left alone otherwise, because the guest reads it only
    ///   for those)
    /// - `PS.EXCM <- 1` (masks interrupts up to `EXCM_LEVEL` = 3 and selects
    ///   the double-exception vector for a fault taken inside this one)
    /// - `PC <- VECBASE + (if PS.UM { VECOFS_USER } else { VECOFS_KERNEL })`
    ///
    /// `PS.INTLEVEL` is **not** raised by an exception — only `EXCM`
    /// changes. That asymmetry with the interrupt path is architectural.
    ///
    /// A fault while `PS.EXCM == 1` takes `VECOFS_DOUBLE` with `DEPC`
    /// instead of `EPC1`. A fault while fetching the vector itself is
    /// [`HartFault::TrapVectorFetch`] (see `deliver_fetch_error`).
    pub(crate) fn enter_exception(&mut self, cause: u32, vaddr: Option<u32>) {
        let pc = self.cpu.pc;
        let vector = if self.ps & PS_EXCM != 0 {
            self.sr.depc = pc;
            VECOFS_DOUBLE
        } else {
            self.sr.epc[1] = pc;
            if self.ps & PS_UM != 0 {
                VECOFS_USER
            } else {
                VECOFS_KERNEL
            }
        };
        self.sr.exccause = cause & sr::EXCCAUSE_MASK;
        if let Some(v) = vaddr {
            self.sr.excvaddr = v;
        }
        self.ps |= PS_EXCM;
        self.cpu.pc = self.sr.vecbase.wrapping_add(vector);
    }

    /// Enter a window overflow/underflow handler, or the alloca / illegal
    /// exception the window checks produced. `owb` is the WindowBase the
    /// check saved; the check has already rotated `WindowBase`.
    ///
    /// **PS.OWB is written here.** `_AllocAException` reads it back with
    /// `extui a3, a2, 8, 4`, xors in its own rotation and writes PS: a hart
    /// that did not maintain OWB would let that handler corrupt the window.
    fn window_exception(&mut self, event: WindowEvent, owb: u8, pc: u32) {
        let vector = match event {
            WindowEvent::Overflow { vector_inc } => overflow_vecofs(vector_inc),
            WindowEvent::Underflow { n } => underflow_vecofs(n),
            WindowEvent::Alloca => return self.enter_exception(cause::ALLOCA, None),
            WindowEvent::Illegal => return self.enter_exception(cause::ILLEGAL_INSTRUCTION, None),
        };
        self.ps = (self.ps & !PS_OWB_MASK) | (u32::from(owb & 0xF) << PS_OWB_SHIFT);
        self.sr.epc[1] = pc;
        self.ps |= PS_EXCM;
        self.cpu.pc = self.sr.vecbase.wrapping_add(vector);
    }

    /// Take an interrupt (RM §4.4.5.4 `takeinterrupt`, and
    /// `Exception(Level1InterruptCause)` for level 1). `EPC[n]` is the
    /// instruction that has not yet run.
    fn take_interrupt(&mut self, take: Take) {
        match take {
            Take::Level1 => self.enter_exception(cause::LEVEL1_INTERRUPT, None),
            Take::Level(level) => {
                let level = level.clamp(2, NMI_LEVEL);
                self.sr.epc[usize::from(level)] = self.cpu.pc;
                self.sr.eps[usize::from(level)] = self.ps();
                self.ps = (self.ps & !PS_INTLEVEL_MASK) | u32::from(level) | PS_EXCM;
                self.cpu.pc = self.sr.vecbase.wrapping_add(level_vecofs(level));
                if level == NMI_LEVEL {
                    self.ints.nmi_taken();
                }
            }
        }
    }

    /// Take the debug exception at `DEBUGLEVEL` (the BREAK page's sequence,
    /// shared by IBREAK and DBREAK): `EPC6 <- pc`, `EPS6 <- PS`,
    /// `DEBUGCAUSE <- bits`, `PS.INTLEVEL <- 6`, `PS.EXCM <- 1`.
    fn enter_debug(&mut self, bits: u32) {
        let level = usize::from(DEBUGLEVEL);
        self.sr.epc[level] = self.cpu.pc;
        self.sr.eps[level] = self.ps();
        self.sr.debugcause = bits;
        self.ps = (self.ps & !PS_INTLEVEL_MASK) | u32::from(DEBUGLEVEL) | PS_EXCM;
        self.cpu.pc = self.sr.vecbase.wrapping_add(level_vecofs(DEBUGLEVEL));
    }

    /// Turn a failed instruction fetch into `InstructionFetchErrorCause`
    /// with `EXCVADDR`, or into [`HartFault::TrapVectorFetch`] when the
    /// fetch that failed *was* a vector's.
    fn deliver_fetch_error(&mut self, e: MemoryError, pc: u32) -> Result<(), HartFault> {
        if is_vector_entry(self.sr.vecbase, pc) {
            return Err(HartFault::TrapVectorFetch { vector: pc });
        }
        let address = match e {
            MemoryError::InvalidAccess { address, .. }
            | MemoryError::Unaligned { address, .. }
            | MemoryError::Watchpoint { address, .. } => address,
        };
        self.enter_exception(cause::INSTRUCTION_FETCH_ERROR, Some(address));
        Ok(())
    }

    /// An instruction this emulator does not implement: the architectural
    /// illegal-instruction exception, or the strict stop.
    fn deliver_unsupported(
        &mut self,
        pc: u32,
        word: u32,
        len: u8,
        reason: &str,
    ) -> Option<SliceEnd> {
        log::debug!("mach: unsupported instruction {word:#08x} at {pc:#010x}: {reason}");
        if self.strict_unsupported {
            return Some(SliceEnd::Fault(HartFault::UnsupportedInstruction {
                pc,
                word,
                len,
            }));
        }
        self.enter_exception(cause::ILLEGAL_INSTRUCTION, None);
        None
    }

    /// Map a trap from the executors (or from the hart's own families) onto
    /// the architecture's causes.
    fn deliver_trap(
        &mut self,
        trap: Trap,
        pc: u32,
        inst: &Inst,
        len: u32,
    ) -> Result<(), HartFault> {
        if trap.kind != TrapKind::Exception {
            log::error!("mach: no architectural mapping for executor trap: {trap:?}");
            return Err(HartFault::UnmappedExecutorError { pc });
        }
        if trap.cause & TRAP_CAUSE_WATCHPOINT != 0 {
            // The bus reports the watchpoint *instead of* performing the
            // access, so nothing was written and EPC6 names the load/store
            // itself. Delivered whatever CINTLEVEL is: the bus has already
            // refused the access, and dropping it silently would be worse
            // than a debug exception the guest did not expect (the RM masks
            // DBREAK at and above DEBUGLEVEL; a guest at level 6 with a
            // watchpoint armed is not one this hart has met).
            let slot = trap.cause & 0xF;
            self.enter_debug(debugcause::DBREAK | (slot << debugcause::DBNUM_SHIFT));
            return Ok(());
        }
        match trap.cause {
            cause::ILLEGAL_INSTRUCTION => {
                // `ill`/`ill.n` and the hart's own refusals arrive here.
                if exec::is_unimplemented(inst) {
                    let word = lp_xt_inst::encode(inst)
                        .iter()
                        .rev()
                        .fold(0u32, |w, &b| (w << 8) | u32::from(b));
                    if let Some(SliceEnd::Fault(f)) = self.deliver_unsupported(
                        pc,
                        word,
                        len as u8,
                        "not implemented by this hart",
                    ) {
                        return Err(f);
                    }
                } else {
                    self.enter_exception(cause::ILLEGAL_INSTRUCTION, None);
                }
            }
            cause::SYSCALL
            | cause::ALLOCA
            | cause::INTEGER_DIVIDE_BY_ZERO
            | cause::COPROCESSOR0_DISABLED => {
                self.enter_exception(trap.cause, None);
            }
            cause::INSTRUCTION_FETCH_ERROR
            | cause::LOAD_STORE_ERROR
            | cause::LOAD_STORE_ALIGNMENT => {
                self.enter_exception(trap.cause, Some(trap.vaddr));
            }
            other => {
                log::error!(
                    "mach: no architectural mapping for executor trap cause {other}: {trap:?}"
                );
                return Err(HartFault::UnmappedExecutorError { pc });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
