//! `Bus`: what an instruction stream reads and writes.
//!
//! [`crate::Memory`] is the flat user-mode implementation (code + optional
//! shared + RAM regions, fixed bases); a SoC bus with MMIO-mapped
//! peripherals is another. Reads take `&mut self` because MMIO reads can
//! have side effects (FIFO pops, clear-on-read registers) — `Memory`'s own
//! reads happen to be pure, but the trait can't assume that of every
//! implementor. Errors carry no PC: the owner of the PC (the emulator, or
//! the privileged hart) attaches it at its error boundary, same as
//! [`crate::MemoryError`] does today.

use crate::memory::MemoryError;

/// What an instruction stream reads and writes.
///
/// Method names and return types mirror `Memory`'s inherent methods exactly,
/// so `impl Bus for Memory` is a mechanical forwarding impl and executor
/// bodies need no change beyond the parameter type.
pub trait Bus {
    fn fetch_instruction(&mut self, address: u32) -> Result<u32, MemoryError>;
    fn read_word(&mut self, address: u32) -> Result<i32, MemoryError>;
    fn read_halfword(&mut self, address: u32) -> Result<i16, MemoryError>;
    fn read_byte(&mut self, address: u32) -> Result<i8, MemoryError>;
    fn read_u8(&mut self, address: u32) -> Result<u8, MemoryError>;
    fn write_word(&mut self, address: u32, value: i32) -> Result<(), MemoryError>;
    fn write_halfword(&mut self, address: u32, value: i16) -> Result<(), MemoryError>;
    fn write_byte(&mut self, address: u32, value: i8) -> Result<(), MemoryError>;

    /// A hardware watchpoint slot (RISC-V trigger `mcontrol`, Xtensa
    /// `DBREAK`). The privileged layer mirrors its trigger CSRs here; a bus
    /// that honours it returns [`MemoryError::Watchpoint`] *instead of*
    /// performing the matching access. Default: ignored (user-mode
    /// `Memory`, which has no privileged state to trap from).
    #[inline(always)]
    fn set_watchpoint(&mut self, _slot: usize, _wp: Option<Watchpoint>) {}

    /// Tell the bus which instruction is about to issue accesses, and at
    /// what cycle.
    ///
    /// A bus log is worth having because it says *who* and *when*: `cyc=41288
    /// pc=0x42009a1c R4 TIMG0+0x068 rtccalicfg` answers "which status bit is
    /// it spinning on" in one line. Both halves have to arrive per
    /// instruction to be that line — a machine that set them once per
    /// scheduler slice would stamp a million accesses with the same value,
    /// and the unmapped-site dedup (keyed on `(pc, address)`) would collapse
    /// unrelated sites onto one.
    ///
    /// `cycle` is the count *before* this instruction is charged, which is
    /// the only reading that composes: two accesses one instruction apart
    /// differ by exactly that instruction's cost.
    ///
    /// Called before each instruction the privileged stepper executes. The
    /// default is empty and inlines away; only a bus that keeps a trace or a
    /// spin detector implements it.
    #[inline(always)]
    fn set_issuing(&mut self, _pc: u32, _cycle: u64) {}

    /// Side-band after an MMIO-class access: `true` when the bus's
    /// interrupt state may have changed (an MMIO store). Consumed by the
    /// privileged stepper after Store/System-class instructions only; the
    /// default is a constant the optimizer removes.
    #[inline(always)]
    fn take_sideband(&mut self) -> bool {
        false
    }

    /// The CPU interrupt this bus's interrupt matrix asserts *right now* for
    /// the hart that is executing, or `None` for "nothing asserted".
    ///
    /// This is the other half of [`Bus::take_sideband`], and the two are a
    /// pair: when the side-band says an MMIO store may have changed interrupt
    /// state, the privileged stepper **replaces** its pending-interrupt input
    /// with this value and polls, so a store that raises a peripheral line is
    /// delivered before the next instruction retires — and a store that
    /// *lowers* one stops being pending in the same breath.
    ///
    /// Therefore: **a bus that ever returns `true` from `take_sideband` must
    /// implement this method.** A bus that never raises the side-band never
    /// has it called, which is why the default is a constant the optimizer
    /// removes rather than an `unimplemented!()`.
    #[inline(always)]
    fn pending_cpu_interrupt(&self) -> Option<u8> {
        None
    }

    /// Side-band after a Load-class instruction: the **pure** MMIO read it
    /// performed, if that is all it did.
    ///
    /// A pure read is one the peripheral declares side-effect free *and*
    /// whose value can change only through a write or a scheduled event —
    /// never as a function of the current cycle. Reading one twice with
    /// nothing in between returns the same value, and reading it a thousand
    /// times leaves the machine in the state one read leaves it in. That is
    /// exactly the property a poll-loop skip needs, which is why it is the
    /// bus — the only component that knows what a register *is* — that
    /// declares it.
    ///
    /// The contract, which a bus that overrides this must keep:
    ///
    /// - It reports **this instruction's** access, not an older one. A bus
    ///   clears it per instruction (`set_issuing` is the natural place).
    /// - It is `Some` only when the instruction's *only* MMIO access was one
    ///   pure read. A RAM load, an impure read (a FIFO pop, a clear-on-read
    ///   register), or any MMIO write leaves it `None`.
    ///
    /// The privileged stepper consumes it after Load-class instructions and
    /// treats `None` as "no evidence", which is always safe: the detector
    /// resets and nothing is skipped. The default is a constant the
    /// optimizer removes.
    #[inline(always)]
    fn take_pure_read(&mut self) -> Option<PureRead> {
        None
    }

    /// The stepper skipped `iterations` whole iterations of a pure poll loop
    /// whose read was of `address`, issued at `pc`.
    ///
    /// Bring-up only: a bus that keeps a trace writes one line per skip so a
    /// reader can see where guest time went. The default is empty and
    /// inlines away.
    #[inline(always)]
    fn note_poll_skip(&mut self, _pc: u32, _address: u32, _iterations: u64) {}
}

/// One side-effect-free MMIO read, as [`Bus::take_pure_read`] reports it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PureRead {
    pub address: u32,
    /// The value the peripheral returned, zero-extended to a word.
    pub value: u32,
}

/// A hardware watchpoint slot's configuration.
///
/// Shaped after the RISC-V `tdata1`/`tdata2` trigger pair (`mcontrol`); the
/// same shape covers Xtensa's `DBREAK` registers, so the privileged layer
/// for either architecture can drive it without a second type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Watchpoint {
    /// NAPOT-encoded compare value as written to `tdata2` (bit pattern), or
    /// an exact address when `napot` is `false`.
    pub address: u32,
    pub napot: bool,
    pub on_store: bool,
    pub on_load: bool,
    pub on_execute: bool,
}
