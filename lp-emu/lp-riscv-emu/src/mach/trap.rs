//! Trap causes and the delivery / `mret` state dance.
//!
//! *The RISC-V Instruction Set Manual, Volume II: Privileged Architecture*,
//! version 20240411: §3.1.15 (`mcause`) for the codes, §3.1.7 (`mtvec`) for
//! vectoring, and §3.3.2 (`Trap-Return Instructions`) for `mret`.
//!
//! Delivery lives here rather than on the hart so the whole `mstatus` dance
//! is one readable function that a test can drive without a bus.

use super::csr::{CsrFile, MSTATUS_MIE, MSTATUS_MPIE, MSTATUS_MPP};

/// The synchronous exception codes this hart can raise (`mcause` with bit 31
/// clear). Only the codes reachable on a C6 running M-mode-only firmware are
/// modelled; there is no S/U mode, no MMU and no PMP here, so codes 8, 9, 12,
/// 13 and 15 cannot occur.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Exception {
    /// Unreachable on an RVC hart — 2-byte alignment is always legal — but
    /// mapped rather than swallowed, so a bus that ever reports an unaligned
    /// *fetch* produces the architectural answer instead of a silent hang.
    InstructionAddressMisaligned = 0,
    /// The bus refused an instruction fetch.
    InstructionAccessFault = 1,
    /// A bad encoding, an unknown CSR, a write to a read-only CSR, or an
    /// RV32F opcode (this hart has no FPU).
    IllegalInstruction = 2,
    /// `ebreak` / `c.ebreak`, or a trigger that fired.
    Breakpoint = 3,
    LoadAddressMisaligned = 4,
    LoadAccessFault = 5,
    StoreAddressMisaligned = 6,
    StoreAccessFault = 7,
    /// `ecall` from M-mode.
    MachineEnvCall = 11,
}

impl Exception {
    #[inline]
    #[must_use]
    pub const fn code(self) -> u32 {
        self as u32
    }
}

/// `mcause` bit 31: the trap was an interrupt, not an exception
/// (spec §3.1.15; `riscv-0.15.0`'s `mcause` reads it as `is_interrupt`,
/// discovery §1g).
pub const MCAUSE_INTERRUPT: u32 = 1 << 31;

/// Enter a trap handler.
///
/// `cause` is the full `mcause` value (interrupt bit included), `tval` goes
/// to `mtval`, `epc` to `mepc`, and `vectored_slot` is `Some(n)` only for an
/// interrupt taken while `mtvec.MODE == 1`. Returns the new `pc`.
///
/// The `mstatus` dance is the spec's: `MPIE ← MIE`, `MIE ← 0`, `MPP ← 3`
/// (the only privilege mode here). `riscv::interrupt::nested` saves and
/// restores exactly `mstatus.{MPIE,MPP}` and `mepc` around a re-enable
/// (discovery §2f), so those three must survive a nested trap — they do,
/// because software owns them between the two calls.
#[inline]
pub fn deliver(
    csr: &mut CsrFile,
    cause: u32,
    tval: u32,
    epc: u32,
    vectored_slot: Option<u32>,
) -> u32 {
    csr.mepc = epc;
    csr.mcause = cause;
    csr.mtval = tval;

    let mie = csr.mstatus & MSTATUS_MIE;
    // MPIE takes MIE's old value; MIE clears; MPP is pinned at 3.
    let mpie = if mie != 0 { MSTATUS_MPIE } else { 0 };
    csr.mstatus = MSTATUS_MPP | mpie;

    let base = csr.mtvec_base();
    match vectored_slot {
        Some(n) if csr.mtvec_vectored() => base.wrapping_add(4u32.wrapping_mul(n)),
        // Exceptions always go to `base`, and so do interrupts in Direct
        // mode (spec §3.1.7, Table 3.5).
        _ => base,
    }
}

/// Deliver a synchronous exception. `epc` is the faulting instruction, per
/// spec §3.1.14 ("the virtual address of the instruction that was
/// interrupted or that encountered the exception").
#[inline]
pub fn deliver_exception(csr: &mut CsrFile, exception: Exception, tval: u32, epc: u32) -> u32 {
    deliver(csr, exception.code(), tval, epc, None)
}

/// Deliver CPU interrupt `n`. `epc` is the *next* instruction — nothing has
/// been skipped — and `mtval` is 0 (spec §3.1.16: `mtval` is only written
/// for exceptions that define a value).
#[inline]
pub fn deliver_interrupt(csr: &mut CsrFile, n: u8, epc: u32) -> u32 {
    deliver(
        csr,
        MCAUSE_INTERRUPT | u32::from(n),
        0,
        epc,
        Some(u32::from(n)),
    )
}

/// `mret` (spec §3.3.2): `pc ← mepc`, `MIE ← MPIE`, `MPIE ← 1`, and
/// `MPP ← 3` — the least-privileged supported mode, which on an M-only hart
/// is M. Returns the new `pc`.
#[inline]
pub fn mret(csr: &mut CsrFile) -> u32 {
    let mpie = csr.mstatus & MSTATUS_MPIE;
    let mie = if mpie != 0 { MSTATUS_MIE } else { 0 };
    csr.mstatus = MSTATUS_MPP | MSTATUS_MPIE | mie;
    csr.mepc
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mach::csr::MSTATUS_BOOT;

    #[test]
    fn delivery_then_mret_round_trips_mie() {
        let mut csr = CsrFile::new();
        csr.mstatus = MSTATUS_BOOT;
        csr.mtvec = 0x4080_0001;

        let pc = deliver_exception(&mut csr, Exception::MachineEnvCall, 0, 0x4200_0100);
        assert_eq!(pc, 0x4080_0000, "exceptions vector to base, never base+4n");
        assert_eq!(csr.mcause, 11);
        assert_eq!(csr.mepc, 0x4200_0100);
        assert_eq!(csr.mstatus, MSTATUS_MPP | MSTATUS_MPIE, "MIE cleared");

        assert_eq!(mret(&mut csr), 0x4200_0100);
        assert_eq!(csr.mstatus, MSTATUS_BOOT, "MIE restored, MPIE set to 1");
    }

    #[test]
    fn a_direct_mtvec_sends_interrupts_to_base_too() {
        let mut csr = CsrFile::new();
        csr.mstatus = MSTATUS_BOOT;
        csr.mtvec = 0x4080_0000; // mode 0
        assert_eq!(deliver_interrupt(&mut csr, 17, 0x100), 0x4080_0000);
        assert_eq!(csr.mcause, MCAUSE_INTERRUPT | 17);
        assert_eq!(csr.mtval, 0);
    }
}
