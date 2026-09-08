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

/// THROWAWAY (M5 P1) — never merge.
#[cfg(feature = "block-profile")]
pub mod blockprof;
#[cfg(feature = "block-profile")]
extern crate alloc;
pub mod csr;
pub mod trap;
pub mod trigger;

use core::marker::PhantomData;

use lp_emu_core::{Bus, CycleModel, InstClass, MemoryAccessKind, MemoryError};

use crate::emu::{EmulatorError, FpRegs, LoggingDisabled, decode_execute};
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
    /// THROWAWAY (M5 P1) — never merge.
    #[cfg(feature = "block-profile")]
    pub prof: alloc::boxed::Box<blockprof::BlockProf>,
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
            #[cfg(feature = "block-profile")]
            prof: alloc::boxed::Box::new(blockprof::BlockProf::new()),
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
            #[cfg(feature = "block-profile")]
            prof: alloc::boxed::Box::new(blockprof::BlockProf::new()),
            _bus: PhantomData,
        }
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

    #[inline]
    pub fn set_cycle_model(&mut self, model: CycleModel) {
        self.cycle_model = model;
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
        // THROWAWAY (M5 P1) — never merge.
        #[cfg(feature = "block-profile")]
        self.prof.restart(true);

        // The deadline as an absolute cycle: one compare per instruction
        // instead of a subtract and a compare. `saturating_add` keeps a
        // `u64::MAX` budget meaning "never", as the subtraction form did.
        let end = self.cycle_count.saturating_add(budget);
        loop {
            if self.cycle_count >= end {
                return SliceEnd::BudgetExhausted;
            }

            let pc = self.pc;
            // The bus's trace and spin detector are only worth having if the
            // pc and the cycle on each line are this instruction's.
            bus.set_issuing(pc, self.cycle_count);
            let inst_word = match bus.fetch_instruction(pc) {
                Ok(word) => word,
                Err(e) => match self.deliver_fetch_error(e, pc) {
                    Ok(()) => continue,
                    Err(fault) => return SliceEnd::Fault(fault),
                },
            };

            match self.step(bus, pc, inst_word) {
                StepOutcome::Continue => {}
                StepOutcome::End(end) => return end,
            }
        }
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
        // THROWAWAY (M5 P1) — never merge.
        #[cfg(feature = "block-profile")]
        {
            use blockprof::Term;
            let (term, mem) = match result.class {
                InstClass::Store => {
                    self.prof.stores += 1;
                    (Term::Store, true)
                }
                InstClass::Load => {
                    self.prof.loads += 1;
                    (Term::None, true)
                }
                InstClass::Atomic => {
                    self.prof.amo += 1;
                    (Term::Both, true)
                }
                InstClass::Fence => {
                    self.prof.fence += 1;
                    (Term::Both, false)
                }
                InstClass::System => {
                    self.prof.system += 1;
                    (Term::Both, false)
                }
                InstClass::BranchTaken
                | InstClass::BranchNotTaken
                | InstClass::JalCall
                | InstClass::JalTail
                | InstClass::JalrCall
                | InstClass::JalrReturn
                | InstClass::JalrIndirect => {
                    self.prof.control += 1;
                    (Term::Both, false)
                }
                _ => (Term::None, false),
            };
            self.prof.retire(pc, term, mem);
        }
        self.pc = result
            .new_pc
            .unwrap_or(pc.wrapping_add(u32::from(result.inst_size)));

        // (c) an MMIO store may have changed interrupt state, and it may
        // have changed something only the machine can act on.
        if matches!(result.class, InstClass::Store | InstClass::Atomic) {
            if bus.take_sideband() {
                self.resample_external(bus);
            }
            if bus.take_yield() {
                return StepOutcome::End(SliceEnd::BusYield);
            }
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
                    #[cfg(feature = "block-profile")]
                    {
                        self.prof.system += 1;
                        self.prof.retire(pc, blockprof::Term::Both, false);
                    }
                    self.pc = trap::mret(&mut self.csr);
                    // (b) `mret` restores MIE.
                    self.poll_interrupts();
                    StepOutcome::Continue
                }
                FUNCT12_WFI => {
                    self.charge(InstClass::System);
                    self.instruction_count += 1;
                    #[cfg(feature = "block-profile")]
                    {
                        self.prof.system += 1;
                        self.prof.retire(pc, blockprof::Term::Both, false);
                    }
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
        #[cfg(feature = "block-profile")]
        {
            self.prof.system += 1;
            self.prof.retire(pc, blockprof::Term::Both, false);
        }
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
