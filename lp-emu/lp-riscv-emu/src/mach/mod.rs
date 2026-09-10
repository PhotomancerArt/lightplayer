//! `MachineHart` — one RV32 hart with machine-mode privilege.
//!
//! This is the privileged half of the emulator: architectural state for a
//! single hart (registers, `pc`, the M-mode CSR file, four hardware
//! triggers, cycle and instruction counters) plus the trap, `mret`, `wfi`
//! and interrupt-delivery behaviour the RISC-V privileged spec defines. It
//! runs instruction **slices** against a [`Bus`], reusing the crate's
//! user-mode executors for every non-`SYSTEM` instruction and handling
//! `SYSTEM` (`0x73`) and `c.ebreak` itself.
//!
//! It is **arch-only**. No MMIO, no SoC knowledge, no `std`. The ESP32-C6's
//! interrupt matrix, SYSTIMER, UART and friends live above it, in the
//! `lp-emu/esp/` crates; all this hart knows about them is one number —
//! "the highest-priority CPU interrupt currently asserted" — handed to it
//! through [`MachineHart::set_external`].
//!
//! # Where interrupts are polled
//!
//! There is deliberately **no per-instruction privilege check**. Pending
//! interrupt state is examined at exactly four points:
//!
//! - **(a)** on entry to [`MachineHart::run_slice`];
//! - **(b)** after `mret`, after `wfi`, and after any CSR write to
//!   `mstatus` or `mie` — the three instructions that can turn delivery on;
//! - **(c)** after a Store- or System-class instruction whose bus reports
//!   [`Bus::take_sideband`] `== true`: the hart re-reads
//!   [`Bus::pending_cpu_interrupt`] and polls, which is how an MMIO store
//!   that raises a peripheral interrupt is delivered before the next
//!   instruction retires — and how one that lowers a line stops being
//!   pending in the same breath. (Atomics are included: an AMO is a store.)
//!   On a RAM-only `Bus` both calls are inlined constants the optimizer
//!   deletes; `Memory` overrides neither.
//! - **(d)** whenever the owning machine calls
//!   [`MachineHart::poll_interrupts`] at a scheduler event.
//!
//! Nothing else in the slice loop looks at interrupt state.
//!
//! # The reset-state contract
//!
//! [`MachineHart::new`] leaves `mstatus` at the spec's reset value, which
//! has **`MIE = 0`**. Nothing in the esp-hal 1.1.1 stack ever sets it (M3
//! discovery §1h), so the *machine* — not this hart, and not the guest —
//! must seed `mstatus = 0x1888` (`MPP = 3`, `MPIE = 1`, `MIE = 1`, i.e.
//! [`csr::MSTATUS_BOOT`]) through [`MachineHart::set_csr_raw`] before
//! jumping to `_start`, exactly as the real ROM and 2nd-stage bootloader
//! leave the core. A hart left at the reset value will never take an
//! interrupt, and the firmware will idle forever in `wfi`.
//!
//! # Cycles, and what is *not* counted
//!
//! `cycle_count` advances by [`lp_emu_core::CycleModel::cycles_for`] for
//! every instruction the hart *attempts*, including one that traps — an
//! instruction that faults still cost a fetch and a decode. `instruction_count`
//! (`minstret`) advances only for instructions that **retire**, which is the
//! architectural definition. Trap entry and `mret` themselves carry no extra
//! cost: there is no measurement behind a number for those, and an invented
//! one would be indistinguishable from a measured one six months from now.

mod block;
pub mod csr;
pub mod translated;
pub mod trap;
pub mod trigger;

extern crate alloc;

use core::marker::PhantomData;

use alloc::{boxed::Box, string::String, vec::Vec};

use lp_emu_core::block::{BlockCache, BlockStats};
use lp_emu_core::{Bus, CycleModel, InstClass, MemoryAccessKind, MemoryError};

use crate::emu::{EmulatorError, FpRegs, LoggingDisabled, decode_execute};
use block::{Class, MAX_BLOCK_SLOTS, RvSlot};
use csr::CsrFile;
use trap::Exception;
use trigger::TriggerUnit;

/// `c.ebreak` — the only compressed `SYSTEM`-class instruction (RVC v2.0,
/// §16.9). The hart handles it rather than the executors, which treat
/// `ebreak` as "halt the user-mode emulator".
const C_EBREAK: u32 = 0x9002;

/// Opcode `SYSTEM`.
const OPCODE_SYSTEM: u8 = 0x73;
/// Opcode `STORE` — used only to tell a misaligned *store* from a
/// misaligned *load*, which [`MemoryError::Unaligned`] does not carry.
const OPCODE_STORE: u8 = 0x23;

/// `fence.i` — `MISC-MEM` with `funct3 = 1`, `imm = 0x001`, `rs1 = rd = 0`
/// (spec, Zifencei). The whole encoding is fixed, so one compare identifies
/// it.
///
/// This is the block cache's invalidation signal, and the emulator's half of
/// a contract whose other half is in the firmware: `lpvm-native`'s
/// `JitBuffer::from_code` emits a `fence.i` after every JIT publish, which is
/// what real silicon requires anyway. See [`MachineHart::on_fence_i`].
const FENCE_I: u32 = 0x0000_100f;

/// `funct12` of `ecall` / `ebreak` / `mret` / `wfi` (spec §3.3).
const FUNCT12_ECALL: u32 = 0x000;
const FUNCT12_EBREAK: u32 = 0x001;
const FUNCT12_MRET: u32 = 0x302;
const FUNCT12_WFI: u32 = 0x105;

/// The RV32F opcodes. The C6 is RV32IMAC with no FPU, and the crate's
/// executors decode `F` unconditionally — so the hart rejects these *before*
/// calling [`decode_execute`], or a guest `flw` would quietly succeed on a
/// core that has no `f` registers.
///
/// The list mirrors `emu::executor::float`'s `OPCODE_*` constants, which are
/// `pub(super)` inside `executor` and so not nameable from here.
#[inline]
const fn is_fp_opcode(opcode: u8) -> bool {
    matches!(opcode, 0x07 | 0x27 | 0x43 | 0x47 | 0x4B | 0x4F | 0x53)
}

