//! Machine-mode CSR numbers, their semantics class, and the raw CSR file.
//!
//! Every constant below is cited: `spec` means *The RISC-V Instruction Set
//! Manual, Volume II: Privileged Architecture*, version 20240411, and
//! `discovery §N` means the M3 planning discovery
//! `m3/discovery-esp-hal-csr-irq.md`, which read the esp-hal 1.1.1 /
//! esp-riscv-rt 0.14 / riscv-rt 0.16 sources this hart has to satisfy.
//!
//! The one rule worth stating twice: **an unknown CSR is an illegal
//! instruction, not a silent zero** ([`CsrClass::Illegal`]). Silent-zero CSRs
//! are how a stack guard gets lost without anybody noticing.

/// What a CSR number *means* here — the vocabulary the phase report uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CsrClass {
    /// Real architectural state: written by software, read back verbatim
    /// modulo the register's WARL rules, and consulted by the hart.
    State,
    /// Not stored: read is computed from hart state (cycle/instruction
    /// counters, the pending-interrupt bitmap). Writes are discarded.
    Derived,
    /// Stored and read back, but nothing in the hart consults it. Cheaper
    /// than an illegal-instruction trap that would be wrong the day esp-hal
    /// starts writing it (`mhcr`, `mtvt`, the dedicated-GPIO CSRs).
    Scratch,
    /// The trigger unit ([`super::trigger`]): state with a bus side effect.
    Trigger,
    /// Not implemented — reads and writes raise illegal instruction.
    Illegal,
}

// --- machine information (spec §3.1.1-3.1.4; read-only) ---------------------

/// `mvendorid` — 0 means "non-commercial implementation" (spec §3.1.1).
pub const MVENDORID: u16 = 0xF11;
/// `marchid` — 0 means "not assigned" (spec §3.1.2).
pub const MARCHID: u16 = 0xF12;
/// `mimpid` — 0 means "not implemented" (spec §3.1.3).
pub const MIMPID: u16 = 0xF13;
/// `mhartid` — read by `_start` (discovery §1a, `riscv-rt/src/asm.rs:74`).
pub const MHARTID: u16 = 0xF14;

// --- machine trap setup (spec §3.1.6-3.1.9) --------------------------------

/// `mstatus` — only `MIE`/`MPIE`/`MPP` are modelled; see [`MSTATUS_MASK`].
pub const MSTATUS: u16 = 0x300;
/// `misa` — see [`MISA`].
pub const MISA: u16 = 0x301;
/// `mie` — a plain 32-bit mask, bit `n` = CPU interrupt `n` (discovery §1d:
/// `_setup_interrupts` writes `0xFFFF_FFFF` wholesale and nothing reads it
/// back, so Espressif's per-CPU-interrupt bit model is the only one the
/// firmware can be said to rely on).
pub const MIE: u16 = 0x304;
/// `mtvec` — written `_vector_table | 1` = `0x4080_0001` (discovery §1d) and
/// read back with the mode bits intact by `enable_direct` (discovery §2h).
pub const MTVEC: u16 = 0x305;
/// `mtvt` (CLIC vector table base). Never touched on the C6 — the CLIC
/// branch is `#[cfg(interrupt_controller = "clic")]` and the C6 is PLIC
/// (discovery §1h). Scratch, not illegal.
pub const MTVT: u16 = 0x307;

// --- machine trap handling (spec §3.1.14-3.1.17) ---------------------------

/// `mscratch` — a working scratch CSR: `_pre_default_start_trap` parks `t0`
/// in it while it probes the stack pointer (discovery §1f).
pub const MSCRATCH: u16 = 0x340;
/// `mepc` — written by the esp-rtos context switch, consumed by `mret`
/// (discovery §3e), and restored by `riscv::interrupt::nested` (§2f).
pub const MEPC: u16 = 0x341;
/// `mcause` — **writable with arbitrary values**: `_pre_default_start_trap`
/// does `csrw mcause, 14` as a synthetic stack-overflow marker
/// (discovery §1f). 31-bit code plus bit 31 = interrupt (spec §3.1.15).
pub const MCAUSE: u16 = 0x342;
/// `mtval` — the faulting address, or the instruction word for an illegal
/// instruction. `ExceptionHandler` reads it to identify a watchpoint hit
/// (discovery §1g/§4d).
pub const MTVAL: u16 = 0x343;
/// `mip` — never read by the esp stack (discovery §2i). Derived: reads the
/// current pending bitmap, writes are discarded.
pub const MIP: u16 = 0x344;

