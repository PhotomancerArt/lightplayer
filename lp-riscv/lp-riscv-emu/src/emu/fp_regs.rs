//! RV32F architectural state: the `f0`–`f31` register file and `fcsr`.
//!
//! Implemented from *The RISC-V Instruction Set Manual, Volume I: Unprivileged
//! Architecture*, version **20240411**, Chapter 21 (`"F" Standard Extension for
//! Single-Precision Floating-Point, Version 2.2`). Section numbers below are
//! from that release; each citation also names the section *title*, so it stays
//! findable if a later release renumbers the chapters. No GPL implementation
//! (QEMU, GDB, GCC) was consulted for code — see
//! `docs/adr/2026-07-29-license-provenance-discipline.md`.
//!
//! ## FLEN is 32 here, so there is no NaN boxing
//!
//! §21.1 (`F Register State`) gives RV32F thirty-two registers of **FLEN = 32**
//! bits. Each register holds exactly one binary32 value and nothing else, so
//! the register file below is a plain `[u32; 32]` of raw bit patterns.
//!
//! NaN boxing — the rule that a narrower value stored in a wider `f` register
//! is held with all upper bits set to 1, and that an unboxed pattern reads back
//! as the canonical NaN — is defined in the `D` chapter, §22.2
//! (`NaN Boxing of Narrower Values`), and applies **only when FLEN > 32**. It
//! therefore does not apply to this emulator at all. Do not "fix" the register
//! file by adding boxing: on FLEN = 32 it would be wrong.
//!
//! If this file ever grows to FLEN = 64 (RV32D / RV64D), [`FpRegs::read_single`]
//! and [`FpRegs::write_single`] are the two places that change: `write_single`
//! would set the upper 32 bits to all ones, and `read_single` would return the
//! canonical NaN for any register whose upper half is not all ones. Every
//! single-precision executor goes through that pair precisely so the widening
//! has one home rather than thirty-odd call sites.

/// Invalid operation (`NV`) — `fflags` bit 4.
///
/// §21.2 `Floating-Point Control and Status Register`, accrued exception flag
/// field: bits 4:0 are `NV DZ OF UF NX` from bit 4 down to bit 0.
pub const FFLAG_NV: u8 = 1 << 4;
/// Divide by zero (`DZ`) — `fflags` bit 3.
pub const FFLAG_DZ: u8 = 1 << 3;
/// Overflow (`OF`) — `fflags` bit 2.
pub const FFLAG_OF: u8 = 1 << 2;
/// Underflow (`UF`) — `fflags` bit 1.
pub const FFLAG_UF: u8 = 1 << 1;
/// Inexact (`NX`) — `fflags` bit 0.
pub const FFLAG_NX: u8 = 1;

/// Mask of the five defined accrued-exception bits.
pub const FFLAGS_MASK: u8 = 0x1f;

/// CSR number of `fflags` (accrued exception flags alone).
pub const CSR_FFLAGS: u16 = 0x001;
/// CSR number of `frm` (dynamic rounding mode alone).
pub const CSR_FRM: u16 = 0x002;
/// CSR number of `fcsr` (`frm` in bits 7:5 over `fflags` in bits 4:0).
pub const CSR_FCSR: u16 = 0x003;

/// A RISC-V floating-point rounding mode.
///
/// §21.2 `Floating-Point Control and Status Register`, rounding-mode encoding:
///
/// | Encoding | Mnemonic | Meaning                                    |
/// |---------:|----------|--------------------------------------------|
/// | `000`    | `RNE`    | Round to nearest, ties to even             |
/// | `001`    | `RTZ`    | Round towards zero                         |
/// | `010`    | `RDN`    | Round down (towards −∞)                    |
/// | `011`    | `RUP`    | Round up (towards +∞)                      |
/// | `100`    | `RMM`    | Round to nearest, ties to max magnitude    |
/// | `101`    | —        | *Reserved* — illegal instruction           |
/// | `110`    | —        | *Reserved* — illegal instruction           |
/// | `111`    | `DYN`    | In an instruction: use `frm`. In `frm`: invalid |
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoundingMode {
    /// `RNE` — round to nearest, ties to even. The IEEE-754 default, and the
    /// only mode `docs/design/float.md` §2 lets the *shader* tier see. This
    /// emulator implements all five because RV32F machine code may name any of
    /// them, whatever the shader tier chooses to emit.
    Rne,
    /// `RTZ` — round towards zero (truncate).
    Rtz,
    /// `RDN` — round down, towards −∞.
    Rdn,
    /// `RUP` — round up, towards +∞.
    Rup,
    /// `RMM` — round to nearest, ties away from zero.
    Rmm,
}

