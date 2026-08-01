// Encoding data for this module is **assembler-derived**: every opcode field
// value below was read out of `xtensa-esp32s3-elf-as` + `-objdump` output for a
// one-instruction `.S` file (see `lp-xt/fixtures/fp/README.md` for the exact
// derivation procedure and its raw output). Tool *output* is fact and carries no
// license obligation; no binutils, GCC, or QEMU source was read or adapted. See
//   docs/adr/2026-07-29-license-provenance-discipline.md
// Format names (`FP0`, `FP1`, `RRR`, `RRI8`) and operand semantics come from the
// Xtensa ISA Reference Manual.
//
//! The Xtensa single-precision FP coprocessor subset, plus the Boolean register
//! file its compares write to.
//!
//! # The normative subset (M6)
//!
//! This is the list M6 models and M7's emitter may produce. Each row carries two
//! independent verdicts, because they answer different questions:
//!
//! - **Asm** — does `xtensa-esp32s3-elf-as` (esp-14.2.0_20240906) accept the
//!   mnemonic for this chip? Answered here, no hardware needed.
//! - **Silicon** — does the ESP32-S3's FPU configuration actually implement it?
//!   Only a board answers that; `NOT PROBED` is a finding, never a default.
//!   **All 26 probes came back PRESENT** on the desk S3 (XIAO-class, 16 MB,
//!   MAC `d8:3b:da:47:29:70`) on 2026-07-31 — zero crashes, zero reboots. The
//!   record is `p1-silicon-results.md` in this milestone's planning directory;
//!   the payloads are `lp-xt/fixtures/fp/probe.S`. **PRESENT means the
//!   instruction executed, not that its numeric behavior is known** — presence
//!   is P1's question, behavior is P6's.
//!
//! | Instruction | Form | Asm | Silicon | Emitted by M7 |
//! |---|---|---|---|---|
//! | `add.s` `sub.s` `mul.s` | `fr, fs, ft` | OK | PRESENT | yes |
//! | `madd.s` `msub.s` | `fr, fs, ft` | OK | PRESENT | yes |
//! | `maddn.s` `divn.s` | `fr, fs, ft` | OK | PRESENT | division sequence |
//! | `mov.s` `abs.s` `neg.s` | `fr, fs` | OK | PRESENT | yes |
//! | `div0.s` `recip0.s` `sqrt0.s` `rsqrt0.s` | `fr, fs` | OK | PRESENT | division / sqrt sequence |
//! | `nexp01.s` `mkdadj.s` `addexp.s` `addexpm.s` | `fr, fs` | OK | PRESENT | division / sqrt sequence |
//! | `mksadj.s` | `fr, fs` | OK | NOT PROBED (added by M6 P6; exercised by the sqrt sequence) | sqrt sequence |
//! | `const.s` | `fr, imm0_15` | OK | PRESENT | division sequence |
//! | `rfr` | `ar, fs` | OK | PRESENT | yes |
//! | `wfr` | `fr, as` | OK | PRESENT | yes |
//! | `moveqz.s` `movnez.s` `movltz.s` `movgez.s` | `fr, fs, at` | OK | PRESENT | yes |
//! | `movf.s` `movt.s` | `fr, fs, bt` | OK | PRESENT | yes |
//! | `oeq.s` `olt.s` `ole.s` `ueq.s` `ult.s` `ule.s` `un.s` | `br, fs, ft` | OK | PRESENT | yes |
//! | `round.s` `trunc.s` `floor.s` `ceil.s` `utrunc.s` | `ar, fs, imm0_15` | OK | PRESENT | `trunc.s` at least |
//! | `float.s` `ufloat.s` | `fr, as, imm0_15` | OK | PRESENT | yes |
//! | `lsi` `ssi` | `ft, as, 0..=1020 step 4` | OK | PRESENT | yes (spills) |
//! | `lsip` `ssip` | `ft, as, 0..=1020 step 4` | OK | PRESENT | no |
//! | `lsx` `ssx` | `fr, as, at` | OK | PRESENT | yes |
//! | `lsxp` `ssxp` | `fr, as, at` | OK | PRESENT | no |
//! | `bt` `bf` | `bs, target` | OK | PRESENT | yes (compare → branch) |
//! | `movt` `movf` | `ar, as, bt` | OK | PRESENT | yes (compare → AR) |
//! | `rsr.br` `wsr.br` `xsr.br` | `at` | OK | PRESENT | maybe (bulk BR save) |
//! | `rsr.cpenable` `wsr.cpenable` `xsr.cpenable` | `at` | OK | PRESENT | firmware, not shader code |
//! | `rur.fcr` `wur.fcr` `rur.fsr` `wur.fsr` | `at` | OK | PRESENT | no (test/probe only) |
//!
//! ## Named in the plan but **not** in the subset
//!
//! | Name | Why not |
//! |---|---|
//! | `lsiu` `ssiu` `lsxu` `ssxu` | **Assembler rejects these mnemonics.** The ISA manual's auto-update load/store forms are spelled `lsip`/`ssip`/`lsxp`/`ssxp` by this toolchain, and those *are* accepted; they occupy the encoding slots (`op0=3`, `r=8`/`0xC`; `op1=8`, `op2=1`/`5`) and are modeled under the `p` names. Recorded as a naming finding, not an absence. |
//!
//! ## Deliberately excluded
//!
//! - **Double precision** — not on this chip.
//! - **The ESP32-S3 DSP `ee.*` family** — a different coprocessor; out of scope
//!   repo-wide.
//! - **Boolean logic ops** (`andb` `andbc` `orb` `orbc` `xorb`, `all4` `any4`
//!   `all8` `any8`). The assembler accepts all of them, so their absence here is
//!   scope, not capability: M6 needs the Boolean file only to *observe* FP
//!   compares (D10), which `bt`/`bf`/`movt`/`movf` already do.
//! - **A general SR/UR model.** Only `BR` (SR 4), `CPENABLE` (SR 224), `FCR`
//!   (UR 232), and `FSR` (UR 233) decode; every other special/user register
//!   stays `DecodeError::Unsupported`, as it was before this module existed.
//!
//! ## What the probe settled beyond presence (2026-07-31)
//!
//! - **The whole div/sqrt helper family exists** — `div0.s`, `divn.s`,
//!   `maddn.s`, `nexp01.s`, `mkdadj.s`, `recip0.s`, `rsqrt0.s`, `sqrt0.s`,
//!   `const.s` all executed. M6's escalation A1 ("if the helpers are absent,
//!   M7's division lowering changes materially") is retired on silicon.
//! - **`CPENABLE` arrives armed** under the esp-hal boot chain: the
//!   deliberately-unarmed probe returned its staged value instead of faulting,
//!   on a fresh boot too. Provenance is *not* pinned — no `wsr.cpenable` exists
//!   in esp-hal 1.1.1 or xtensa-lx-rt 0.22 startup, so it is presumably ROM or
//!   the second-stage bootloader. The measured fact is "armed under this boot
//!   chain", not "armed by architecture", so M7 arms it defensively anyway.
//! - **`FCR` and `FSR` both reset to 0**, and **FSR accumulates**: 0 on a fresh
//!   boot, `0x400` after a 24-instruction FP sweep with no intervening write. So
//!   FSR is a sticky-flag register on this chip and `lp-xt-emu` models
//!   accumulation — even though `float.md` §2 puts it out of shader reach.
//!
//! ## What `docs/design/float.md` already constrains here
//!
//! The product spec outranks both IEEE-where-adopted and silicon-as-data, so
//! three of its rows bind M7's use of this subset before any measurement:
//!
//! - **`FCR` is not shader-reachable.** float.md §2: rounding is
//!   round-to-nearest-even always, there is no dynamic rounding mode, and
//!   "emitters never change the mode". `rur.fcr`/`wur.fcr` are modeled so M6 can
//!   *measure* the reset value and so the conformance harness can prove the mode
//!   never moves — not so compiled shader code can touch them.
//! - **`FSR` is not observable either.** float.md §2: no floating-point
//!   exceptions or observable status flags. Same reasoning: modeled to measure,
//!   not to emit.
//! - **`trunc.s` alone does not satisfy the conversion row.** float.md §3 makes
//!   float→int truncation toward zero with **saturation** of finite
//!   out-of-range values a *Guaranteed* behavior, and §5 leaves NaN
//!   unspecified. Whether the S3's `trunc.s` saturates or wraps is a P4/P6
//!   measurement; if it does not saturate, M7 owes a clamp, not a spec
//!   amendment.
//!
//! Denormal flush-to-zero is *target-defined by policy* (float.md §4, settled at
//! G1): the S3 is expected to flush and M6's job is to supply the datum, not to
//! decide the semantics.
//!
//! ## Encoding slots left unassigned
//!
//! Two slots in the FP1 group (`op0=0, op1=0xA, op2=0xF`) — selector `t=2` and
//! `t=0xC` — are not recognized by `xtensa-esp32s3-elf-objdump`, so no mnemonic
//! was found for them. They decode as unsupported. A finding, not a default.

