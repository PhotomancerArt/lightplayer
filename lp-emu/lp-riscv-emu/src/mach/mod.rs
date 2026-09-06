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
//!   [`Bus::take_sideband`] `== true`, which is how an MMIO store that
//!   raises a peripheral interrupt gets noticed before the next
//!   instruction. (Atomics are included: an AMO is a store.) On a RAM-only
//!   `Bus` this is an inlined constant `false` the optimizer deletes;
//!   `Memory` never overrides it.
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

pub mod csr;
pub mod trap;
pub mod trigger;