impl RoundingMode {
    /// Resolve an instruction's 3-bit `rm` field against the current `frm`.
    ///
    /// Returns `None` for every encoding the spec rejects, and the caller must
    /// turn that into an illegal-instruction error (§21.2): `rm` = `101` or
    /// `110` are reserved, and `rm` = `111` (`DYN`) is illegal when `frm`
    /// itself holds a reserved or invalid value (`101`, `110`, or `111`).
    pub fn resolve(inst_rm: u8, frm: u8) -> Option<Self> {
        let selected = if inst_rm == 0b111 { frm } else { inst_rm };
        match selected {
            0b000 => Some(RoundingMode::Rne),
            0b001 => Some(RoundingMode::Rtz),
            0b010 => Some(RoundingMode::Rdn),
            0b011 => Some(RoundingMode::Rup),
            0b100 => Some(RoundingMode::Rmm),
            // 101 and 110 are reserved; 111 reaching here means `frm` held DYN,
            // which is not a rounding mode.
            _ => None,
        }
    }
}

/// The F extension's architectural state: `f0`–`f31` plus `fcsr`.
///
/// Registers hold **raw 32-bit patterns**, never a decoded `f32`. Sign-
/// injection, `FMV.X.W`/`FMV.W.X`, `FCLASS.S` and the load/store pair all move
/// bits without interpreting them, and routing any of those through an `f32`
/// would quietly canonicalize NaN payloads that the spec requires be preserved.
#[derive(Debug, Clone)]
pub struct FpRegs {
    /// `f0`–`f31`, raw binary32 bit patterns (FLEN = 32; see the module docs).
    regs: [u32; 32],
    /// Accrued exception flags, `fflags` bits 4:0. Sticky: set by arithmetic,
    /// cleared only by an explicit CSR write (§21.2).
    fflags: u8,
    /// Dynamic rounding mode, `frm` bits 2:0.
    frm: u8,
}

impl Default for FpRegs {
    fn default() -> Self {
        Self::new()
    }
}

impl FpRegs {
    /// Reset state: all registers zero, no accrued flags, `frm` = `RNE`.
    ///
    /// The spec leaves the register file's reset value unspecified; zeroing is
    /// the emulator's choice and matches how the integer file is initialized.
    pub const fn new() -> Self {
        Self {
            regs: [0; 32],
            fflags: 0,
            frm: 0,
        }
    }

    /// Read `f[idx]` as a binary32 bit pattern.
    ///
    /// FLEN = 32, so this is an unconditional load — see the module docs for
    /// why there is no NaN-unboxing check, and why this function exists anyway.
    #[inline]
    pub fn read_single(&self, idx: u8) -> u32 {
        self.regs[(idx & 0x1f) as usize]
    }

    /// Write a binary32 bit pattern to `f[idx]`.
    ///
    /// Unlike `x0`, **no floating-point register is hardwired to zero**: `f0` is
    /// an ordinary register (§21.1).
    #[inline]
    pub fn write_single(&mut self, idx: u8, bits: u32) {
        self.regs[(idx & 0x1f) as usize] = bits;
    }

    /// Current accrued exception flags (`fflags`, CSR `0x001`).
    #[inline]
    pub fn fflags(&self) -> u8 {
        self.fflags
    }

    /// Overwrite `fflags`; bits above 4:0 are reserved and ignored.
    #[inline]
    pub fn set_fflags(&mut self, value: u8) {
        self.fflags = value & FFLAGS_MASK;
    }

    /// OR new exception flags into `fflags`.
    ///
    /// §21.2: the flags are **accrued** — an arithmetic instruction never
    /// clears a flag another instruction set. Only a CSR write clears them.
    #[inline]
    pub fn accrue(&mut self, flags: u8) {
        self.fflags |= flags & FFLAGS_MASK;
    }

    /// Current dynamic rounding mode (`frm`, CSR `0x002`), as a 3-bit field.
    ///
    /// The value may be one the spec calls invalid (`101`–`111`); that is
    /// legal to *hold*, and only becomes an illegal instruction when an
    /// instruction with `rm` = `DYN` tries to use it. See
    /// [`RoundingMode::resolve`].
    #[inline]
    pub fn frm(&self) -> u8 {
        self.frm
    }

    /// Overwrite `frm`; bits above 2:0 are ignored.
    #[inline]
    pub fn set_frm(&mut self, value: u8) {
        self.frm = value & 0x7;
    }

    /// Read `fcsr` (CSR `0x003`): `frm` in bits 7:5 over `fflags` in bits 4:0.
    #[inline]
    pub fn fcsr(&self) -> u8 {
        (self.frm << 5) | self.fflags
    }

    /// Write `fcsr`. Bits 31:8 are reserved (`WPRI`) and are discarded.
    #[inline]
    pub fn set_fcsr(&mut self, value: u32) {
        self.fflags = (value as u8) & FFLAGS_MASK;
        self.frm = ((value >> 5) as u8) & 0x7;
    }

    /// Read one of the three F-extension CSRs by number.
    ///
    /// Returns `None` for every other CSR so the caller can keep the
    /// emulator's long-standing "CSR reads return 0" behaviour for the CSRs it
    /// genuinely does not model (`cycle`, `mstatus`, …). Only `fflags`, `frm`
    /// and `fcsr` became real state; nothing else changed.
    #[inline]
    pub fn read_csr(&self, csr: u16) -> Option<u32> {
        match csr {
            CSR_FFLAGS => Some(u32::from(self.fflags)),
            CSR_FRM => Some(u32::from(self.frm)),
            CSR_FCSR => Some(u32::from(self.fcsr())),
            _ => None,
        }
    }