/// Why a slice stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SliceEnd {
    /// The cycle budget ran out. `pc` points at the next instruction.
    BudgetExhausted,
    /// `wfi` retired and no enabled interrupt was pending. `pc` already
    /// points *past* the `wfi`, so the machine can jump guest time to the
    /// next scheduler event and poll — the deterministic idle skip.
    Wfi,
    /// `ebreak` or `c.ebreak` at `pc`, which has **not** been advanced.
    ///
    /// The machine gets first refusal, because P4's ROM hook table claims
    /// certain PCs (`rtc_get_reset_reason` and friends are `ebreak` stubs
    /// there). A machine that does not claim `pc` must call
    /// [`MachineHart::deliver_breakpoint`] to give the guest the
    /// architectural answer.
    Ebreak { pc: u32 },
    /// A peripheral asked the machine to take over before the next
    /// instruction ([`lp_emu_core::Bus::take_yield`]). `pc` already points
    /// at the next instruction, so the machine acts and resumes.
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
    /// [`decode_execute`] returned an error with no privileged-spec
    /// counterpart (the user-mode-only variants: syscall bookkeeping, the
    /// profile gate, the instruction-limit guard). Reaching this is a bug in
    /// this hart or in the executors, not in the guest.
    UnmappedExecutorError { pc: u32 },
}

/// What one instruction did to the slice loop.
enum StepOutcome {
    /// Keep going — the instruction retired, or trapped and the handler's
    /// first instruction is now at `pc`.
    Continue,
    /// The slice is over.
    End(SliceEnd),
}

/// One RV32 hart running in machine mode.
///
/// Generic over the bus rather than boxing it: the slice loop monomorphizes,
/// so a `Bus` whose `take_sideband` is the trait default costs nothing per
/// store. See the module docs for the polling points and the reset contract.
/// `Clone` and `Debug` are written out rather than derived: a derive would
/// add a `B: Clone` / `B: Debug` bound for the sake of a
/// `PhantomData<fn(&mut B)>`, which needs neither, and a machine could then
/// not snapshot a hart whose bus is not itself cloneable. Snapshotting the
/// hart is exactly the case that matters — the clone *is* the architectural
/// state.
pub struct MachineHart<B: Bus> {
    regs: [i32; 32],
    pc: u32,
    csr: CsrFile,
    triggers: TriggerUnit,
    instruction_count: u64,
    cycle_count: u64,
    cycle_model: CycleModel,
    hart_id: u32,
    /// Set by `wfi`, cleared by any enabled pending interrupt (spec §3.3.3:
    /// the wake condition ignores `mstatus.MIE`).
    wfi: bool,
    /// The highest-priority CPU interrupt the SoC's matrix currently
    /// asserts, if any. The matrix decides *which*; the hart is told one
    /// number, either by the owning machine at a scheduler event
    /// ([`MachineHart::set_external`]) or by the bus itself inside a slice
    /// ([`Bus::pending_cpu_interrupt`], polling point (c)).
    external: Option<u8>,
    /// The C6 core performs misaligned data accesses in hardware. The flag
    /// is the hart's copy of that fact — P4 sets it, and mirrors it onto the
    /// bus, which is the component that actually decides. The hart reads it
    /// only to notice a bus that contradicts it.
    allow_unaligned: bool,
    /// RV32F architectural state, which [`decode_execute`] requires by
    /// signature and this hart never uses: FP opcodes are rejected before
    /// the call, and the three F CSRs are illegal here. Held rather than
    /// constructed per instruction so the slice loop stays a loop.
    fp_unused: FpRegs,
    /// The pre-decoded block cache, built on first use.
    ///
    /// **Not architectural state.** Nothing observable may depend on a hit or
    /// a miss; it is absent from a snapshot, a clone starts empty, and
    /// `--no-block-cache` must produce byte-identical everything. See
    /// [`lp_emu_core::block`].
    cache: Option<Box<BlockCache<RvSlot<B>>>>,
    /// Configuration, not state: whether this hart may use a block cache at
    /// all ([`MachineHart::set_block_cache`]).
    block_cache: bool,
    /// How many `fence.i` instructions the guest has retired. A run of the
    /// product's firmware should show one per shader compile; a zero on a run
    /// that compiled a shader means the firmware's fence is not reaching the
    /// machine.
    fence_i_count: u64,
    /// A flush asked for while the cache was lifted out of the hart — see
    /// [`MachineHart::drain_block_flush`].
    block_flush_pending: bool,
    /// The installed translated core, if any, and the entry table that says
    /// where it may be entered (direct-mapped by `pc >> 1`, `0` = no entry).
    ///
    /// **Not architectural state**, exactly like `cache` above: absent from a
    /// snapshot, empty on a clone, and a run with no core installed must
    /// produce a byte-identical everything. See [`mach::translated`].
    ///
    /// [`mach::translated`]: translated
    core: Option<translated::BoxedCore<B>>,
    core_entries: Vec<u32>,
    _bus: PhantomData<fn(&mut B)>,
}

impl<B: Bus> Clone for MachineHart<B> {
    fn clone(&self) -> Self {
        Self {
            regs: self.regs,
            pc: self.pc,
            csr: self.csr.clone(),
            triggers: self.triggers.clone(),
            instruction_count: self.instruction_count,
            cycle_count: self.cycle_count,
            cycle_model: self.cycle_model,
            hart_id: self.hart_id,
            wfi: self.wfi,
            external: self.external,
            allow_unaligned: self.allow_unaligned,
            fp_unused: self.fp_unused.clone(),
            // The block cache is NOT copied. It is not architectural state,
            // so a cloned hart — the snapshot path — starts with an empty one
            // and re-decodes what it needs. Copying it would be a correctness
            // hazard rather than an optimisation: the clone's bus may hold
            // different bytes at the same addresses.
            cache: None,
            block_cache: self.block_cache,
            fence_i_count: self.fence_i_count,
            block_flush_pending: false,
            // Not copied, for the same reason the block cache is not: a
            // translated core holds host code for guest bytes that the
            // clone's bus may not have.
            core: None,
            core_entries: Vec::new(),
            _bus: PhantomData,
        }
    }
}

impl<B: Bus> core::fmt::Debug for MachineHart<B> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("MachineHart")
            .field("hart_id", &self.hart_id)
            .field("pc", &format_args!("{:#010x}", self.pc))
            .field("csr", &self.csr)
            .field("triggers", &self.triggers)
            .field("instruction_count", &self.instruction_count)
            .field("cycle_count", &self.cycle_count)
            .field("cycle_model", &self.cycle_model)
            .field("wfi", &self.wfi)
            .field("external", &self.external)
            .field("allow_unaligned", &self.allow_unaligned)
            .finish()
    }
}

impl<B: Bus> MachineHart<B> {
    /// A hart at reset: zeroed registers, `pc = 0`, `mstatus` at the spec's
    /// reset value (**`MIE = 0`** — see the module docs), no triggers armed.
    #[must_use]
    pub fn new(hart_id: u32) -> Self {
        Self {
            regs: [0; 32],
            pc: 0,
            csr: CsrFile::new(),
            triggers: TriggerUnit::new(),
            instruction_count: 0,
            cycle_count: 0,
            cycle_model: CycleModel::Esp32C6,
            hart_id,
            wfi: false,
            external: None,
            allow_unaligned: false,
            fp_unused: FpRegs::new(),
            cache: None,
            block_cache: true,
            fence_i_count: 0,
            block_flush_pending: false,
            core: None,
            core_entries: Vec::new(),
            _bus: PhantomData,
        }
    }

