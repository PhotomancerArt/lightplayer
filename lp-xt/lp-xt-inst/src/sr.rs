// Encoding data for this module is **assembler-derived**: the RSR/WSR/XSR/RUR/
// WUR field layouts and every register number below were read out of
// `xtensa-esp32s3-elf-as` + `-objdump` output for one-instruction `.S` files
// (see `lp-xt/fixtures/fp/README.md`). Tool *output* is fact; no binutils, GCC,
// or QEMU source was read or adapted. See
//   docs/adr/2026-07-29-license-provenance-discipline.md
//
//! Special- and user-register access, **narrowly** — only the registers M6
//! needs.
//!
//! This is the crate's first SR/UR support and it is deliberately not a general
//! model. Xtensa has ~100 special registers and the emulator models none of the
//! privileged machinery behind them; decoding `rsr.ps` here would create a
//! variant nothing can execute. Four registers earn their place:
//!
//! | Register | Number | Why M6 needs it |
//! |---|---|---|
//! | `BR` | SR 4 | The Boolean file, moved in bulk — the only way to snapshot all 16 compare results at once. |
//! | `CPENABLE` | SR 224 | Gates coprocessor 0. Un-armed FP access raises EXCCAUSE 32; firmware must arm it before any compiled float code runs. |
//! | `FCR` | UR 232 | FP control: rounding mode. |
//! | `FSR` | UR 233 | FP status: the sticky exception flags. |
//!
//! Every other special or user register still decodes as
//! [`crate::DecodeError::Unsupported`], exactly as before.

/// A special register reachable by `RSR`/`WSR`/`XSR` in this crate's subset.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SpecialReg {
    /// `BR` — the 16-bit Boolean register file, SR 4.
    Br,
    /// `CPENABLE` — the per-coprocessor enable mask, SR 224. Bit 0 is the FPU.
    Cpenable,
}

impl SpecialReg {
    /// The architectural special-register number.
    #[inline]
    pub const fn num(self) -> u8 {
        match self {
            SpecialReg::Br => 4,
            SpecialReg::Cpenable => 224,
        }
    }

    /// The register for an architectural number, or `None` if outside the
    /// modeled set.
    #[inline]
    pub const fn from_num(n: u8) -> Option<SpecialReg> {
        match n {
            4 => Some(SpecialReg::Br),
            224 => Some(SpecialReg::Cpenable),
            _ => None,
        }
    }

    /// The objdump mnemonic suffix (`rsr.<name>`).
    #[inline]
    pub const fn name(self) -> &'static str {
        match self {
            SpecialReg::Br => "br",
            SpecialReg::Cpenable => "cpenable",
        }
    }
}

/// A user register reachable by `RUR`/`WUR` in this crate's subset.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UserReg {
    /// `FCR` — FP control register (rounding mode), UR 232.
    Fcr,
    /// `FSR` — FP status register (sticky exception flags), UR 233.
    Fsr,
}

impl UserReg {
    /// The architectural user-register number.
    #[inline]
    pub const fn num(self) -> u8 {
        match self {
            UserReg::Fcr => 232,
            UserReg::Fsr => 233,
        }
    }

    /// The register for an architectural number, or `None` if outside the
    /// modeled set.
    #[inline]
    pub const fn from_num(n: u8) -> Option<UserReg> {
        match n {
            232 => Some(UserReg::Fcr),
            233 => Some(UserReg::Fsr),
            _ => None,
        }
    }

    /// The objdump mnemonic suffix (`rur.<name>`).
    #[inline]
    pub const fn name(self) -> &'static str {
        match self {
            UserReg::Fcr => "fcr",
            UserReg::Fsr => "fsr",
        }
    }
}

/// Which special-register access a [`crate::Inst::Sr`] performs.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SrOp {
    /// `rsr.<sr> at` — read; `op1 = 3`, `op2 = 0`.
    Rsr,
    /// `wsr.<sr> at` — write; `op1 = 3`, `op2 = 1`.
    Wsr,
    /// `xsr.<sr> at` — atomic exchange; `op1 = 1`, `op2 = 6`.
    Xsr,
}

impl SrOp {
    /// The objdump mnemonic prefix.
    #[inline]
    pub const fn name(self) -> &'static str {
        match self {
            SrOp::Rsr => "rsr",
            SrOp::Wsr => "wsr",
            SrOp::Xsr => "xsr",
        }
    }
}

/// Which user-register access a [`crate::Inst::Ur`] performs.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UrOp {
    /// `rur.<ur> at` — read; `op1 = 3`, `op2 = 0xE`.
    Rur,
    /// `wur.<ur> at` — write; `op1 = 3`, `op2 = 0xF`.
    Wur,
}

impl UrOp {
    /// The objdump mnemonic prefix.
    #[inline]
    pub const fn name(self) -> &'static str {
        match self {
            UrOp::Rur => "rur",
            UrOp::Wur => "wur",
        }
    }
}