    /// Write one of the three F-extension CSRs by number.
    ///
    /// Returns `true` if the CSR was one of ours (and was written), `false` if
    /// the caller should fall back to its no-op handling.
    #[inline]
    pub fn write_csr(&mut self, csr: u16, value: u32) -> bool {
        match csr {
            CSR_FFLAGS => {
                self.set_fflags(value as u8);
                true
            }
            CSR_FRM => {
                self.set_frm(value as u8);
                true
            }
            CSR_FCSR => {
                self.set_fcsr(value);
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registers_round_trip_raw_bits() {
        let mut fp = FpRegs::new();
        // A signaling NaN with a payload: nothing in the register file may
        // canonicalize it.
        fp.write_single(7, 0x7f80_0001);
        assert_eq!(fp.read_single(7), 0x7f80_0001);
        // f0 is an ordinary register, unlike x0.
        fp.write_single(0, 0xdead_beef);
        assert_eq!(fp.read_single(0), 0xdead_beef);
    }

    #[test]
    fn fcsr_packs_frm_above_fflags() {
        let mut fp = FpRegs::new();
        fp.set_fflags(FFLAG_NV | FFLAG_NX);
        fp.set_frm(0b011);
        assert_eq!(fp.fcsr(), (0b011 << 5) | 0b1_0001);
        assert_eq!(fp.fflags(), 0b1_0001);
        assert_eq!(fp.frm(), 0b011);
    }

    #[test]
    fn fcsr_write_splits_into_frm_and_fflags() {
        let mut fp = FpRegs::new();
        fp.set_fcsr(0b1010_1010);
        assert_eq!(fp.frm(), 0b101);
        assert_eq!(fp.fflags(), 0b0_1010);
    }

    #[test]
    fn fcsr_write_discards_reserved_high_bits() {
        let mut fp = FpRegs::new();
        fp.set_fcsr(0xffff_ff00);
        assert_eq!(fp.fcsr(), 0);
    }

    #[test]
    fn fflags_write_discards_bits_above_four() {
        let mut fp = FpRegs::new();
        fp.set_fflags(0xff);
        assert_eq!(fp.fflags(), FFLAGS_MASK);
    }

    #[test]
    fn accrue_is_sticky() {
        let mut fp = FpRegs::new();
        fp.accrue(FFLAG_NX);
        fp.accrue(FFLAG_OF);
        assert_eq!(fp.fflags(), FFLAG_NX | FFLAG_OF);
        fp.accrue(0);
        assert_eq!(fp.fflags(), FFLAG_NX | FFLAG_OF);
    }

    #[test]
    fn csr_dispatch_covers_only_the_three_fp_csrs() {
        let mut fp = FpRegs::new();
        assert_eq!(fp.read_csr(CSR_FFLAGS), Some(0));
        assert_eq!(fp.read_csr(CSR_FRM), Some(0));
        assert_eq!(fp.read_csr(CSR_FCSR), Some(0));
        // `cycle`, and anything else, stays unmodelled.
        assert_eq!(fp.read_csr(0xc00), None);
        assert!(!fp.write_csr(0xc00, 1));

        assert!(fp.write_csr(CSR_FRM, 0b010));
        assert_eq!(fp.frm(), 0b010);
        assert!(fp.write_csr(CSR_FFLAGS, FFLAG_DZ as u32));
        assert_eq!(fp.fflags(), FFLAG_DZ);
        assert_eq!(
            fp.read_csr(CSR_FCSR),
            Some(u32::from((0b010 << 5) | FFLAG_DZ))
        );
    }

    #[test]
    fn resolve_maps_the_five_defined_static_modes() {
        assert_eq!(RoundingMode::resolve(0b000, 0), Some(RoundingMode::Rne));
        assert_eq!(RoundingMode::resolve(0b001, 0), Some(RoundingMode::Rtz));
        assert_eq!(RoundingMode::resolve(0b010, 0), Some(RoundingMode::Rdn));
        assert_eq!(RoundingMode::resolve(0b011, 0), Some(RoundingMode::Rup));
        assert_eq!(RoundingMode::resolve(0b100, 0), Some(RoundingMode::Rmm));
    }

    #[test]
    fn resolve_rejects_reserved_static_modes() {
        assert_eq!(RoundingMode::resolve(0b101, 0), None);
        assert_eq!(RoundingMode::resolve(0b110, 0), None);
    }

    #[test]
    fn resolve_dyn_reads_frm() {
        assert_eq!(RoundingMode::resolve(0b111, 0b011), Some(RoundingMode::Rup));
        // frm holding a reserved or invalid value makes DYN illegal.
        assert_eq!(RoundingMode::resolve(0b111, 0b101), None);
        assert_eq!(RoundingMode::resolve(0b111, 0b110), None);
        assert_eq!(RoundingMode::resolve(0b111, 0b111), None);
    }
}