    // --- the translated core ----------------------------------------------

    /// Install a translated core and the guest pcs at which it may be
    /// entered, replacing any core already installed.
    ///
    /// See [`mach::translated`](translated) for what a core is and what it
    /// promises. Installing one changes no architectural state and no
    /// transcript; it is the mechanism, not the decision.
    pub fn set_translated_core(&mut self, core: translated::BoxedCore<B>, entries: &[u32]) {
        self.core_entries = translated::entry_table(entries);
        self.core = Some(core);
    }

    /// Remove the installed core, if any, and go back to interpreting.
    ///
    /// This is what `--interpreter` reaches: every build can turn the
    /// translator off entirely and must then print an identical everything.
    pub fn clear_translated_core(&mut self) {
        self.core = None;
        self.core_entries = Vec::new();
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
        self.pc
    }

    #[inline]
    pub fn set_pc(&mut self, pc: u32) {
        self.pc = pc;
    }

    #[inline]
    #[must_use]
    pub const fn regs(&self) -> &[i32; 32] {
        &self.regs
    }

    #[inline]
    pub fn regs_mut(&mut self) -> &mut [i32; 32] {
        &mut self.regs
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

    /// Change the per-instruction cost model.
    ///
    /// Invalidates the block cache: a cached block carries the most cycles it
    /// can charge *under the model it was built against*
    /// ([`lp_emu_core::block::Block::max_cycles`]), and that bound is what
    /// lets a block run whole with no per-instruction deadline compare. A
    /// stale bound would let a block run past a deadline it should have
    /// stopped inside.
    #[inline]
    pub fn set_cycle_model(&mut self, model: CycleModel) {
        self.cycle_model = model;
        self.invalidate_blocks();
    }

    // --- the block cache ---------------------------------------------------

    /// Turn the pre-decoded block cache on or off. Default: on.
    ///
    /// Off is `--no-block-cache`: the bring-up tool, the bisection tool, and
    /// the identity oracle this milestone leans on hardest — the same binary
    /// with the cache off must print identical everything.
    ///
    /// It is also how [`lp_emu_core::block`]'s one architectural exclusion is
    /// expressed: an ESP32-C6 booting `--boot rom-up` runs the real mask ROM
    /// and the real ESP-IDF second-stage bootloader as *guest* code, and
    /// those copy segments into RAM and jump into them without ever emitting
    /// a `fence.i`. We own neither, so the machine turns the cache off there
    /// (M5 MD13).
    #[inline]
    pub fn set_block_cache(&mut self, on: bool) {
        self.block_cache = on;
        if !on {
            self.cache = None;
        }
    }

    #[inline]
    #[must_use]
    pub const fn block_cache(&self) -> bool {
        self.block_cache
    }

    /// What the cache did, or `None` when it was never built.
    #[inline]
    #[must_use]
    pub fn block_stats(&self) -> Option<BlockStats> {
        self.cache.as_ref().map(|c| c.stats())
    }

    /// `fence.i` instructions the guest has retired.
    #[inline]
    #[must_use]
    pub const fn fence_i_count(&self) -> u64 {
        self.fence_i_count
    }

    /// Forget every cached block.
    ///
    /// For a machine that has written guest code from the host side with no
    /// guest `fence.i` behind it and cannot name the window: a snapshot
    /// restore, a reboot.
    #[inline]
    pub fn invalidate_blocks(&mut self) {
        if let Some(core) = self.core.as_mut() {
            core.invalidate(None);
        }
        match self.cache.as_mut() {
            Some(cache) => cache.invalidate_all(),
            // The cache is lifted out of `self` for the whole of a slice
            // (see [`MachineHart::run_slice_cached`]), so a flush asked for
            // from inside one is recorded and applied where the cache is.
            None => self.block_flush_pending = true,
        }
    }

    /// Forget every cached block whose instructions overlap `[lo, hi)`.
    ///
    /// The emulator-side code-writer funnels: a flash-cache MMU refill and a
    /// ROM-hook `ebreak` patch both write guest code that no guest `fence.i`
    /// follows, and both know exactly which window they wrote. Called at a
    /// slice boundary, where the cache is back in the hart; asked for from
    /// inside a slice it degrades to a whole flush, which is conservative in
    /// the safe direction.
    #[inline]
    pub fn invalidate_block_range(&mut self, lo: u32, hi: u32) {
        if let Some(core) = self.core.as_mut() {
            core.invalidate(Some((lo, hi)));
        }
        match self.cache.as_mut() {
            Some(cache) => cache.invalidate_range(lo, hi),
            None => self.block_flush_pending = true,
        }
    }

    /// The `fence.i` hook: the guest has published instructions, so every
    /// pre-decoded block is suspect.
    ///
    /// A whole-cache flush rather than a range (M5 MD12). It fires once per
    /// shader compile — a handful of times in a render run — so range
    /// precision would buy nothing measurable and would cost correctness
    /// surface. The bus is told too, so a `--strict-bus` run can clear its
    /// "written but not yet published" marks.
    ///
    /// It has a second job for a translated core, and it is the *only* thing
    /// that gives it one: the guest writes its shader code and publishes it
    /// with exactly this instruction, so `fence.i` is where a core learns
    /// that guest code it has translated is now different code. M5's fence
    /// ruling (MD12) is what makes that a contract rather than a hope. The
    /// invalidation itself rides on [`MachineHart::invalidate_blocks`], which
    /// tells the core and the block cache in that order.
    #[inline]
    fn on_fence_i(&mut self, bus: &mut B) {
        self.fence_i_count += 1;
        self.invalidate_blocks();
        bus.note_fence_i();
    }

    /// Apply a flush that was asked for while the cache was lifted out of the
    /// hart. `fence.i` is the reason this exists.
    #[inline]
    fn drain_block_flush(&mut self, cache: &mut BlockCache<RvSlot<B>>) {
        if self.block_flush_pending {
            self.block_flush_pending = false;
            cache.invalidate_all();
        }
    }

    /// See [`MachineHart::allow_unaligned`].
    #[inline]
    pub fn set_allow_unaligned(&mut self, allow: bool) {
        self.allow_unaligned = allow;
    }

    #[inline]
    #[must_use]
    pub const fn allow_unaligned(&self) -> bool {
        self.allow_unaligned
    }

    /// Read-only view of the CSR file, for the machine's state dumps and
    /// snapshotting.
    #[inline]
    #[must_use]
    pub const fn csr(&self) -> &CsrFile {
        &self.csr
    }

    /// Read-only view of the trigger unit.
    #[inline]
    #[must_use]
    pub const fn triggers(&self) -> &TriggerUnit {
        &self.triggers
    }

    /// True while the hart is parked in `wfi`.
    #[inline]
    #[must_use]
    pub const fn is_wfi(&self) -> bool {
        self.wfi
    }

    // --- machine-side state seeding ---------------------------------------

    /// Write a machine-mode CSR from *outside* the guest — the reset-vector
    /// seeding path (`mstatus = 0x1888`), and state restore.
    ///
    /// WARL rules still apply (`mstatus.MPP` stays 3), but the read-only and
    /// unknown-CSR checks do not, and nothing is polled or armed as a side
    /// effect. The trigger CSRs are deliberately *not* reachable through it:
    /// arming a watchpoint means telling the bus, which needs a `&mut B` the
    /// machine has not handed over. Returns `false` for a CSR this hart does
    /// not keep as state.
    pub fn set_csr_raw(&mut self, csr_num: u16, value: u32) -> bool {
        match csr_num {
            csr::MSTATUS => {
                self.csr.write_mstatus(value);
                true
            }
            csr::MIE => {
                self.csr.mie = value;
                true
            }
            csr::MTVEC => {
                self.csr.mtvec = value;
                true
            }
            csr::MSCRATCH => {
                self.csr.mscratch = value;
                true
            }
            csr::MEPC => {
                self.csr.mepc = value;
                true
            }
            csr::MCAUSE => {
                self.csr.mcause = value;
                true
            }
            csr::MTVAL => {
                self.csr.mtval = value;
                true
            }
            other => self.csr.write_scratch(other, value),
        }
    }

    /// Jump guest time forward to `cycle` — the deterministic idle skip.
    ///
    /// After [`SliceEnd::Wfi`] the hart is parked with nothing to do until
    /// the scheduler's next event, and simulating the wait one `wfi` at a
    /// time would burn host seconds to produce no guest state. The machine
    /// moves the clock instead.
    ///
    /// Never moves backwards: a machine that could rewind the cycle counter
    /// would make `mcycle` non-monotonic, and the one CSR the C6 firmware
    /// reads for spacing (`PCCR`) reads from it.
    pub fn advance_to_cycle(&mut self, cycle: u64) {
        if cycle > self.cycle_count {
            self.cycle_count = cycle;
        }
    }

    // --- interrupt input ---------------------------------------------------

    /// Tell the hart which CPU interrupt the SoC's matrix currently asserts,
    /// or `None` for "nothing".
    ///
    /// The matrix — not the hart — decides *which* one: enabled, level
    /// asserted, and priority at or above `MXINT_THRESH` (M3 discovery §2g).
    /// The hart only masks it against `mie` and `mstatus.MIE`.
    ///
    /// Setting it does not deliver anything; the machine polls.
    #[inline]
    pub fn set_external(&mut self, external: Option<u8>) {
        self.external = external;
    }

    #[inline]
    #[must_use]
    pub const fn external(&self) -> Option<u8> {
        self.external
    }

    /// Examine pending interrupt state; deliver one if it is takeable.
    /// Returns `true` when a trap was taken.
    ///
    /// Two separate conditions, in spec order (§3.3.3, §3.1.9):
    ///
    /// 1. **Wake.** A pending interrupt that is enabled in `mie` clears the
    ///    `wfi` park *regardless of `mstatus.MIE`* — the spec's wake
    ///    condition ignores the global enable.
    /// 2. **Deliver.** The same interrupt is taken only when `mstatus.MIE`
    ///    is also set.
    pub fn poll_interrupts(&mut self) -> bool {
        let Some(n) = self.external else {
            return false;
        };
        if self.csr.mie & (1u32 << (n & 31)) == 0 {
            return false;
        }

        // (1) wake, even with MIE clear.
        self.wfi = false;

        // (2) deliver, only with MIE set.
        if !self.csr.mie_enabled() {
            return false;
        }
        self.pc = trap::deliver_interrupt(&mut self.csr, n, self.pc);
        true
    }

    /// Give the guest the architectural answer to an `ebreak` the machine
    /// chose not to claim: `mcause = 3`, `mtval = 0`, `mepc` = the `ebreak`
    /// itself (spec §3.1.14 — `mepc` names the instruction that encountered
    /// the exception, and `ebreak` does not advance past itself).
    pub fn deliver_breakpoint(&mut self, pc: u32) {
        self.pc = trap::deliver_exception(&mut self.csr, Exception::Breakpoint, 0, pc);
    }

    // --- the slice loop ----------------------------------------------------

    /// Run instructions until the cycle `budget` is spent or something ends
    /// the slice.
    ///
    /// `budget` is in **cycles** — the scheduler's next deadline minus now —
    /// and is honoured to within one instruction's cost: the check happens
    /// before each fetch, so the last instruction of a slice may overshoot
    /// by its own cost and no more. Cycles consumed are
    /// `cycle_count()` differenced across the call.
    pub fn run_slice(&mut self, bus: &mut B, budget: u64) -> SliceEnd {
        // (a) poll on entry.
        self.poll_interrupts();

        // The deadline as an absolute cycle: one compare per instruction
        // instead of a subtract and a compare. `saturating_add` keeps a
        // `u64::MAX` budget meaning "never", as the subtraction form did.
        let end = self.cycle_count.saturating_add(budget);

        // A block cache decodes ahead and then executes without fetching
        // again, so it may only run over a bus whose fetches have no
        // consequence beyond returning the word — see
        // [`Bus::fetch_is_pure`]. Read once per slice, and re-read whenever
        // an instruction that could have changed the answer runs.
        if self.block_cache && bus.fetch_is_pure() {
            self.run_slice_cached(bus, end)
        } else {
            self.run_slice_stepping(bus, end)
        }
    }

    /// The slice loop as it has always been: fetch, step, charge, repeat.
    ///
    /// Still the whole of `--no-block-cache`, of every bus that does not
    /// claim [`Bus::fetch_is_pure`], and of any address the block decoder
    /// refuses. The cached loop reuses [`MachineHart::step_once`] for exactly
    /// those addresses, so there is one copy of the per-instruction path and
    /// not two that can drift.
    fn run_slice_stepping(&mut self, bus: &mut B, end: u64) -> SliceEnd {
        loop {
            if self.cycle_count >= end {
                return SliceEnd::BudgetExhausted;
            }
            if let Some(over) = self.step_once(bus) {
                return over;
            }
        }
    }

    /// One instruction: fetch it, run it, charge what the bus billed.
    ///
    /// `None` means "keep going"; `Some` ends the slice.
    #[inline(always)]
    fn step_once(&mut self, bus: &mut B) -> Option<SliceEnd> {
        let pc = self.pc;
        // The bus's trace and spin detector are only worth having if the
        // pc and the cycle on each line are this instruction's.
        bus.set_issuing(pc, self.cycle_count);
        let inst_word = match bus.fetch_instruction(pc) {
            Ok(word) => word,
            Err(e) => {
                // A fetch that faulted still went to memory, so whatever
                // the bus charged for it is charged here rather than
                // carried into the next instruction's total.
                self.charge_memory(bus);
                return match self.deliver_fetch_error(e, pc) {
                    Ok(()) => None,
                    Err(fault) => Some(SliceEnd::Fault(fault)),
                };
            }
        };

        let outcome = self.step(bus, pc, inst_word);
        // Drained after the instruction, so the fetch and any load or
        // store it made are charged together, once, in the same place
        // `charge` bills the instruction's class.
        self.charge_memory(bus);
        match outcome {
            StepOutcome::Continue => None,
            StepOutcome::End(end) => Some(end),
        }
    }

    /// The same slice, run out of pre-decoded blocks.
    ///
    /// The cache is lifted out of `self` for the whole slice so the arena and
    /// the hart's registers are two disjoint borrows rather than one; that is
    /// two pointer moves per slice, against ~419 instructions per slice on
    /// `render-basic`.
    fn run_slice_cached(&mut self, bus: &mut B, end: u64) -> SliceEnd {
        let mut cache = self
            .cache
            .take()
            .unwrap_or_else(|| Box::new(BlockCache::with_defaults()));
        self.drain_block_flush(&mut cache);
        let out = self.run_blocks(&mut cache, bus, end);
        self.cache = Some(cache);
        out
    }

    fn run_blocks(&mut self, cache: &mut BlockCache<RvSlot<B>>, bus: &mut B, end: u64) -> SliceEnd {
        loop {
            if self.cycle_count >= end {
                return SliceEnd::BudgetExhausted;
            }
            let pc = self.pc;
            // The translated-core entry, ahead of the block cache: one table
            // read and one compare when no core is installed, which is what
            // keeps this free on the interpreted path.
            if let Some(core) = self.core.as_mut()
                && self.core_entries[translated::entry_slot(pc)] == pc
            {
                let cx = translated::EntryCx {
                    regs: &mut self.regs,
                    pc,
                    cycle_count: self.cycle_count,
                    instruction_count: self.instruction_count,
                    end,
                };
                if let translated::RunOutcome::Ran {
                    pc: new_pc,
                    cycle_count,
                    instruction_count,
                    after_store,
                } = core.run(cx, bus)
                    // The no-progress guard. A core whose first block does
                    // not fit the remaining budget returns `Ran` having done
                    // nothing; without this the loop would ask it again
                    // forever. The interpreter runs the block instead, which
                    // is exactly what happens when the budget is short.
                    && (new_pc != pc || instruction_count != self.instruction_count)
                {
                    self.pc = new_pc;
                    self.cycle_count = cycle_count;
                    self.instruction_count = instruction_count;
                    if after_store {
                        // Polling point (c), byte for byte what `run_block`
                        // does after an interpreted store.
                        if bus.take_sideband() {
                            self.resample_external(bus);
                        }
                        if bus.take_yield() {
                            return SliceEnd::BusYield;
                        }
                    }
                    continue;
                }
            }
            let block = match cache.lookup(pc) {
                Some(block) => block,
                None => {
                    let model = self.cycle_model;
                    match cache.build(pc, model, |out| decode_block(bus, pc, out)) {
                        Some(block) => block,
                        None => {
                            // Nothing cacheable starts here — a `SYSTEM`, a
                            // fence, an atomic, an FP opcode, an encoding the
                            // executors reject, or a fetch that faults. Run
                            // exactly one instruction the way the
                            // single-stepping loop always has.
                            if let Some(over) = self.step_once(bus) {
                                self.drain_block_flush(cache);
                                return over;
                            }
                            // That instruction may have been a `fence.i`, and
                            // the cache it wanted flushed is out here rather
                            // than in the hart.
                            self.drain_block_flush(cache);
                            // It may also have been the CSR write
                            // that arms an execute watchpoint, which is the
                            // one thing that can make decoding ahead unsafe
                            // in the middle of a slice.
                            if !bus.fetch_is_pure() {
                                cache.invalidate_all();
                                return self.run_slice_stepping(bus, end);
                            }
                            continue;
                        }
                    }
                }
            };
            // `--strict-bus` only: these instructions are about to run
            // without the bus seeing a fetch for them, so the missing-fence
            // checker is told directly. Empty and inlined away otherwise.
            bus.note_cached_execute(block.pc, block.bytes);

            // The budget rule (M5 MD3). `max_cycles` is what the block can
            // charge at most, so a block that fits leaves every interior
            // instruction boundary strictly below `end` and needs no
            // per-instruction compare; one that does not fit runs with the
            // compare the single-stepping loop uses.
            let whole = self.cycle_count.saturating_add(u64::from(block.max_cycles)) <= end;
            // The slot arena is borrowed for the length of the block so the
            // inner loop reads a slice rather than re-deriving `&self.arena`
            // and bounds-checking on every instruction; the counter the
            // borrow would conflict with is updated out here.
            let (out, ran) = {
                let slots = cache.slots_of(&block);
                self.run_block(slots, bus, block.pc, whole, end)
            };
            cache.note_slots_run(ran);
            if let Some(over) = out {
                return over;
            }
        }
    }

    /// Run one block's slots.
    ///
    /// Every per-instruction hook [`MachineHart::step`] runs today runs here,
    /// at the same point and in the same order: `set_issuing`, execution,
    /// `instruction_count`, `charge`, the `pc` update, then — for a store or
    /// an atomic — the bus side-band and the yield. The only thing that
    /// changed is how the handler was found.
    ///
    /// `None` means the block ended normally and the outer loop should look
    /// up the next one.
    fn run_block(
        &mut self,
        slots: &[RvSlot<B>],
        bus: &mut B,
        block_pc: u32,
        whole: bool,
        end: u64,
    ) -> (Option<SliceEnd>, u32) {
        let mut pc = block_pc;
        let mut ran = 0u32;
        for slot in slots {
            if !whole && self.cycle_count >= end {
                return (Some(SliceEnd::BudgetExhausted), ran);
            }
            // F10: per slot, not per block. A trap in the middle of a block
            // would otherwise report the block's first `pc` to the bus's
            // trace and to its unmapped-site dedup, which is keyed on
            // `(pc, address)`.
            bus.set_issuing(pc, self.cycle_count);

            let result = match (slot.handler)(slot.word, pc, &mut self.regs, bus) {
                Ok(result) => result,
                Err(e) => {
                    self.charge(InstClass::System);
                    let out = match self.deliver_executor_error(e, pc, slot.word) {
                        Ok(()) => None,
                        Err(fault) => Some(SliceEnd::Fault(fault)),
                    };
                    self.charge_memory(bus);
                    return (out, ran);
                }
            };

            debug_assert!(
                !result.should_halt && !result.syscall,
                "the hart handles ecall/ebreak itself; a block never holds one"
            );

            self.instruction_count += 1;
            self.charge(result.class);
            self.pc = result
                .new_pc
                .unwrap_or(pc.wrapping_add(u32::from(result.inst_size)));
            ran += 1;

            // (c) an MMIO store may have changed interrupt state, and it may
            // have changed something only the machine can act on.
            if matches!(result.class, InstClass::Store | InstClass::Atomic) {
                if bus.take_sideband() {
                    self.resample_external(bus);
                }
                if bus.take_yield() {
                    self.charge_memory(bus);
                    return (Some(SliceEnd::BusYield), ran);
                }
            }
            self.charge_memory(bus);

            // Defence in depth, and the reason a classification mistake can
            // only make a block shorter. Anything that moved the hart off the
            // decoder's straight line — a taken branch, a jump, a trap, an
            // interrupt delivered by the side-band above, an instruction
            // whose width the executor disagrees about — leaves the block and
            // is looked up again by pc. In a correct build this fires exactly
            // at a taken terminator.
            let straight_on = pc.wrapping_add(u32::from(slot.width));
            if self.pc != straight_on {
                return (None, ran);
            }
            pc = straight_on;
        }
        (None, ran)
    }

    /// One instruction: the hart's own `SYSTEM` handling, the FP rejection,
    /// or the shared executors.
    fn step(&mut self, bus: &mut B, pc: u32, inst_word: u32) -> StepOutcome {
        let compressed = (inst_word & 0b11) != 0b11;

        if compressed {
            if inst_word & 0xFFFF == C_EBREAK {
                return StepOutcome::End(SliceEnd::Ebreak { pc });
            }
        } else {
            let opcode = (inst_word & 0x7F) as u8;
            if opcode == OPCODE_SYSTEM {
                return self.execute_system(bus, pc, inst_word);
            }
            if is_fp_opcode(opcode) {
                self.charge(InstClass::System);
                self.deliver_illegal(pc, inst_word, "RV32F opcode on a hart with no FPU");
                return StepOutcome::Continue;
            }
        }

        let executed = decode_execute::<LoggingDisabled, B>(
            inst_word,
            pc,
            &mut self.regs,
            bus,
            &mut self.fp_unused,
        );
        let result = match executed {
            Ok(result) => result,
            Err(e) => {
                self.charge(InstClass::System);
                return match self.deliver_executor_error(e, pc, inst_word) {
                    Ok(()) => StepOutcome::Continue,
                    Err(fault) => StepOutcome::End(SliceEnd::Fault(fault)),
                };
            }
        };

        debug_assert!(
            !result.should_halt && !result.syscall,
            "the hart handles ecall/ebreak itself; the executors must never report them"
        );

        self.instruction_count += 1;
        self.charge(result.class);
        self.pc = result
            .new_pc
            .unwrap_or(pc.wrapping_add(u32::from(result.inst_size)));

        match result.class {
            // (c) an MMIO store may have changed interrupt state, and it may
            // have changed something only the machine can act on.
            InstClass::Store | InstClass::Atomic => {
                if bus.take_sideband() {
                    self.resample_external(bus);
                }
                if bus.take_yield() {
                    return StepOutcome::End(SliceEnd::BusYield);
                }
            }
            // The guest has published instructions. A fence is never inside a
            // block (M5 MD2), so this hook is always reached on the
            // single-stepping path and always at a block boundary.
            InstClass::Fence if inst_word == FENCE_I => self.on_fence_i(bus),
            _ => {}
        }

        StepOutcome::Continue
    }

    /// Answer a raised bus side-band: re-read the matrix and poll.
    ///
    /// The re-read is what makes polling point (c) able to change an outcome
    /// at all. Before it, the hart's `external` was only ever written by the
    /// owning machine at a scheduler event, so an MMIO store that raised a
    /// line inside a slice could not be delivered until the next event —
    /// which for a UART's TX-done or a software interrupt is "never".
    /// [`Bus::pending_cpu_interrupt`] is the bus's answer, and a bus that
    /// raises the side-band is required to implement it.
    #[inline]
    fn resample_external(&mut self, bus: &B) {
        self.external = bus.pending_cpu_interrupt();
        self.poll_interrupts();
    }

    /// `SYSTEM` (`0x73`): `ecall`, `ebreak`, `mret`, `wfi`, and the six CSR
    /// instructions.
    fn execute_system(&mut self, bus: &mut B, pc: u32, inst_word: u32) -> StepOutcome {
        let funct3 = (inst_word >> 12) & 0x7;
        if funct3 == 0 {
            return match (inst_word >> 20) & 0xFFF {
                FUNCT12_ECALL => {
                    self.charge(InstClass::System);
                    // `mtval` is 0 for an environment call (spec §3.1.16).
                    self.pc =
                        trap::deliver_exception(&mut self.csr, Exception::MachineEnvCall, 0, pc);
                    StepOutcome::Continue
                }
                // Deliberately *not* charged and `pc` deliberately not
                // advanced: the machine gets the instruction back untouched,
                // and may re-present it after running a ROM hook. Charging
                // here would bill it twice.
                FUNCT12_EBREAK => StepOutcome::End(SliceEnd::Ebreak { pc }),
                FUNCT12_MRET => {
                    self.charge(InstClass::System);
                    self.instruction_count += 1;
                    self.pc = trap::mret(&mut self.csr);
                    // (b) `mret` restores MIE.
                    self.poll_interrupts();
                    StepOutcome::Continue
                }
                FUNCT12_WFI => {
                    self.charge(InstClass::System);
                    self.instruction_count += 1;
                    self.pc = pc.wrapping_add(4);
                    self.wfi = true;
                    // (b) an already-pending interrupt un-parks immediately,
                    // and may be delivered here rather than after the skip.
                    self.poll_interrupts();
                    if self.wfi {
                        StepOutcome::End(SliceEnd::Wfi)
                    } else {
                        StepOutcome::Continue
                    }
                }
                _ => {
                    self.charge(InstClass::System);
                    self.deliver_illegal(pc, inst_word, "unknown SYSTEM funct12");
                    StepOutcome::Continue
                }
            };
        }

        let rd = ((inst_word >> 7) & 0x1F) as usize;
        let rs1 = ((inst_word >> 15) & 0x1F) as usize;
        let csr_num = ((inst_word >> 20) & 0xFFF) as u16;

        // Register-sourced forms read `rs1`; the immediate forms take the
        // zero-extended 5-bit `zimm` from the same field (spec §2.1).
        let (source, source_is_zero) = match funct3 {
            0b001 | 0b010 | 0b011 => (if rs1 == 0 { 0 } else { self.regs[rs1] as u32 }, rs1 == 0),
            0b101 | 0b110 | 0b111 => (rs1 as u32, rs1 == 0),
            _ => {
                self.charge(InstClass::System);
                self.deliver_illegal(pc, inst_word, "unknown CSR funct3");
                return StepOutcome::Continue;
            }
        };

        // CSRRS/CSRRC with a zero source are the spec's read-only forms and
        // must not write; CSRRW/CSRRWI always write.
        let is_write_op = matches!(funct3, 0b001 | 0b101);
        let writes = is_write_op || !source_is_zero;

        if writes && csr::is_read_only(csr_num) {
            self.charge(InstClass::System);
            self.deliver_illegal(pc, inst_word, "write to a read-only CSR");
            return StepOutcome::Continue;
        }

        // Read before charging: a counter CSR reports the count as of the
        // instruction *before* the one reading it, which is the only reading
        // that composes (two back-to-back reads differ by exactly the cost of
        // one read).
        let Some(old) = self.csr_read(csr_num) else {
            self.charge(InstClass::System);
            log::debug!(
                "mach: illegal instruction at {pc:#010x}: unimplemented CSR {csr_num:#05x} \
                 (instruction {inst_word:#010x})"
            );
            self.deliver_illegal(pc, inst_word, "unimplemented CSR");
            return StepOutcome::Continue;
        };
        self.charge(InstClass::System);

        if writes {
            let value = match funct3 {
                0b001 | 0b101 => source,
                0b010 | 0b110 => old | source,
                _ => old & !source,
            };
            self.csr_write(bus, csr_num, value);
        }

        if rd != 0 {
            self.regs[rd] = old as i32;
        }
        self.instruction_count += 1;
        self.pc = pc.wrapping_add(4);

        // (b) `mstatus` and `mie` are the two CSRs that turn delivery on.
        if writes && matches!(csr_num, csr::MSTATUS | csr::MIE) {
            self.poll_interrupts();
        }
        // (c) System-class: consume any side-band the bus is holding.
        if bus.take_sideband() {
            self.resample_external(bus);
        }

        StepOutcome::Continue
    }

    /// Read a CSR, or `None` when the number is not implemented (an illegal
    /// instruction — never a silent zero).
    fn csr_read(&self, csr_num: u16) -> Option<u32> {
        Some(match csr_num {
            csr::MSTATUS => self.csr.mstatus,
            csr::MISA => csr::MISA_VALUE,
            csr::MIE => self.csr.mie,
            csr::MTVEC => self.csr.mtvec,
            csr::MSCRATCH => self.csr.mscratch,
            csr::MEPC => self.csr.mepc,
            csr::MCAUSE => self.csr.mcause,
            csr::MTVAL => self.csr.mtval,
            // Pending state lives in the SoC's matrix, not here; `mip`
            // reports the one number the matrix has asserted. Nothing in the
            // esp stack reads it (discovery §2i).
            csr::MIP => self.external.map_or(0, |n| 1u32 << (n & 31)),

            csr::MVENDORID | csr::MARCHID | csr::MIMPID => 0,
            csr::MHARTID => self.hart_id,

            csr::MCYCLE | csr::CYCLE => self.cycle_count as u32,
            csr::MCYCLEH | csr::CYCLEH => (self.cycle_count >> 32) as u32,
            csr::MINSTRET | csr::INSTRET => self.instruction_count as u32,
            csr::MINSTRETH | csr::INSTRETH => (self.instruction_count >> 32) as u32,
            // Espressif's performance counter — the only cycle-ish CSR the
            // C6 firmware reads, and only to space RNG samples (§5).
            csr::PCCR_MACHINE | csr::PCCR_USER => self.cycle_count as u32,

            csr::TSELECT => self.triggers.tselect(),
            csr::TDATA1 => self.triggers.tdata1(),
            csr::TDATA2 => self.triggers.tdata2(),
            csr::TCONTROL => self.triggers.tcontrol(),

            other => {
                let value = self.csr.read_scratch(other)?;
                log::trace!("mach: read scratch CSR {other:#05x} = {value:#010x}");
                value
            }
        })
    }

    /// Write a CSR that [`Self::csr_read`] accepted. Trigger writes re-derive
    /// the affected slots and push them to the bus.
    fn csr_write(&mut self, bus: &mut B, csr_num: u16, value: u32) {
        match csr_num {
            csr::MSTATUS => self.csr.write_mstatus(value),
            csr::MIE => self.csr.mie = value,
            csr::MTVEC => self.csr.mtvec = value,
            csr::MSCRATCH => self.csr.mscratch = value,
            csr::MEPC => self.csr.mepc = value,
            csr::MCAUSE => self.csr.mcause = value,
            csr::MTVAL => self.csr.mtval = value,

            csr::TSELECT => {
                self.triggers.write_tselect(value);
                self.arm_selected(bus);
            }
            csr::TDATA1 => {
                self.triggers.write_tdata1(value);
                self.arm_selected(bus);
            }
            csr::TDATA2 => {
                self.triggers.write_tdata2(value);
                self.arm_selected(bus);
            }
            csr::TCONTROL => {
                // `mte` is global, so every slot's arming can change.
                self.triggers.write_tcontrol(value);
                for slot in 0..trigger::TRIGGER_COUNT {
                    bus.set_watchpoint(slot, self.triggers.watchpoint(slot));
                }
            }

            other => {
                if self.csr.write_scratch(other, value) {
                    log::trace!("mach: wrote scratch CSR {other:#05x} = {value:#010x}");
                } else {
                    // Derived and read-only-in-practice CSRs (`misa`, `mip`,
                    // the counters): the write is discarded, not an error.
                    log::trace!("mach: discarded write to derived CSR {other:#05x}");
                }
            }
        }
    }

    #[inline]
    fn arm_selected(&mut self, bus: &mut B) {
        let slot = self.triggers.selected();
        bus.set_watchpoint(slot, self.triggers.watchpoint(slot));
    }

    // --- trap delivery -----------------------------------------------------

    #[inline]
    fn charge(&mut self, class: InstClass) {
        self.cycle_count += u64::from(self.cycle_model.cycles_for(class));
    }

    /// Charge whatever the bus's memory system billed for this instruction's
    /// accesses ([`Bus::take_memory_cost`]).
    ///
    /// The hart deliberately learns nothing from it: no address, no width,
    /// no reason. A bus with no memory-cost model returns a constant zero
    /// and this compiles to nothing.
    #[inline]
    fn charge_memory(&mut self, bus: &mut B) {
        let extra = bus.take_memory_cost();
        if extra != 0 {
            self.cycle_count += u64::from(extra);
        }
    }

    fn deliver_illegal(&mut self, pc: u32, inst_word: u32, reason: &str) {
        log::debug!("mach: illegal instruction {inst_word:#010x} at {pc:#010x}: {reason}");
        // `mtval` carries the faulting instruction word (spec §3.1.16).
        self.pc =
            trap::deliver_exception(&mut self.csr, Exception::IllegalInstruction, inst_word, pc);
    }

    /// Turn a failed instruction fetch into a trap, or into a
    /// [`HartFault::TrapVectorFetch`] when the fetch that failed *was* the
    /// handler's.
    fn deliver_fetch_error(&mut self, e: MemoryError, pc: u32) -> Result<(), HartFault> {
        let (exception, tval, slot) = match e {
            MemoryError::InvalidAccess { address, .. } => {
                (Exception::InstructionAccessFault, address, None)
            }
            MemoryError::Unaligned { address, .. } => {
                (Exception::InstructionAddressMisaligned, address, None)
            }
            MemoryError::Watchpoint { address, slot, .. } => {
                (Exception::Breakpoint, address, Some(slot))
            }
        };
        if let Some(slot) = slot {
            self.triggers.set_hit(usize::from(slot));
        }

        // A fault at the vector we would jump to is a double fault; real
        // silicon loops, and looping here would be a hang with no diagnosis.
        let vector = self.csr.mtvec_base();
        if pc == vector {
            return Err(HartFault::TrapVectorFetch { vector });
        }

        self.pc = trap::deliver_exception(&mut self.csr, exception, tval, pc);
        Ok(())
    }

    /// Map an executor error onto the privileged spec's causes.
    fn deliver_executor_error(
        &mut self,
        e: EmulatorError,
        pc: u32,
        inst_word: u32,
    ) -> Result<(), HartFault> {
        match e {
            EmulatorError::InvalidInstruction { .. } | EmulatorError::UnknownOpcode { .. } => {
                self.deliver_illegal(pc, inst_word, "executor rejected the encoding");
            }
            EmulatorError::InvalidMemoryAccess { address, kind, .. } => {
                let exception = match kind {
                    MemoryAccessKind::Read => Exception::LoadAccessFault,
                    MemoryAccessKind::Write => Exception::StoreAccessFault,
                    MemoryAccessKind::InstructionFetch => Exception::InstructionAccessFault,
                };
                self.pc = trap::deliver_exception(&mut self.csr, exception, address, pc);
            }
            EmulatorError::UnalignedAccess { address, .. } => {
                if self.allow_unaligned {
                    log::warn!(
                        "mach: the bus reported an unaligned access at {address:#010x} while the \
                         hart is configured to permit them — check the machine's bus setup"
                    );
                }
                // `MemoryError::Unaligned` carries no direction, so the
                // opcode is what tells a store from a load.
                let exception = if is_store_class(inst_word) {
                    Exception::StoreAddressMisaligned
                } else {
                    Exception::LoadAddressMisaligned
                };
                self.pc = trap::deliver_exception(&mut self.csr, exception, address, pc);
            }
            EmulatorError::Watchpoint { address, slot, .. } => {
                // The bus reports the watchpoint *instead of* performing the
                // access, so nothing was written and `mepc` names the store
                // itself — `mcontrol` action 0 semantics (discovery §4d).
                self.triggers.set_hit(usize::from(slot));
                self.pc =
                    trap::deliver_exception(&mut self.csr, Exception::Breakpoint, address, pc);
            }
            other => {
                log::error!("mach: no architectural mapping for executor error: {other}");
                return Err(HartFault::UnmappedExecutorError { pc });
            }
        }
        Ok(())
    }
}

/// Build one block's slots by walking forward from `pc`.
///
/// Appends nothing when the very first instruction cannot be cached, which is
/// how "this address is not cacheable" is said; the caller then single-steps
/// it, exactly as it always did.
///
/// The walk fetches through the bus, which is why the whole cached path is
/// gated on [`Bus::fetch_is_pure`]: these fetches happen before the guest
/// reaches the instructions and must charge nothing and trap nothing. A fetch
/// that *fails* simply ends the block — the address is left uncached and the
/// single-stepping path takes the fault, at the right moment, with the right
/// `pc`.
///
/// A block may cross a page or a region boundary. It does not need to be
/// stopped at one: [`BlockCache::invalidate_range`] compares byte spans, so a
/// block that reaches into a window an emulator-side writer touched is
/// dropped whether it started in that window or not.
fn decode_block<B: Bus>(bus: &mut B, pc: u32, out: &mut Vec<RvSlot<B>>) {
    let mut at = pc;
    for _ in 0..MAX_BLOCK_SLOTS {
        let Ok(word) = bus.fetch_instruction(at) else {
            return;
        };
        match block::classify::<B>(word) {
            Class::Body(slot) => {
                let width = u32::from(slot.width);
                out.push(slot);
                match at.checked_add(width) {
                    Some(next) => at = next,
                    // A block that would wrap the address space ends here.
                    None => return,
                }
            }
            Class::Terminator(slot) => {
                out.push(slot);
                return;
            }
            Class::Refused => return,
        }
    }
}

/// True when `inst_word` is a store, for the one place the direction of a
/// misaligned access has to be recovered from the encoding.
///
/// The C6 core performs misaligned accesses in hardware, so this is a
/// completeness path, not a hot one.
#[inline]
fn is_store_class(inst_word: u32) -> bool {
    if (inst_word & 0b11) == 0b11 {
        return (inst_word & 0x7F) as u8 == OPCODE_STORE;
    }
    // RVC (v2.0 §16): `c.sw` is quadrant 0 funct3 0b110, `c.swsp` is
    // quadrant 2 funct3 0b110. No `c.fsw` — this hart has no FPU.
    let quadrant = inst_word & 0b11;
    let funct3 = (inst_word >> 13) & 0b111;
    matches!((quadrant, funct3), (0b00, 0b110) | (0b10, 0b110))
}

#[cfg(test)]
mod tests;