// --- Espressif / vendor CSRs ------------------------------------------------

/// `mhcr` (branch-predictor control). Only written under
/// `soc_cpu_has_branch_predictor`, which the C6 does not set (discovery §1h).
/// Scratch so a future esp-hal that does enable it is not met with a wrong
/// illegal-instruction trap.
pub const MHCR: u16 = 0x7C1;

/// `mpcer` — the performance counter's event select (`1` = CPU cycles).
/// Scratch: the counter below counts cycles whatever is selected, so the
/// write is remembered and nothing else. Written by the firmware's
/// `cycle_counter::setup()` (`board/esp32c6/cycle_counter.rs`), which the
/// shader-compile harness runs before its first tick — an illegal-
/// instruction trap here was the M3 P6 harness's "PANIC" (esp-hal's
/// `ExceptionHandler`) before the counter was measured.
pub const MPCER: u16 = 0x7E0;
/// `mpcmr` — the performance counter's mode / enable (`1` = enabled).
/// Scratch, same reasoning as [`MPCER`]: the count runs from reset here
/// (*modeled*; on silicon it runs from the enable), and every reader takes
/// `wrapping_sub` deltas, so the offset is never observed.
pub const MPCMR: u16 = 0x7E1;
/// `pccr` / `mpccr` (machine) — Espressif's performance counter, the
/// cycle-ish CSR the C6 firmware reads: the RNG entropy spacing loop
/// (discovery §5) and the harness's per-tick `slice_cycles`. Reads
/// `cycle_count` truncated to 32 bits.
pub const PCCR_MACHINE: u16 = 0x7E2;
/// `pccr` (user) — compiled but statically unreachable on the C6
/// (`tee_enabled()` is a `const false`, discovery §5). Same value as
/// [`PCCR_MACHINE`].
pub const PCCR_USER: u16 = 0x802;

/// Dedicated-GPIO CSRs (`CSR_GPIO_OEN_USER`, `CSR_GPIO_IN_USER`,
/// `CSR_GPIO_OUT_USER` and their two unnamed neighbours), discovery §1h:
/// `esp-hal/src/gpio/dedicated.rs:2005-2039`. Scratch — this hart has no
/// GPIO; the SoC layer (P5) owns pins.
pub const GPIO_CSR_FIRST: u16 = 0x800;
/// Last dedicated-GPIO CSR. `0x802` is carved out of this range as
/// [`PCCR_USER`].
pub const GPIO_CSR_LAST: u16 = 0x805;

// --- trigger unit (spec, Debug Support 1.0, §5.2) --------------------------

/// `tselect` — which of the four triggers the `tdata*` window addresses.
pub const TSELECT: u16 = 0x7A0;
/// `tdata1` — `mcontrol` for the selected trigger.
pub const TDATA1: u16 = 0x7A1;
/// `tdata2` — the compare value for the selected trigger.
pub const TDATA2: u16 = 0x7A2;
/// `tcontrol` — `mte`/`mpte`; `mte == 0` means triggers do not fire in
/// M-mode. esp-hal sets `mte` as part of every `set_watchpoint`
/// (discovery §4a/§4b).
pub const TCONTROL: u16 = 0x7A5;

// --- counters (spec §3.1.11, §12) ------------------------------------------

