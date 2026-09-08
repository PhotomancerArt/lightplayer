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

    /// What the accesses of the instruction now retiring did to the bus, as
    /// far as a poll-loop skip is concerned. See [`PollSample`].
    ///
    /// The contract a bus that overrides this must keep:
    ///
    /// - It answers about **this instruction**, not an older one. A bus
    ///   resets it per instruction (`set_issuing` is the natural place).
    /// - It answers [`PollSample::Pure`] only when the instruction's *only*
    ///   MMIO access was one read of a register the peripheral declares
    ///   side-effect free.
    /// - It answers [`PollSample::Inert`] only when it is certain the
    ///   instruction left the machine's state exactly as it found it.
    /// - Everything else is [`PollSample::Impure`], which is always the safe
    ///   answer.
    ///
    /// The default is `Impure`: a bus that makes no claim gets no skip. It
    /// is a constant the optimizer removes.
    #[inline(always)]
    fn take_poll_sample(&mut self) -> PollSample {
        PollSample::Impure
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

/// One side-effect-free MMIO read, as [`PollSample::Pure`] carries it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PureRead {
    pub address: u32,
    /// The value the peripheral returned, zero-extended to a word.
    pub value: u32,
}

/// What one instruction's bus accesses did, for the poll-loop skip.
///
/// The three answers are graded by how much they let the privileged stepper
/// conclude, and a bus that is unsure always has [`Impure`](Self::Impure)
/// available.
///
/// The distinction between `Inert` and `Impure` is the whole reason this is
/// an enum rather than an `Option`. Real poll loops are not three
/// instructions long: the ROM's `uart_serial_tx_one_char` spins on
/// `uart_hal_get_txfifo_count` with the character it is about to send spilled
/// to the stack and reloaded every iteration. Those two accesses touch no
/// peripheral and leave memory holding exactly what it held before, so they
/// cannot move a fixed point — but a bus that could only say "pure read" or
/// "no" would have to say "no", and the skip would never fire on the loop it
/// was written for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PollSample {
    /// The instruction changed nothing the guest can observe: it touched no
    /// MMIO, and any store it performed wrote bytes that were already there.
    ///
    /// A RAM **read** is inert because within the skip's horizon nothing
    /// outside the hart writes memory — a peripheral acts only at a
    /// scheduled event, and the horizon stops at the next one. A RAM
    /// **write** of the value already in place is inert because memory after
    /// it equals memory before it.
    Inert,
    /// Exactly one MMIO read, of a register the peripheral declares
    /// side-effect free (see the ESP crates' `Peripheral::pure_read`).
    Pure(PureRead),
    /// Anything else: an MMIO write, an MMIO read the peripheral does not
    /// declare pure, a store that changed memory, an access that faulted, or
    /// a bus that does not answer the question at all.
    Impure,
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
