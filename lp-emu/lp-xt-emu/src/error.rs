//! Traps: how a run stops abnormally, mirroring `xt_runner_proto::CrashReport`
//! so dual-run can compare emulator faults against hardware crash reports.

use lp_emu_core::memory::{MemoryAccessKind, MemoryError};

/// Classification of an abnormal stop.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TrapKind {
    /// A hardware-style exception (illegal instruction, bad fetch, bad
    /// load/store). Corresponds to `xt_runner_proto::CrashKind::Exception`.
    Exception,
    /// The instruction budget was exhausted — the payload looped forever.
    /// Corresponds to the device watchdog firing (`CrashKind::Timeout`).
    Timeout,
}

/// A trap raised during execution.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Trap {
    pub kind: TrapKind,
    /// EXCCAUSE-style cause code (0 for timeouts).
    pub cause: u32,
    /// Faulting PC (0 if not applicable / filled in by the run loop).
    pub pc: u32,
    /// Faulting data address for load/store errors (else 0).
    pub vaddr: u32,
}

/// Turn a bus error into the Xtensa trap the user-mode runner has always
/// produced.
///
/// This is the round trip that keeps `Q3` true. The user-mode
/// [`crate::memory::Memory`] raises exactly **two** EXCCAUSE values and both
/// carry the faulting address:
///
/// | `Memory` | `MemoryError` |
/// |---|---|
/// | fetch fault, `cause = EXC_INSTR_FETCH_ERROR (2)`, `pc = vaddr = pc` | `InvalidAccess { address: pc, kind: InstructionFetch }` |
/// | `load_fault(addr)`, `cause = EXC_LOAD_STORE_ERROR (3)`, `pc = 0` | `InvalidAccess { address: addr, kind: Read }` |
/// | `store_fault(addr)`, `cause = EXC_LOAD_STORE_ERROR (3)`, `pc = 0` | `InvalidAccess { address: addr, kind: Write }` |
///
/// so `Trap -> MemoryError -> Trap` is the **identity** on everything user
/// mode can raise. `size` carries no Xtensa meaning — no EXCCAUSE encodes an
/// access width — so it is dropped on the way back, which is why the round
/// trip starts from the `Trap` side and not from an arbitrary `MemoryError`.
///
/// `pc` follows what `Memory` has always done rather than a rule of its own:
/// **0** for a load or a store, filled in by the run loop's error boundary
/// (`emu.rs`'s `if trap.pc == 0 { trap.pc = self.cpu.pc }`), and **the
/// faulting address** for a fetch, which is the pc by construction. A fetch
/// trap therefore reaches the run loop already stamped, exactly as
/// `Memory::fetch`'s own trap did.
///
/// [`MemoryError::Unaligned`] and [`MemoryError::Watchpoint`] are
/// machine-mode-only shapes: `Memory` raises neither, so mapping them changes
/// no user-mode byte. They carry their own causes so the privileged hart
/// ([`crate::mach`]) can tell them apart at its error boundary: an unaligned
/// access is [`EXC_LOAD_STORE_ALIGNMENT`] with the address in `vaddr`, and a
/// watchpoint is the crate-private [`TRAP_CAUSE_WATCHPOINT`] pseudo-cause
/// with the slot in its low bits — not an EXCCAUSE at all, because a DBREAK
/// hit is a *debug* exception (DEBUGCAUSE, level `DEBUGLEVEL`), and only the
/// hart can raise one.
pub(crate) fn trap_from_bus(e: MemoryError) -> Trap {
    let (address, kind) = match e {
        MemoryError::InvalidAccess { address, kind, .. } => (address, kind),
        MemoryError::Unaligned { address, .. } => {
            return Trap {
                kind: TrapKind::Exception,
                cause: EXC_LOAD_STORE_ALIGNMENT,
                pc: 0,
                vaddr: address,
            };
        }
        MemoryError::Watchpoint { address, slot, .. } => {
            return Trap {
                kind: TrapKind::Exception,
                cause: TRAP_CAUSE_WATCHPOINT | u32::from(slot),
                pc: 0,
                vaddr: address,
            };
        }
    };
    match kind {
        MemoryAccessKind::InstructionFetch => Trap {
            kind: TrapKind::Exception,
            cause: crate::memory::EXC_INSTR_FETCH_ERROR,
            pc: address,
            vaddr: address,
        },
        MemoryAccessKind::Read | MemoryAccessKind::Write => Trap {
            kind: TrapKind::Exception,
            cause: crate::memory::EXC_LOAD_STORE_ERROR,
            pc: 0,
            vaddr: address,
        },
    }
}

/// So the sixteen `self.mem.*` sites in `executor/` keep their spelling: they
/// are `Result<_, MemoryError>` now and `?` converts.
impl From<MemoryError> for Trap {
    #[inline]
    fn from(e: MemoryError) -> Trap {
        trap_from_bus(e)
    }
}

/// EXCCAUSE for an illegal / unsupported instruction (`IllegalInstructionCause`).
pub const EXC_ILLEGAL_INSTRUCTION: u32 = 0;
/// EXCCAUSE for a `SYSCALL` with no host handler installed (`SyscallCause`).
pub const EXC_SYSCALL: u32 = 1;
/// EXCCAUSE for a load or store whose address the access width cannot use
/// (`LoadStoreAlignmentCause`, ISA RM §4.4.3, Table 4-68). Machine-mode
/// only: user-mode `Memory` never raises [`MemoryError::Unaligned`].
pub const EXC_LOAD_STORE_ALIGNMENT: u32 = 9;
/// The crate-private pseudo-cause a bus watchpoint travels under between the
/// executors and the machine-mode hart: `TRAP_CAUSE_WATCHPOINT | slot`. It is
/// **not** an EXCCAUSE value — it sits far above the 6-bit cause space so it
/// can never be mistaken for one — and it never reaches user mode, whose
/// `Memory` has no watchpoint slots.
pub(crate) const TRAP_CAUSE_WATCHPOINT: u32 = 0x1_0000;
/// EXCCAUSE for an integer divide (or remainder) by zero
/// (`IntegerDivideByZeroCause`). Hardware raises this from `quos`/`quou`/
/// `rems`/`remu` with a zero divisor; the P3 dual-run corpus asserts the
/// emulator and the ESP32-S3 agree on this exact cause code.
pub const EXC_INTEGER_DIVIDE_BY_ZERO: u32 = 6;
/// EXCCAUSE for a coprocessor-0 (FPU) instruction executed with `CPENABLE`
/// bit 0 clear (`Coprocessor0Disabled`).
///
/// Modeled rather than assumed-away: firmware must arm `CPENABLE` before any
/// compiled float code runs, and an always-on emulator would let that omission
/// reach a board. **Not yet confirmed against silicon:** the M6 P1 probe found
/// the S3 arrives with the coprocessor *already armed* under the esp-hal boot
/// chain, so its deliberately-unarmed probe returned a value instead of
/// faulting and the cause code stayed unmeasured. 32 is the architectural
/// value; a P6 vector that first clears `CPENABLE` would confirm it.
pub const EXC_COPROCESSOR0_DISABLED: u32 = 32;