/// `mcycle` / `cycle` and their `h` halves. Never read on the C6
/// (discovery §5) but cheap and unambiguous, so they are real.
pub const MCYCLE: u16 = 0xB00;
pub const MINSTRET: u16 = 0xB02;
pub const MCYCLEH: u16 = 0xB80;
pub const MINSTRETH: u16 = 0xB82;
pub const CYCLE: u16 = 0xC00;
pub const INSTRET: u16 = 0xC02;
pub const CYCLEH: u16 = 0xC80;
pub const INSTRETH: u16 = 0xC82;

// --- mstatus fields (spec §3.1.6) ------------------------------------------

/// `mstatus.MIE`, bit 3 — the global machine interrupt enable.
pub const MSTATUS_MIE: u32 = 1 << 3;
/// `mstatus.MPIE`, bit 7 — MIE's value before the trap.
pub const MSTATUS_MPIE: u32 = 1 << 7;
/// `mstatus.MPP`, bits 12:11. M-mode-only hart, so it is WARL-pinned to 3.
pub const MSTATUS_MPP: u32 = 0b11 << 11;

/// The bits a `mstatus` write may change. Everything else reads 0: this hart
/// has no S/U mode (so no SIE/SPIE/SPP/MPRV/SUM/MXR/TVM/TW/TSR), no FPU (so
/// no FS), and no vector or extension state (no VS/XS/SD).
pub const MSTATUS_MASK: u32 = MSTATUS_MIE | MSTATUS_MPIE;

/// Reset `mstatus`: `MPP = 3` (the only legal value), `MPIE = 0`,
/// **`MIE = 0`** — the spec's reset state.
///
/// Nothing in the esp-hal stack ever sets `MIE` (discovery §1h), so the
/// *machine* (P4) must seed `mstatus = 0x1888` through
/// [`super::MachineHart::set_csr_raw`] before jumping to `_start`, exactly as
/// the real ROM / 2nd-stage bootloader leaves the core. A hart left at this
/// reset value will never take an interrupt.
pub const MSTATUS_RESET: u32 = MSTATUS_MPP;

/// The value the machine seeds before entering the app: `MPP = 3`,
/// `MPIE = 1`, `MIE = 1` (discovery §1h, "your emulator must therefore boot
/// the core with `mstatus.MIE = 1` already set").
pub const MSTATUS_BOOT: u32 = MSTATUS_MPP | MSTATUS_MPIE | MSTATUS_MIE;

/// The value [`MISA`] reads: `MXL = 1` (RV32) plus `A`, `C`, `I`, `M`
/// (spec §3.1.1, `Machine ISA Register`; extension letter `n` is bit
/// `n - 'A'`, so `A` = 1, `C` = 4, `I` = 0x100, `M` = 0x1000).
///
/// **No `U` bit.** M is the only privilege mode this hart implements —
/// `mstatus.MPP` is WARL-pinned to 3 and there is no S/U state anywhere in
/// `mach` — so claiming user mode would be a plausible fiction of exactly
/// the kind the rest of this file refuses. Nothing in the esp-hal stack
/// reads `misa` at all (the M3 discovery found no site), so the bit was
/// unobservable either way; it is cleared because it was untrue, not
/// because something noticed.
pub const MISA_VALUE: u32 = 0x4000_1105;

/// `mtvec.MODE == 1` (Vectored): interrupt `n` vectors to `base + 4*n`
/// (spec §3.1.7). `enable_direct` asserts this mode reads back
/// (discovery §2h).
pub const MTVEC_MODE_VECTORED: u32 = 1;

/// True when writing `csr` is architecturally impossible: bits 11:10 of a
/// CSR address encode read/write permission, and `0b11` means read-only
/// (spec §2.1, "CSR Address Mapping Conventions"). An attempt to write one
/// raises illegal instruction.
#[inline]
#[must_use]
pub const fn is_read_only(csr: u16) -> bool {
    (csr >> 10) & 0b11 == 0b11
}

