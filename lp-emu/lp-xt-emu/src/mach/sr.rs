//! The special- and user-register file: storage for everything `rsr`/`wsr`/
//! `xsr`/`rur`/`wur` can name that is not already `Cpu` state.
//!
//! Plain storage. Read and write *behaviour* — the derived values (`PS`,
//! `CCOUNT`, `INTERRUPT`, `WINDOWBASE`), the write side effects (a DBREAK
//! write arming the bus, a CCOMPARE write clearing its timer, a PS write
//! polling) — lives on the hart in `mod.rs`, because it needs the hart. The
//! RV32 twin is `lp-riscv-emu/src/mach/csr.rs`.
//!
//! Register numbers are `lp-xt-inst`'s (assembler-derived). Field layouts and
//! reset values are from the Xtensa ISA Reference Manual, chapter 5.

/// PS field layout (ISA RM §4.4.1.3, Figure 4-8), confirmed by two pieces of
/// guest code that run on this hart: `xtensa-lx-rt-0.22.0/src/exception/
/// asm.rs:87-91` and `esp-rtos-0.3.0/src/task/xtensa.rs:52-56`; the OWB
/// position by `asm.rs`'s `_AllocAException` (`extui a3, a2, 8, 4`).
pub const PS_INTLEVEL_MASK: u32 = 0x0000_000F; // bits [3:0]
pub const PS_EXCM: u32 = 0x0000_0010; // bit 4
pub const PS_UM: u32 = 0x0000_0020; // bit 5
pub const PS_RING_SHIFT: u32 = 6; // bits [7:6]
pub const PS_RING_MASK: u32 = 0x3 << PS_RING_SHIFT;
pub const PS_OWB_SHIFT: u32 = 8; // bits [11:8]
pub const PS_OWB_MASK: u32 = 0xF << PS_OWB_SHIFT;
pub const PS_CALLINC_SHIFT: u32 = 16; // bits [17:16]
pub const PS_CALLINC_MASK: u32 = 0x3 << PS_CALLINC_SHIFT;
pub const PS_WOE: u32 = 0x0004_0000; // bit 18

/// The bits a `wsr.ps` keeps (RM Table 5-139: `PS <- 0^13 || AR[t]18..16 ||
/// 0^4 || AR[t]11..0`).
pub const PS_WRITE_MASK: u32 = 0x0007_0FFF;

/// The architectural reset value of PS with the Interrupt Option configured
/// (RM Table 5-139): `INTLEVEL = 15`, `EXCM = 1`, everything else 0.
pub const PS_RESET: u32 = 0x0000_001F;

/// `PS = WOE | UM | CALLINC(2)`: INTLEVEL 0, EXCM 0 — what the real ROM and
/// second-stage bootloader leave the core in when they jump to the app, and
/// therefore what a machine seeds for a **direct load** (the Xtensa twin of
/// the C6's `mstatus = 0x1888`). The reason is concrete: `xtensa-lx-rt`'s
/// `Reset:` (`src/lib.rs:126-160`) begins with `entry a1, 0x10` and uses
/// `call4`/`callx4` at once, which needs `PS.WOE = 1` and `PS.EXCM = 0`. A
/// hart left at [`PS_RESET`] raises an illegal instruction on the app's first
/// instruction — the twin of "the firmware idles forever in `wfi`".
///
/// `CALLINC = 2` because the bootloader reaches the entry point through a C
/// indirect call — a `callx8` under the windowed ABI — so the app's `entry`
/// rotates by two groups on silicon. The M1 planning note pinned
/// `0x0004_0020` (CALLINC 0), under which that `entry` rotates by nothing;
/// harmless for the runtime, but not what the bootloader leaves. The
/// machine (M3) owns the final word; this constant records the reasoning.
pub const PS_BOOT: u32 = PS_WOE | PS_UM | (2 << PS_CALLINC_SHIFT);

/// Number of exception levels with their own EPC/EPS/EXCSAVE bank, indexed
/// 1..=7 (`EPC1` is the general-exception slot; `EPS1` does not exist).
pub const NUM_LEVELS: usize = 8;

/// `EXCCAUSE` is 6 bits (RM Table 5-153).
pub const EXCCAUSE_MASK: u32 = 0x3F;

/// The register file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SrFile {
    // --- loops (SR 0..2) ---
    pub lbeg: u32,
    pub lend: u32,
    pub lcount: u32,
    // --- unprivileged scratch ---
    /// `LITBASE` (SR 5). Stored only: this hart's `l32r` is the executors'
    /// PC-relative one; the extended-L32R enable is never honoured, and the
    /// firmware never sets it (M0 report).
    pub litbase: u32,
    pub scompare1: u32,
    // --- exception bank: index = level; slot 0 unused ---
    pub epc: [u32; NUM_LEVELS],
    pub eps: [u32; NUM_LEVELS],
    pub excsave: [u32; NUM_LEVELS],
    pub depc: u32,
    pub exccause: u32,
    pub excvaddr: u32,
    pub vecbase: u32,
    pub debugcause: u32,
    /// `ICOUNT`/`ICOUNTLEVEL` (SR 236/237). Stored, not counted: the
    /// instruction-count debug exception is out of this phase's scope and
    /// nothing in the firmware arms it.
    pub icount: u32,
    pub icountlevel: u32,
    /// `MEMCTL` (SR 97), `ATOMCTL` (SR 99), `DDR` (SR 104): accept and
    /// remember.
    pub memctl: u32,
    pub atomctl: u32,
    pub ddr: u32,
    pub misc: [u32; 4],
    // --- user registers ---
    pub threadptr: u32,
    /// `EXPSTATE` (UR 230, LX6 only): accept and remember.
    pub expstate: u32,
    /// `F64R_LO`/`F64R_HI`/`F64S` (UR 234..236, LX6 only): accept and
    /// remember. The `f64*` *instructions* are deliberately not decoded
    /// (ruling DD16); the registers are, because `save_context` round-trips
    /// them.
    pub f64r_lo: u32,
    pub f64r_hi: u32,
    pub f64s: u32,
    // --- region protection: accept and remember ---
    /// The 4-bit attribute per 512 MiB region, instruction and data TLB,
    /// written by `witlb`/`wdtlb` and read back by `ritlb1`/`rdtlb1` (RM
    /// §4.6.3.2). Nothing else looks at them; the classic's cache/protection
    /// behaviour is the machine's (M3).
    pub itlb_attr: [u8; 8],
    pub dtlb_attr: [u8; 8],
}

impl SrFile {
    /// Reset. Every register the RM leaves undefined at reset is zero here
    /// — a choice the architecture permits, stated so nobody mistakes it for
    /// a measured value. `IBREAKENABLE` (the one the RM does define, as 0)
    /// lives in the break unit.
    #[must_use]
    pub const fn new(vecbase: u32) -> Self {
        Self {
            lbeg: 0,
            lend: 0,
            lcount: 0,
            litbase: 0,
            scompare1: 0,
            epc: [0; NUM_LEVELS],
            eps: [0; NUM_LEVELS],
            excsave: [0; NUM_LEVELS],
            depc: 0,
            exccause: 0,
            excvaddr: 0,
            vecbase,
            debugcause: 0,
            icount: 0,
            icountlevel: 0,
            memctl: 0,
            atomctl: 0,
            ddr: 0,
            misc: [0; 4],
            threadptr: 0,
            expstate: 0,
            f64r_lo: 0,
            f64r_hi: 0,
            f64s: 0,
            itlb_attr: [0; 8],
            dtlb_attr: [0; 8],
        }
    }
}