/// An Xtensa floating-point register `f0`..`f15`.
///
/// Deliberately **not** [`crate::Reg`]: its `Debug` prints `a3`, which would
/// break disassembly and objdiff matching. The FR file is also flat — the
/// windowed rotation that makes AR preservation free has no FR analogue.
///
/// The inner value is always in `0..=15`; constructors enforce this.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct FReg(u8);

impl FReg {
    /// Create a register from a raw number, panicking if `>15`.
    #[inline]
    pub const fn new(n: u8) -> FReg {
        assert!(n < 16, "Xtensa float register out of range");
        FReg(n)
    }

    /// Create a register from the low 4 bits of `n` (for decode).
    #[inline]
    pub const fn from_nibble(n: u8) -> FReg {
        FReg(n & 0x0f)
    }

    /// The raw register number `0..=15`.
    #[inline]
    pub const fn num(self) -> u8 {
        self.0
    }
}

impl core::fmt::Debug for FReg {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "f{}", self.0)
    }
}

/// An Xtensa Boolean register `b0`..`b15` (the Boolean core option).
///
/// FP compares write here, not to an AR, so without this file a compare result
/// cannot be read back at all — on silicon or in the emulator.
///
/// The inner value is always in `0..=15`; constructors enforce this.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct BReg(u8);