/// The semantics class of a CSR number — the table the phase report quotes.
#[must_use]
pub const fn class(csr: u16) -> CsrClass {
    match csr {
        MSTATUS | MIE | MTVEC | MSCRATCH | MEPC | MCAUSE | MTVAL => CsrClass::State,

        MISA | MVENDORID | MARCHID | MIMPID | MHARTID | MIP | MCYCLE | MCYCLEH | MINSTRET
        | MINSTRETH | CYCLE | CYCLEH | INSTRET | INSTRETH | PCCR_MACHINE | PCCR_USER => {
            CsrClass::Derived
        }

        TSELECT | TDATA1 | TDATA2 | TCONTROL => CsrClass::Trigger,

        MTVT | MHCR | MPCER | MPCMR => CsrClass::Scratch,
        // `0x802` inside this range is PCCR_USER, matched above.
        GPIO_CSR_FIRST..=GPIO_CSR_LAST => CsrClass::Scratch,

        _ => CsrClass::Illegal,
    }
}

/// The stored half of the machine-mode CSR file.
///
/// Only [`CsrClass::State`] and [`CsrClass::Scratch`] registers live here;
/// derived ones are computed by the hart and the trigger CSRs live in
/// [`super::trigger::TriggerUnit`].
#[derive(Clone, Debug)]
pub struct CsrFile {
    /// `mstatus`, already masked to [`MSTATUS_MASK`] plus a pinned `MPP`.
    pub mstatus: u32,
    pub mie: u32,
    pub mtvec: u32,
    pub mscratch: u32,
    pub mepc: u32,
    pub mcause: u32,
    pub mtval: u32,
    /// `mtvt`, `mhcr`, the five dedicated-GPIO CSRs, `mpcer` and `mpcmr`,
    /// in the order [`CsrFile::scratch_index`] assigns.
    scratch: [u32; SCRATCH_COUNT],
}

/// How many [`CsrClass::Scratch`] CSRs there are.
const SCRATCH_COUNT: usize = 9;

impl Default for CsrFile {
    fn default() -> Self {
        Self::new()
    }
}

impl CsrFile {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            mstatus: MSTATUS_RESET,
            mie: 0,
            mtvec: 0,
            mscratch: 0,
            mepc: 0,
            mcause: 0,
            mtval: 0,
            scratch: [0; SCRATCH_COUNT],
        }
    }

    /// Where a [`CsrClass::Scratch`] CSR lives in [`Self::scratch`].
    #[inline]
    const fn scratch_index(csr: u16) -> Option<usize> {
        match csr {
            MTVT => Some(0),
            MHCR => Some(1),
            0x800 => Some(2),
            0x801 => Some(3),
            // 0x802 is PCCR_USER, not scratch.
            0x803 => Some(4),
            0x804 => Some(5),
            0x805 => Some(6),
            MPCER => Some(7),
            MPCMR => Some(8),
            _ => None,
        }
    }

    #[inline]
    #[must_use]
    pub fn read_scratch(&self, csr: u16) -> Option<u32> {
        Self::scratch_index(csr).map(|i| self.scratch[i])
    }

    #[inline]
    pub fn write_scratch(&mut self, csr: u16, value: u32) -> bool {
        match Self::scratch_index(csr) {
            Some(i) => {
                self.scratch[i] = value;
                true
            }
            None => false,
        }
    }

    /// WARL write to `mstatus`: only [`MSTATUS_MASK`] is writable, and `MPP`
    /// is pinned to 3 because M is the only implemented privilege mode
    /// (spec §3.1.6, "MPP is WARL").
    #[inline]
    pub fn write_mstatus(&mut self, value: u32) {
        self.mstatus = (value & MSTATUS_MASK) | MSTATUS_MPP;
    }

    /// `mstatus.MIE`.
    #[inline]
    #[must_use]
    pub const fn mie_enabled(&self) -> bool {
        self.mstatus & MSTATUS_MIE != 0
    }

    /// The trap vector base: `mtvec` with the two mode bits cleared
    /// (spec §3.1.7).
    #[inline]
    #[must_use]
    pub const fn mtvec_base(&self) -> u32 {
        self.mtvec & !0b11
    }

    /// True when `mtvec.MODE` selects vectored interrupt delivery.
    ///
    /// The field is two bits; only `0` (Direct) and `1` (Vectored) are
    /// defined, and the C6 firmware writes exactly `1` (discovery §1d). The
    /// written value is stored verbatim so `enable_direct`'s read-back
    /// assertion sees what it wrote (§2h); anything other than `1` is
    /// treated as Direct.
    #[inline]
    #[must_use]
    pub const fn mtvec_vectored(&self) -> bool {
        self.mtvec & 0b11 == MTVEC_MODE_VECTORED
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_only_encoding_matches_the_spec_convention() {
        // 0xC00/0xF14 are read-only; 0x300/0x7A1 are read/write.
        assert!(is_read_only(CYCLE));
        assert!(is_read_only(MHARTID));
        assert!(!is_read_only(MSTATUS));
        assert!(!is_read_only(TDATA1));
        // The machine counters at 0xB00 are read/write in the spec.
        assert!(!is_read_only(MCYCLE));
    }

    #[test]
    fn mstatus_write_pins_mpp_and_drops_unmodelled_bits() {
        let mut csr = CsrFile::new();
        csr.write_mstatus(0xFFFF_FFFF);
        assert_eq!(csr.mstatus, MSTATUS_BOOT);
        csr.write_mstatus(0);
        assert_eq!(csr.mstatus, MSTATUS_MPP);
        assert!(!csr.mie_enabled());
    }

    #[test]
    fn misa_says_rv32imac_and_claims_no_user_mode() {
        assert_eq!(MISA_VALUE >> 30, 1, "MXL = 1: XLEN is 32");
        // Extension letter `n` is bit `n - 'A'`.
        for (letter, bit) in [('A', 0), ('C', 2), ('I', 8), ('M', 12)] {
            assert!(
                MISA_VALUE & (1 << bit) != 0,
                "{letter} must be present in misa"
            );
        }
        assert_eq!(
            MISA_VALUE & (1 << 20),
            0,
            "U must be clear: M is the only privilege mode this hart implements"
        );
        // F/D/Q: no FPU on this hart, and the RV32F opcodes are illegal.
        for bit in [3, 5, 16] {
            assert_eq!(MISA_VALUE & (1 << bit), 0, "no float extension");
        }
    }

    #[test]
    fn pccr_user_is_not_shadowed_by_the_gpio_scratch_range() {
        assert_eq!(class(PCCR_USER), CsrClass::Derived);
        assert_eq!(class(0x801), CsrClass::Scratch);
        assert_eq!(class(0x803), CsrClass::Scratch);
        assert!(CsrFile::scratch_index(PCCR_USER).is_none());
    }

    #[test]
    fn the_performance_counter_control_csrs_are_accepted_not_illegal() {
        // `cycle_counter::setup()`: `csrw 0x7E0, 1; csrw 0x7E1, 1`. An
        // illegal-instruction trap here is a guest panic before the first
        // harness tick.
        assert_eq!(class(MPCER), CsrClass::Scratch);
        assert_eq!(class(MPCMR), CsrClass::Scratch);
        assert_eq!(class(PCCR_MACHINE), CsrClass::Derived);
        let mut f = CsrFile::new();
        assert!(f.write_scratch(MPCER, 1));
        assert!(f.write_scratch(MPCMR, 1));
        assert_eq!(f.read_scratch(MPCER), Some(1));
        assert_eq!(f.read_scratch(MPCMR), Some(1));
        assert_eq!(
            f.read_scratch(0x805),
            Some(0),
            "the GPIO slots are untouched"
        );
        // Every scratch slot is reachable by exactly one CSR.
        let mut seen = [false; SCRATCH_COUNT];
        for csr in 0u16..0x1000 {
            if let Some(i) = CsrFile::scratch_index(csr) {
                assert!(!seen[i], "slot {i} claimed twice (csr {csr:#05x})");
                seen[i] = true;
            }
        }
        assert!(seen.iter().all(|s| *s), "every slot has a CSR");
    }
}