impl BReg {
    /// Create a register from a raw number, panicking if `>15`.
    #[inline]
    pub const fn new(n: u8) -> BReg {
        assert!(n < 16, "Xtensa boolean register out of range");
        BReg(n)
    }

    /// Create a register from the low 4 bits of `n` (for decode).
    #[inline]
    pub const fn from_nibble(n: u8) -> BReg {
        BReg(n & 0x0f)
    }

    /// The raw register number `0..=15`.
    #[inline]
    pub const fn num(self) -> u8 {
        self.0
    }
}

impl core::fmt::Debug for BReg {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "b{}", self.0)
    }
}

/// Three-FR-operand FP arithmetic (`FP0`: `op0 = 0`, `op1 = 0xA`).
/// Shape: `op fr, fs, ft`. The `op2` field selects the operation.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FpRrrOp {
    /// `add.s` — `op2 = 0`.
    AddS,
    /// `sub.s` — `op2 = 1`.
    SubS,
    /// `mul.s` — `op2 = 2`.
    MulS,
    /// `madd.s fr, fs, ft` — accumulates *into* `fr`; `op2 = 4`.
    MaddS,
    /// `msub.s fr, fs, ft` — accumulates *into* `fr`; `op2 = 5`.
    MsubS,
    /// `maddn.s` — the no-rounding multiply-accumulate of the divide sequence;
    /// `op2 = 6`.
    MaddnS,
    /// `divn.s` — the divide-step accumulate; `op2 = 7`.
    DivnS,
}

/// Two-FR-operand FP ops (`FP1`: `op0 = 0`, `op1 = 0xA`, `op2 = 0xF`).
/// Shape: `op fr, fs`. The `t` field selects the operation.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FpRrOp {
    /// `mov.s` — `t = 0`.
    MovS,
    /// `abs.s` — `t = 1`.
    AbsS,
    /// `neg.s` — `t = 6`.
    NegS,
    /// `div0.s` — divide-sequence seed; `t = 7`.
    Div0S,
    /// `recip0.s` — reciprocal estimate (implementation-defined table);
    /// `t = 8`.
    Recip0S,
    /// `sqrt0.s` — square-root estimate (implementation-defined table);
    /// `t = 9`.
    Sqrt0S,
    /// `rsqrt0.s` — reciprocal-square-root estimate (implementation-defined
    /// table); `t = 0xA`.
    Rsqrt0S,
    /// `nexp01.s` — normalize exponent to [1,2); `t = 0xB`.
    Nexp01S,
    /// `mksadj.s` — make square-root adjustment; `t = 0xC`.
    ///
    /// Found by the M6 P6 campaign, not the P1 probe: the toolchain's
    /// `__ieee754_sqrtf` sequence uses it (`xtensa-esp32s3-elf-objdump` of the
    /// esp-14.2.0 libm disassembles `0xfa21c0` as `mksadj.s f2, f1`), so the
    /// square-root sequence cannot be decoded without it.
    MksadjS,
    /// `mkdadj.s` — make divide adjustment; `t = 0xD`.
    MkdadjS,
    /// `addexp.s` — `t = 0xE`.
    AddexpS,
    /// `addexpm.s` — `t = 0xF`.
    AddexpmS,
}

/// FP compares (`op0 = 0`, `op1 = 0xB`). Shape: `op br, fs, ft` — the result
/// lands in a **Boolean** register, never an AR.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FpCmpOp {
    /// `un.s` — unordered; `op2 = 1`.
    UnS,
    /// `oeq.s` — ordered equal; `op2 = 2`.
    OeqS,
    /// `ueq.s` — unordered-or-equal; `op2 = 3`.
    UeqS,
    /// `olt.s` — ordered less-than; `op2 = 4`.
    OltS,
    /// `ult.s` — unordered-or-less-than; `op2 = 5`.
    UltS,
    /// `ole.s` — ordered less-or-equal; `op2 = 6`.
    OleS,
    /// `ule.s` — unordered-or-less-or-equal; `op2 = 7`.
    UleS,
}

/// FP conditional moves keyed on an **address** register (`op0 = 0`,
/// `op1 = 0xB`). Shape: `op fr, fs, at`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FpMovArOp {
    /// `moveqz.s` — `op2 = 8`.
    MoveqzS,
    /// `movnez.s` — `op2 = 9`.
    MovnezS,
    /// `movltz.s` — `op2 = 0xA`.
    MovltzS,
    /// `movgez.s` — `op2 = 0xB`.
    MovgezS,
}

/// FP conditional moves keyed on a **Boolean** register (`op0 = 0`,
/// `op1 = 0xB`). Shape: `op fr, fs, bt` — the branch-free consumer of a compare.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FpMovBrOp {
    /// `movf.s` — move when `bt` is clear; `op2 = 0xC`.
    MovfS,
    /// `movt.s` — move when `bt` is set; `op2 = 0xD`.
    MovtS,
}

/// Float → integer conversions (`FP0`, `op0 = 0`, `op1 = 0xA`).
/// Shape: `op ar, fs, imm` where `imm` (0..=15) is a binary scale applied
/// before conversion; the `t` field carries it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FpToIntOp {
    /// `round.s` — `op2 = 8`.
    RoundS,
    /// `trunc.s` — `op2 = 9`.
    TruncS,
    /// `floor.s` — `op2 = 0xA`.
    FloorS,
    /// `ceil.s` — `op2 = 0xB`.
    CeilS,
    /// `utrunc.s` — `op2 = 0xE`.
    UtruncS,
}

/// Integer → float conversions (`FP0`, `op0 = 0`, `op1 = 0xA`).
/// Shape: `op fr, as, imm` where `imm` (0..=15) is a binary scale applied
/// after conversion; the `t` field carries it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum IntToFpOp {
    /// `float.s` — signed; `op2 = 0xC`.
    FloatS,
    /// `ufloat.s` — unsigned; `op2 = 0xD`.
    UfloatS,
}

/// Indexed FP load/store (`op0 = 0`, `op1 = 8`). Shape: `op fr, as, at`.
///
/// The `p` forms additionally write `as + at` back to `as`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FpLsxOp {
    /// `lsx` — `op2 = 0`.
    Lsx,
    /// `lsxp` — `op2 = 1`.
    Lsxp,
    /// `ssx` — `op2 = 4`.
    Ssx,
    /// `ssxp` — `op2 = 5`.
    Ssxp,
}

/// Immediate-offset FP load/store (`RRI8`, `op0 = 3`).
/// Shape: `op ft, as, offset`, offset 0..=1020 in steps of 4 (the `imm8` field
/// holds `offset / 4`).
///
/// The `p` forms additionally write `as + offset` back to `as`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FpLsiOp {
    /// `lsi` — `r = 0`.
    Lsi,
    /// `ssi` — `r = 4`.
    Ssi,
    /// `lsip` — `r = 8`.
    Lsip,
    /// `ssip` — `r = 0xC`.
    Ssip,
}
