//! Everything that computes a float value: arithmetic, compares, conversions,
//! the conditional moves, and the divide/sqrt helper family.
//!
//! # What is fixed and what is measured
//!
//! IEEE-754 binary32 under round-to-nearest-even fixes the *value* of `add.s`,
//! `sub.s`, and `mul.s` for finite operands, and Rust's `f32` is exactly that
//! arithmetic — including subnormals, because the M6 P6 campaign measured that
//! **this FPU does not flush denormals in either direction** (family F3: every
//! one of 350 rows is consistent with full IEEE subnormal arithmetic, and the
//! 80 rows that could distinguish flushing from IEEE all came back IEEE).
//!
//! Everything IEEE leaves open was measured on the desk S3 (2026-07-31,
//! chip `d8:3b:da:47:29:70`) and is cited field by field in
//! [`crate::fp_policy::FpPolicy`]:
//!
//! - **NaNs propagate last-operand-quieted** (family F2): the result is the
//!   *last* NaN operand in `(fs, ft)` order with the quiet bit forced and the
//!   payload preserved; a NaN generated from non-NaN operands is `0x7FC00000`.
//! - **`FCR.RM` is honored** (family F1: 556 of 648 operand groups produce
//!   mode-dependent results, and all 1944 directed-mode rows match IEEE-754
//!   directed rounding bit for bit). The emulator therefore implements the
//!   three directed modes for `add.s`/`sub.s`/`mul.s` — the measured surface —
//!   and *refuses* a non-default mode for every other operation, where no
//!   measurement exists.
//! - **`round.s` breaks ties to even** (family F6).
//! - **`utrunc.s` of an in-range negative wraps like the signed conversion**
//!   (family F6, 16 DIVERGE rows) — falsifying the ISA RM's `0x80000000`
//!   claim, which silicon only honors below `i32::MIN`.
//! - **FSR flags are real** (falsifying ISA RM §4.3.11.4): INEXACT on any
//!   rounded result, UNDERFLOW on tiny-and-inexact, OVERFLOW alongside
//!   INEXACT, INVALID on signalling-NaN operands, NaN generation, invalid
//!   conversions — and on quiet NaNs through `olt.s`/`ole.s` only, exactly
//!   IEEE's signaling-predicate rule.
//!
//! # Divide and square root
//!
//! There is no divide or sqrt instruction; both are the toolchain's code
//! sequences over the helper family, whose semantics are measured (not
//! documented anywhere) and live in [`crate::fp_rom`]. `maddn.s` measured
//! bit-identical to `madd.s` on all 1536 probe points at RNE, but sets **no**
//! FSR flags; `divn.s` performs the sequence's final correctly-rounded step
//! and is modeled as the fused accumulate — exact on the sequence envelope,
//! with its off-envelope behavior recorded in the campaign record rather than
//! guessed at.
//!
//! # Provenance
//!
//! IEEE-754, the Xtensa ISA Reference Manual (cited by page where used), and
//! the M6 P6 silicon captures under `tests/fixtures/fp/captures/`. No manual
//! text is reproduced, and no QEMU, binutils, or GCC source was read.

use lp_xt_inst::{FpCmpOp, FpMovArOp, FpMovBrOp, FpRrOp, FpRrrOp, FpToIntOp, Inst, IntToFpOp};

use crate::cpu::{FSR_INEXACT, FSR_INVALID, FSR_OVERFLOW, FSR_UNDERFLOW};
use crate::emu::{Emulator, Flow};
use crate::error::Trap;
use crate::fp_policy::{NanRule, OutOfRangeRule, TiesRule, UtruncNegativeRule};
use crate::fp_rom;
use crate::trace::Tracer;

/// The binary32 sign bit.
const SIGN: u32 = 0x8000_0000;
/// Exponent field, shifted into place.
const EXP: u32 = 0x7F80_0000;
/// Significand field.
const FRAC: u32 = 0x007F_FFFF;

/// A NaN: exponent all ones, significand non-zero.
#[inline]
fn is_nan(bits: u32) -> bool {
    bits & EXP == EXP && bits & FRAC != 0
}

/// A signalling NaN: NaN with the quiet bit clear.
#[inline]
fn is_snan(bits: u32) -> bool {
    is_nan(bits) && bits & 0x0040_0000 == 0
}

/// The result of one arithmetic step: the bits, plus the FSR flags it raised.
type FpOut = (u32, u32);

impl Emulator {
    pub(super) fn exec_float_math(
        &mut self,
        inst: &Inst,
        tracer: &mut dyn Tracer,
    ) -> Result<Flow, Trap> {
        self.require_fpu()?;

        match *inst {
            Inst::FpRrr(op, fr, fs, ft) => {
                let (a, b) = (self.rfreg(fs.num()), self.rfreg(ft.num()));
                let (out, fsr) = match op {
                    FpRrrOp::AddS => self.fp_binop(a, b, false, |x, y| x + y),
                    FpRrrOp::SubS => self.fp_binop(a, b, true, |x, y| x - y),
                    FpRrrOp::MulS => self.fp_mul(a, b),
                    // madd/msub accumulate INTO fr, so it is a third operand.
                    FpRrrOp::MaddS => self.fp_madd(self.rfreg(fr.num()), a, b, false, true),
                    FpRrrOp::MsubS => self.fp_madd(self.rfreg(fr.num()), a, b, true, true),
                    // The Newton-step accumulate: measured bit-identical to
                    // madd.s on all 1536 probe points, but flag-silent.
                    FpRrrOp::MaddnS => {
                        let (v, _) = self.fp_madd(self.rfreg(fr.num()), a, b, false, false);
                        (v, 0)
                    }
                    // The sequences' final correctly-rounded step, which also
                    // reassembles mkdadj/mksadj's split exponent encoding.
                    // Fully measured semantics — see fp_rom::divn.
                    FpRrrOp::DivnS => {
                        self.require_rne("divn.s");
                        fp_rom::divn(self.rfreg(fr.num()), a, b)
                    }
                };
                self.cpu.or_fsr(fsr);
                self.wfreg(fr.num(), out, tracer);
            }

            Inst::FpRr(op, fr, fs) => {
                let a = self.rfreg(fs.num());
                let (out, fsr) = match op {
                    // Sign-bit operations, not `f32::abs()` / `-x`: they must
                    // not canonicalize a NaN and must give the right signed
                    // zero. `float.md` §3 makes both Guaranteed.
                    FpRrOp::AbsS => (a & !SIGN, 0),
                    FpRrOp::NegS => (a ^ SIGN, 0),
                    // The measured estimate ROMs and helper semantics (D5).
                    FpRrOp::Recip0S => fp_rom::recip0(a),
                    FpRrOp::Sqrt0S => fp_rom::sqrt0(a),
                    FpRrOp::Rsqrt0S => fp_rom::rsqrt0(a),
                    FpRrOp::Div0S => fp_rom::div0(a),
                    FpRrOp::Nexp01S => (fp_rom::nexp01(a), 0),
                    FpRrOp::MksadjS => fp_rom::mksadj(a),
                    // mkdadj.s reads its destination too: fr holds the
                    // divide sequence's denominator, fs the numerator.
                    FpRrOp::MkdadjS => fp_rom::mkdadj(self.rfreg(fr.num()), a),
                    FpRrOp::AddexpS => (fp_rom::addexp(self.rfreg(fr.num()), a), 0),
                    FpRrOp::AddexpmS => (fp_rom::addexpm(self.rfreg(fr.num()), a), 0),
                    // `mov.s` is data movement and lives in `float.rs`.
                    FpRrOp::MovS => unreachable!("mov.s is handled by exec_float"),
                };
                self.cpu.or_fsr(fsr);
                self.wfreg(fr.num(), out, tracer);
            }

            // Sixteen architecturally-defined constants; measured, the table
            // is `[0.0, 1.0, 2.0, 0.5]` selected by `imm & 3`.
            Inst::ConstS(fr, imm) => {
                let table = self.fp_policy.const_s_table.get();
                let out = table[usize::from(imm) & 0xf];
                self.wfreg(fr.num(), out, tracer);
            }

            // Conditional moves: pure bit copies, predicated on an AR.
            Inst::FpMovAr(op, fr, fs, at) => {
                let cond = self.rreg(at.num());
                let take = match op {
                    FpMovArOp::MoveqzS => cond == 0,
                    FpMovArOp::MovnezS => cond != 0,
                    FpMovArOp::MovltzS => (cond as i32) < 0,
                    FpMovArOp::MovgezS => (cond as i32) >= 0,
                };
                if take {
                    let v = self.rfreg(fs.num());
                    self.wfreg(fr.num(), v, tracer);
                }
            }

            // ...and predicated on a BR: the branch-free consumer of a compare.
            Inst::FpMovBr(op, fr, fs, bt) => {
                let want = matches!(op, FpMovBrOp::MovtS);
                if self.cpu.b(bt.num()) == want {
                    let v = self.rfreg(fs.num());
                    self.wfreg(fr.num(), v, tracer);
                }
            }

            // Compares write a BOOLEAN register, never an AR. IEEE fixes every
            // predicate; the flags are measured: a signalling NaN raises
            // INVALID on every compare, and `olt.s`/`ole.s` raise it on quiet
            // NaNs too — IEEE's signaling-predicate rule, on real silicon,
            // falsifying the ISA RM's "no signalling NaN support" claim.
            Inst::FpCmp(op, br, fs, ft) => {
                let a = self.rfreg(fs.num());
                let b = self.rfreg(ft.num());
                let (x, y) = (f32::from_bits(a), f32::from_bits(b));
                let unordered = x.is_nan() || y.is_nan();
                let r = match op {
                    FpCmpOp::UnS => unordered,
                    FpCmpOp::OeqS => !unordered && x == y,
                    FpCmpOp::UeqS => unordered || x == y,
                    FpCmpOp::OltS => !unordered && x < y,
                    FpCmpOp::UltS => unordered || x < y,
                    FpCmpOp::OleS => !unordered && x <= y,
                    FpCmpOp::UleS => unordered || x <= y,
                };
                let signaling = matches!(op, FpCmpOp::OltS | FpCmpOp::OleS);
                if is_snan(a) || is_snan(b) || (signaling && unordered) {
                    let snan = self.fp_policy.snan_compare_signals.get();
                    debug_assert!(*snan, "measured: compares raise INVALID");
                    self.cpu.or_fsr(FSR_INVALID);
                }
                self.wbreg(br.num(), r, tracer);
            }

            Inst::FpToInt(op, ar, fs, imm) => {
                let bits = self.rfreg(fs.num());
                let (out, fsr) = self.fp_to_int(op, bits, imm);
                self.cpu.or_fsr(fsr);
                self.wreg(ar.num(), out, tracer);
            }

            Inst::IntToFp(op, fr, ars, imm) => {
                self.require_rne("float.s/ufloat.s");
                let v = self.rreg(ars.num());
                let x64 = match op {
                    IntToFpOp::FloatS => f64::from(v as i32),
                    IntToFpOp::UfloatS => f64::from(v),
                };
                // The scale divides by an exact power of two, and f64 holds
                // any i32/u32 and the scaled value exactly — so converting
                // the exact f64 rounds exactly once, and exactness of the
                // whole conversion is a plain comparison.
                let scaled64 = x64 / f64::from(self.scale_factor(imm));
                let out = (scaled64 as f32).to_bits();
                let fsr = if f64::from(f32::from_bits(out)) == scaled64 {
                    0
                } else {
                    FSR_INEXACT
                };
                self.cpu.or_fsr(fsr);
                self.wfreg(fr.num(), out, tracer);
            }

            _ => unreachable!("exec_float_math got {inst:?}"),
        }
        Ok(Flow::Next)
    }

    /// Refuse a non-default rounding mode for the operations the campaign did
    /// not measure under one. `FCR.RM` is honored on this silicon
    /// (`fcr_rounding_honored`, family F1) and implemented for
    /// `add.s`/`sub.s`/`mul.s`; every other operation was measured at RNE
    /// only, and an unmeasured corner is a refusal, not a default (D6).
    fn require_rne(&self, what: &str) {
        let rm = self.cpu.fcr_rounding_mode();
        if rm != crate::cpu::FCR_RM_NEAREST {
            panic!(
                "{what} under FCR.RM={rm} is unmeasured: the F1 campaign measured \
                 directed rounding for add.s/sub.s/mul.s only. Measure before modeling."
            );
        }
    }

    /// Which NaN survives, when at least one operand is one. Measured (F2):
    /// the **last** NaN operand, quieted, payload preserved.
    fn fp_nan_out(&self, operands: &[u32]) -> u32 {
        match *self.fp_policy.nan_propagation.get() {
            NanRule::LastOperandQuieted => {
                let nan = operands
                    .iter()
                    .rev()
                    .copied()
                    .find(|b| is_nan(*b))
                    .expect("called only when an operand is NaN");
                nan | 0x0040_0000
            }
            NanRule::FirstOperandQuieted => {
                let nan = operands
                    .iter()
                    .copied()
                    .find(|b| is_nan(*b))
                    .expect("called only when an operand is NaN");
                nan | 0x0040_0000
            }
            NanRule::Canonicalize => *self.fp_policy.default_generated_nan.get(),
        }
    }

    /// Flags for a rounded arithmetic result: INEXACT when the f32 result is
    /// not the exact value, UNDERFLOW when it is also tiny (after rounding),
    /// OVERFLOW when it saturated to infinity. `exact` is the mathematically
    /// exact value as `(f64 head, residual-is-nonzero)`.
    fn round_flags(result: u32, head: f64, residual: bool) -> u32 {
        let r = f32::from_bits(result);
        let inexact = residual || f64::from(r) != head;
        if !inexact {
            return 0;
        }
        let mut fsr = FSR_INEXACT;
        if r.is_infinite() {
            fsr |= FSR_OVERFLOW;
        } else if r == 0.0 || r.is_subnormal() {
            fsr |= FSR_UNDERFLOW;
        }
        fsr
    }

    /// `add.s` / `sub.s`: IEEE-fixed at RNE (including subnormals — measured,
    /// no flush), IEEE directed rounding under a non-default `FCR.RM`
    /// (measured, F1).
    fn fp_binop(&self, a: u32, b: u32, negate_b: bool, f: impl Fn(f32, f32) -> f32) -> FpOut {
        if is_nan(a) || is_nan(b) {
            let fsr = if is_snan(a) || is_snan(b) {
                FSR_INVALID
            } else {
                0
            };
            return (self.fp_nan_out(&[a, b]), fsr);
        }
        let (x, y) = (f32::from_bits(a), f32::from_bits(b));
        let yy = if negate_b { -y } else { y };
        // Infinite operands: the result is exact (an infinity) in every
        // rounding mode, or a generated NaN (inf - inf).
        if x.is_infinite() || yy.is_infinite() {
            let r = x + yy;
            if r.is_nan() {
                return (*self.fp_policy.default_generated_nan.get(), FSR_INVALID);
            }
            return (r.to_bits(), 0);
        }
        // Exact sum: f64 head + Knuth two-sum residual (exact algebra).
        let (head, residual) = two_sum(f64::from(x), f64::from(yy));
        let rm = self.cpu.fcr_rounding_mode();
        let out = if rm == crate::cpu::FCR_RM_NEAREST {
            f(x, y).to_bits()
        } else {
            debug_assert!(*self.fp_policy.fcr_rounding_honored.get());
            round_f64_residual_to_f32(head, residual_dir(residual), rm)
        };
        (out, Self::round_flags(out, head, residual != 0.0))
    }

    /// `mul.s`: the product of two f32s is exact in f64, so both the directed
    /// rounding and the exactness test work from the exact value.
    fn fp_mul(&self, a: u32, b: u32) -> FpOut {
        if is_nan(a) || is_nan(b) {
            let fsr = if is_snan(a) || is_snan(b) {
                FSR_INVALID
            } else {
                0
            };
            return (self.fp_nan_out(&[a, b]), fsr);
        }
        let (x, y) = (f32::from_bits(a), f32::from_bits(b));
        if x.is_infinite() || y.is_infinite() {
            let r = x * y;
            if r.is_nan() {
                // inf * 0: a generated NaN.
                return (*self.fp_policy.default_generated_nan.get(), FSR_INVALID);
            }
            return (r.to_bits(), 0);
        }
        let p = f64::from(x) * f64::from(y); // exact: 24+24 <= 53 bits
        let rm = self.cpu.fcr_rounding_mode();
        let out = if rm == crate::cpu::FCR_RM_NEAREST {
            (x * y).to_bits()
        } else {
            debug_assert!(*self.fp_policy.fcr_rounding_honored.get());
            round_f64_residual_to_f32(p, 0, rm)
        };
        (out, Self::round_flags(out, p, false))
    }

    /// `madd.s` (`acc + x*y`) and `msub.s` (`acc - x*y`): fused, one rounding
    /// (ISA RM p. 406, confirmed on silicon by the F2/probe detector rows).
    /// `flags` distinguishes `madd.s` (full flags) from `maddn.s` (silent).
    fn fp_madd(&self, acc: u32, x: u32, y: u32, subtract: bool, flags: bool) -> FpOut {
        self.require_rne(if flags {
            "madd.s/msub.s"
        } else {
            "maddn.s/divn.s"
        });
        if is_nan(acc) || is_nan(x) || is_nan(y) {
            // INVALID for a signalling operand — or for a 0 * inf product,
            // which silicon flags even while a quiet NaN accumulator
            // propagates (measured: madd.s(qNaN, -0, +inf) reads FSR 0x800).
            let zero_times_inf =
                (x & !0x8000_0000 == 0 && y & 0x7F80_0000 == 0x7F80_0000 && y & 0x007F_FFFF == 0)
                    || (y & !0x8000_0000 == 0
                        && x & 0x7F80_0000 == 0x7F80_0000
                        && x & 0x007F_FFFF == 0);
            let fsr = if flags && (is_snan(acc) || is_snan(x) || is_snan(y) || zero_times_inf) {
                FSR_INVALID
            } else {
                0
            };
            // Operand priority, measured: the ACCUMULATOR (fr) beats ft,
            // which beats fs. F2 pins ft > fs (acc staged non-NaN there);
            // the accumulator's precedence is pinned by the qNaN/qNaN divide
            // sequence, whose maddn chain only reproduces silicon's +NaN
            // answer under acc-first. Encoded as "last NaN in (fs, ft, fr)
            // order" for the shared rule.
            return (self.fp_nan_out(&[x, y, acc]), fsr);
        }
        let (a, mut p, q) = (f32::from_bits(acc), f32::from_bits(x), f32::from_bits(y));
        if subtract {
            p = -p;
        }
        debug_assert!(*self.fp_policy.madd_fused.get());
        let r = p.mul_add(q, a);
        if r.is_nan() {
            // inf*0 + acc, or inf - inf against the accumulator.
            let fsr = if flags { FSR_INVALID } else { 0 };
            return (*self.fp_policy.default_generated_nan.get(), fsr);
        }
        if a.is_infinite() || p.is_infinite() || q.is_infinite() {
            // An exact infinity, in every rounding mode.
            return (r.to_bits(), 0);
        }
        // Exact value: the f64 product is exact; two-sum against the addend
        // captures the exact tail.
        let (head, residual) = two_sum(f64::from(p) * f64::from(q), f64::from(a));
        let fsr = if flags {
            Self::round_flags(r.to_bits(), head, residual != 0.0)
        } else {
            0
        };
        (r.to_bits(), fsr)
    }

    /// `2^imm` as an f32 — exact for `imm <= 15`.
    fn scale_factor(&self, imm: u8) -> f32 {
        if imm == 0 {
            return 1.0;
        }
        // Only consulted for imm != 0; the rule is manual-sourced and
        // silicon-confirmed (FractionalBits).
        let _ = self.fp_policy.conversion_scale.get();
        f32::from_bits((127u32 + u32::from(imm)) << 23)
    }

    /// `trunc.s` / `utrunc.s` / `round.s` / `floor.s` / `ceil.s`.
    ///
    /// Boundaries per the ISA RM's per-instruction pages, confirmed on
    /// silicon — except `utrunc.s` on in-range negatives, where silicon
    /// **falsified** the manual: the result wraps like the signed conversion
    /// (16 DIVERGE rows, family F6), saturating only below `i32::MIN`.
    /// Flags measured: INVALID for NaN / out-of-range / negative `utrunc.s`
    /// wraps (suppressing INEXACT); INEXACT alone when rounding dropped bits.
    fn fp_to_int(&self, op: FpToIntOp, bits: u32, imm: u8) -> FpOut {
        self.require_rne("the float->int conversions");
        let x = f32::from_bits(bits);
        let unsigned = matches!(op, FpToIntOp::UtruncS);

        if x.is_nan() {
            let v = if unsigned {
                U32_SATURATE_MAX
            } else {
                *self.fp_policy.float_to_int_nan.get()
            };
            return (v, FSR_INVALID);
        }
        // Scale in f64: exact for every f32 times 2^0..=15.
        let v = f64::from(x) * f64::from(self.scale_factor(imm));

        let rounded: f64 = match op {
            FpToIntOp::TruncS | FpToIntOp::UtruncS => v.trunc(),
            FpToIntOp::FloorS => v.floor(),
            FpToIntOp::CeilS => v.ceil(),
            FpToIntOp::RoundS => {
                // Measured (family F6): ties to even.
                match *self.fp_policy.round_s_ties.get() {
                    TiesRule::ToEven => round_ties_even_f64(v),
                    TiesRule::AwayFromZero => v.round(),
                }
            }
        };
        let inexact = if rounded == v { 0 } else { FSR_INEXACT };

        if unsigned {
            if rounded >= 4_294_967_296.0 {
                return (U32_SATURATE_MAX, FSR_INVALID);
            }
            if rounded >= 0.0 {
                // Includes the in-range-negative-truncating-to-zero case
                // (-0.5 -> 0), which is inexact but NOT invalid (measured).
                return (rounded as u32, inexact);
            }
            // Negative: measured silicon behavior — wrap like the signed
            // conversion, saturate below i32::MIN, INVALID either way.
            match *self.fp_policy.utrunc_negative.get() {
                UtruncNegativeRule::WrapLikeSignedSaturating => {
                    if rounded < -2_147_483_648.0 {
                        return (I32_SATURATE_MIN, FSR_INVALID);
                    }
                    return (rounded as i32 as u32, FSR_INVALID);
                }
                UtruncNegativeRule::Sentinel(v) => return (v, FSR_INVALID),
            }
        }

        if rounded >= 2_147_483_648.0 || rounded < -2_147_483_648.0 {
            let v = out_of_range(
                rounded,
                *self.fp_policy.float_to_int_out_of_range.get(),
                I32_SATURATE_MIN,
                I32_SATURATE_MAX,
            );
            return (v, FSR_INVALID);
        }
        (rounded as i32 as u32, inexact)
    }
}

/// Knuth two-sum: `a + b == head + err` exactly, for any two f64 values.
fn two_sum(a: f64, b: f64) -> (f64, f64) {
    let s = a + b;
    let bb = s - a;
    let err = (a - (s - bb)) + (b - bb);
    (s, err)
}

fn residual_dir(err: f64) -> i32 {
    if err > 0.0 {
        1
    } else if err < 0.0 {
        -1
    } else {
        0
    }
}

/// Round the exact value `head + tiny*dir` to f32 under a directed rounding
/// mode, where `head` is an f64 and `dir` marks an infinitesimal residual
/// (`|residual| < ulp64(head)/2`, so it only matters when `head` sits exactly
/// on a target grid point).
///
/// The full range is implemented — subnormal results, overflow to the
/// largest finite (down-in-magnitude) or infinity (up-in-magnitude) — because
/// family F1's exponent spread reaches all of it and silicon matched IEEE
/// directed rounding on every one of the 1944 rows.
fn round_f64_residual_to_f32(head: f64, dir: i32, rm: u32) -> u32 {
    assert!(head.is_finite(), "directed rounding of a non-finite head");
    let bits = head.to_bits();
    let sign = (bits >> 63) as u32;
    let down_in_magnitude = match rm {
        crate::cpu::FCR_RM_TOWARD_ZERO => true,
        crate::cpu::FCR_RM_TOWARD_POS_INF => sign == 1,
        crate::cpu::FCR_RM_TOWARD_NEG_INF => sign == 0,
        _ => unreachable!("directed modes only"),
    };
    // Residual direction in *magnitude* terms.
    let mdir = if sign == 1 { -dir } else { dir };

    if head == 0.0 {
        // ±0, possibly with a residual pointing off zero.
        if mdir > 0 && !down_in_magnitude {
            return (sign << 31) | 1; // up to the smallest subnormal
        }
        return sign << 31;
    }

    let e2 = ((bits >> 52) & 0x7FF) as i32 - 1023;
    let m = (bits & 0xF_FFFF_FFFF_FFFF) | (1 << 52);
    let e32 = e2 + 127;

    // How many mantissa bits fall below the target grid: 29 for a normal
    // result, more as the result goes subnormal.
    let drop = if e32 >= 1 { 29 } else { 29 + (1 - e32) };
    if e32 >= 255 || drop >= 64 {
        return round_extreme(sign, e32 >= 255, down_in_magnitude, mdir);
    }
    let kept = (m >> drop) as u32;
    let dropped = m & ((1u64 << drop) - 1);
    let below = dropped == 0 && mdir < 0;
    let above = dropped != 0 || mdir > 0;

    if e32 >= 1 {
        let (mut sig, mut e32) = (kept, e32);
        if down_in_magnitude {
            if below {
                sig -= 1;
                if sig < (1 << 23) {
                    sig = (1 << 24) - 1;
                    e32 -= 1;
                }
            }
        } else if above {
            sig += 1;
            if sig == (1 << 24) {
                sig = 1 << 23;
                e32 += 1;
            }
        }
        if e32 >= 255 {
            return round_extreme(sign, true, down_in_magnitude, mdir);
        }
        if e32 >= 1 {
            return (sign << 31) | ((e32 as u32) << 23) | (sig & FRAC);
        }
        // Stepping down from the smallest normal lands on the largest
        // subnormal.
        return (sign << 31) | sig;
    }

    // Subnormal grid: `kept` already is the raw subnormal significand, and
    // `kept + 1` reaching 2^23 is exactly the smallest normal.
    let mut sig = kept;
    if down_in_magnitude {
        if below {
            sig -= 1;
        }
    } else if above {
        sig += 1;
    }
    (sign << 31) | sig
}

/// Overflow (`huge`) or total-underflow endpoints of directed rounding.
fn round_extreme(sign: u32, huge: bool, down_in_magnitude: bool, mdir: i32) -> u32 {
    if huge {
        if down_in_magnitude {
            (sign << 31) | 0x7F7F_FFFF // largest finite
        } else {
            (sign << 31) | EXP // infinity
        }
    } else {
        // A nonzero value entirely below the subnormal grid: truncation gives
        // zero, rounding away from zero gives the smallest subnormal.
        let _ = mdir; // the value itself is already strictly off zero
        if down_in_magnitude {
            sign << 31
        } else {
            (sign << 31) | 1
        }
    }
}

/// The signed conversions' negative-overflow answer (`32'h80000000`).
const I32_SATURATE_MIN: u32 = 0x8000_0000;
/// The signed conversions' positive-overflow and NaN answer (`32'h7fffffff`).
const I32_SATURATE_MAX: u32 = 0x7FFF_FFFF;
/// `utrunc.s`'s positive-overflow and NaN answer (`32'hffffffff`).
const U32_SATURATE_MAX: u32 = 0xFFFF_FFFF;

/// Round half to even on the scaled f64 value.
fn round_ties_even_f64(x: f64) -> f64 {
    let lo = x.floor();
    if x - lo == 0.5 {
        if (lo as i64) % 2 == 0 { lo } else { lo + 1.0 }
    } else {
        x.round()
    }
}

/// Resolve a float→int conversion that cannot be represented.
fn out_of_range(r: f64, rule: OutOfRangeRule, min: u32, max: u32) -> u32 {
    match rule {
        OutOfRangeRule::Saturate => {
            if r.is_sign_negative() {
                min
            } else {
                max
            }
        }
        OutOfRangeRule::Wrap => r as i64 as u32,
    }
}

#[cfg(test)]
mod tests {
    use lp_xt_inst::{BReg, FReg, FpCmpOp, FpRrOp, FpRrrOp, Inst, Reg};

    use crate::cpu::{CPENABLE_FPU, FSR_INEXACT, FSR_INVALID, FSR_UNDERFLOW};
    use crate::emu::Emulator;
    use crate::trace::NoopTracer;

    fn armed() -> Emulator {
        let mut e = Emulator::new();
        e.cpu.cpenable = CPENABLE_FPU;
        e
    }

    fn exec(emu: &mut Emulator, inst: &Inst) {
        let mut t = NoopTracer;
        emu.execute(inst, 0x4000_0100, &mut t).expect("no trap");
    }

    /// A tiny deterministic bit generator, so the agreement test below is a
    /// broad randomized sweep rather than a handful of cases — and reproducible.
    fn xorshift(state: &mut u32) -> u32 {
        let mut x = *state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        *state = x;
        x
    }

    /// The oracle for the space IEEE fixes: for non-NaN operands,
    /// `add.s`/`sub.s`/`mul.s` must equal Rust's `f32` bit for bit —
    /// **including subnormal operands and results**, because the campaign
    /// measured no flushing anywhere (family F3).
    #[test]
    fn ieee_core_agrees_with_host_f32_bit_for_bit_including_subnormals() {
        let mut emu = armed();
        let mut state = 0x1234_5678u32;
        let mut checked = 0;
        for _ in 0..20_000 {
            let a = xorshift(&mut state);
            let b = xorshift(&mut state);
            let (fa, fb) = (f32::from_bits(a), f32::from_bits(b));
            if fa.is_nan() || fb.is_nan() {
                continue;
            }
            for (op, expect) in [
                (FpRrrOp::AddS, fa + fb),
                (FpRrrOp::SubS, fa - fb),
                (FpRrrOp::MulS, fa * fb),
            ] {
                if expect.is_nan() {
                    continue; // generated NaNs carry the measured pattern
                }
                emu.cpu.set_f(1, a);
                emu.cpu.set_f(2, b);
                exec(
                    &mut emu,
                    &Inst::FpRrr(op, FReg::new(0), FReg::new(1), FReg::new(2)),
                );
                assert_eq!(
                    emu.cpu.f(0),
                    expect.to_bits(),
                    "{op:?} on {a:#010x}, {b:#010x}"
                );
                checked += 1;
            }
        }
        assert!(checked > 20_000, "the sweep must be broad, got {checked}");
    }

    #[test]
    fn abs_and_neg_are_sign_bit_operations() {
        let mut emu = armed();
        const SNAN: u32 = 0x7F80_1234;
        for (op, input, expect) in [
            (FpRrOp::AbsS, SNAN, SNAN),
            (FpRrOp::NegS, SNAN, SNAN | 0x8000_0000),
            (FpRrOp::AbsS, SNAN | 0x8000_0000, SNAN),
            (FpRrOp::NegS, 0x0000_0000, 0x8000_0000), // -(+0) == -0
            (FpRrOp::NegS, 0x8000_0000, 0x0000_0000),
            (FpRrOp::AbsS, 0x8000_0000, 0x0000_0000),
        ] {
            emu.cpu.set_f(1, input);
            exec(&mut emu, &Inst::FpRr(op, FReg::new(0), FReg::new(1)));
            assert_eq!(emu.cpu.f(0), expect, "{op:?} on {input:#010x}");
        }
    }

    /// The full ordered/unordered matrix, including both NaN kinds.
    #[test]
    fn compares_cover_the_ordered_and_unordered_matrix() {
        const QNAN: u32 = 0x7FC0_0000;
        const SNAN: u32 = 0x7F80_0001;
        let one = 1.0f32.to_bits();
        let two = 2.0f32.to_bits();
        let pos0 = 0u32;
        let neg0 = 0x8000_0000u32;
        let cases: &[(FpCmpOp, u32, u32, bool)] = &[
            (FpCmpOp::OeqS, one, one, true),
            (FpCmpOp::OeqS, one, two, false),
            // +0 == -0 is true; `float.md` §3 makes it Guaranteed.
            (FpCmpOp::OeqS, pos0, neg0, true),
            (FpCmpOp::OltS, one, two, true),
            (FpCmpOp::OltS, two, one, false),
            (FpCmpOp::OleS, one, one, true),
            (FpCmpOp::UnS, one, two, false),
            (FpCmpOp::UnS, QNAN, one, true),
            (FpCmpOp::UnS, one, SNAN, true),
            (FpCmpOp::OeqS, QNAN, one, false),
            (FpCmpOp::OeqS, one, SNAN, false),
            (FpCmpOp::OltS, QNAN, one, false),
            (FpCmpOp::OleS, SNAN, one, false),
            (FpCmpOp::UeqS, QNAN, one, true),
            (FpCmpOp::UltS, one, QNAN, true),
            (FpCmpOp::UleS, SNAN, two, true),
        ];
        let mut emu = armed();
        for &(op, a, b, want) in cases {
            emu.cpu.set_f(1, a);
            emu.cpu.set_f(2, b);
            emu.cpu.set_b(3, !want); // prove the write happened
            exec(
                &mut emu,
                &Inst::FpCmp(op, BReg::new(3), FReg::new(1), FReg::new(2)),
            );
            assert_eq!(emu.cpu.b(3), want, "{op:?} on {a:#010x}, {b:#010x}");
        }
    }

    /// The measured compare-flag rule: sNaN raises INVALID on any compare,
    /// and the "less" ordered predicates raise it on quiet NaNs too.
    #[test]
    fn compare_flags_follow_the_measured_signaling_rule() {
        const QNAN: u32 = 0x7FC0_0000;
        let one = 1.0f32.to_bits();
        for (op, a, want_invalid) in [
            (FpCmpOp::OeqS, QNAN, false),
            (FpCmpOp::UeqS, QNAN, false),
            (FpCmpOp::UnS, QNAN, false),
            (FpCmpOp::OltS, QNAN, true),
            (FpCmpOp::OleS, QNAN, true),
            (FpCmpOp::UltS, QNAN, false),
            (FpCmpOp::OeqS, 0x7F80_0001, true), // sNaN signals everywhere
            (FpCmpOp::UnS, 0x7F80_0001, true),
        ] {
            let mut emu = armed();
            emu.cpu.set_f(1, a);
            emu.cpu.set_f(2, one);
            exec(
                &mut emu,
                &Inst::FpCmp(op, BReg::new(3), FReg::new(1), FReg::new(2)),
            );
            assert_eq!(
                emu.cpu.fsr & FSR_INVALID != 0,
                want_invalid,
                "{op:?} on {a:#010x}"
            );
        }
    }

    /// The measured NaN rule (family F2): last NaN operand, quieted, payload
    /// preserved; generated NaNs are the canonical quiet NaN with INVALID.
    #[test]
    fn nan_propagation_is_last_operand_quieted() {
        const A: u32 = 0x7FD5_AA55; // qNaN, distinctive payload
        const B: u32 = 0x7FA5_A5A5; // sNaN, distinctive payload
        let mut emu = armed();
        // both NaN: the SECOND (ft) survives, quieted.
        emu.cpu.set_f(1, A);
        emu.cpu.set_f(2, B);
        exec(
            &mut emu,
            &Inst::FpRrr(FpRrrOp::AddS, FReg::new(0), FReg::new(1), FReg::new(2)),
        );
        assert_eq!(emu.cpu.f(0), B | 0x0040_0000);
        assert!(
            emu.cpu.fsr & FSR_INVALID != 0,
            "sNaN operand raises INVALID"
        );

        // only fs NaN: it survives quieted, and a quiet NaN raises nothing.
        let mut emu = armed();
        emu.cpu.set_f(1, A);
        emu.cpu.set_f(2, 1.0f32.to_bits());
        exec(
            &mut emu,
            &Inst::FpRrr(FpRrrOp::MulS, FReg::new(0), FReg::new(1), FReg::new(2)),
        );
        assert_eq!(emu.cpu.f(0), A);
        assert_eq!(emu.cpu.fsr, 0, "quiet NaN propagation raises nothing");

        // generated: inf - inf.
        let mut emu = armed();
        emu.cpu.set_f(1, 0x7F80_0000);
        emu.cpu.set_f(2, 0x7F80_0000);
        exec(
            &mut emu,
            &Inst::FpRrr(FpRrrOp::SubS, FReg::new(0), FReg::new(1), FReg::new(2)),
        );
        assert_eq!(emu.cpu.f(0), 0x7FC0_0000);
        assert!(emu.cpu.fsr & FSR_INVALID != 0);
    }

    /// FCR.RM is honored (measured, F1): the directed modes produce IEEE
    /// directed rounding for add/sub/mul.
    #[test]
    fn directed_rounding_matches_ieee_on_the_f1_shape() {
        // 1.0 + half-an-ulp: RNE ties to even (1.0), toward+inf rounds up,
        // toward-zero and toward-neg-inf round down.
        let a = 1.0f32.to_bits();
        let tie = 0x3380_0000u32; // 2^-24
        for (rm, want) in [
            (0u32, 0x3F80_0000u32),
            (1, 0x3F80_0000),
            (2, 0x3F80_0001),
            (3, 0x3F80_0000),
        ] {
            let mut emu = armed();
            emu.cpu.fcr = rm;
            emu.cpu.set_f(1, a);
            emu.cpu.set_f(2, tie);
            exec(
                &mut emu,
                &Inst::FpRrr(FpRrrOp::AddS, FReg::new(0), FReg::new(1), FReg::new(2)),
            );
            assert_eq!(emu.cpu.f(0), want, "rm={rm}");
            assert!(emu.cpu.fsr & FSR_INEXACT != 0);
        }
        // The negative mirror.
        for (rm, want) in [(2u32, 0xBF80_0000u32), (3, 0xBF80_0001), (1, 0xBF80_0000)] {
            let mut emu = armed();
            emu.cpu.fcr = rm;
            emu.cpu.set_f(1, a | 0x8000_0000);
            emu.cpu.set_f(2, tie | 0x8000_0000);
            exec(
                &mut emu,
                &Inst::FpRrr(FpRrrOp::AddS, FReg::new(0), FReg::new(1), FReg::new(2)),
            );
            assert_eq!(emu.cpu.f(0), want, "rm={rm} negative");
        }
    }

    /// Subnormal arithmetic is real (measured, F3): no flush on input, no
    /// flush on output, and underflow flags on tiny inexact results.
    #[test]
    fn subnormals_are_ieee_with_underflow_flags() {
        let mut emu = armed();
        // max subnormal + max subnormal = a normal number, exactly.
        emu.cpu.set_f(1, 0x007F_FFFF);
        emu.cpu.set_f(2, 0x007F_FFFF);
        exec(
            &mut emu,
            &Inst::FpRrr(FpRrrOp::AddS, FReg::new(0), FReg::new(1), FReg::new(2)),
        );
        assert_eq!(emu.cpu.f(0), 0x00FF_FFFE, "no input flush");
        assert_eq!(emu.cpu.fsr, 0, "exact: no flags");

        // min-normal * 0.5 = subnormal, exactly: no flags.
        let mut emu = armed();
        emu.cpu.set_f(1, 0x0080_0000);
        emu.cpu.set_f(2, 0.5f32.to_bits());
        exec(
            &mut emu,
            &Inst::FpRrr(FpRrrOp::MulS, FReg::new(0), FReg::new(1), FReg::new(2)),
        );
        assert_eq!(emu.cpu.f(0), 0x0040_0000, "no output flush");
        assert_eq!(emu.cpu.fsr, 0);

        // (1+ulp) * min-subnormal: inexact and tiny -> INEXACT | UNDERFLOW.
        let mut emu = armed();
        emu.cpu.set_f(1, 0x3F80_0001);
        emu.cpu.set_f(2, 0x0000_0001);
        exec(
            &mut emu,
            &Inst::FpRrr(FpRrrOp::MulS, FReg::new(0), FReg::new(1), FReg::new(2)),
        );
        assert_eq!(emu.cpu.f(0), 0x0000_0001);
        assert_eq!(emu.cpu.fsr, FSR_INEXACT | FSR_UNDERFLOW);
    }

    /// Conversions at `imm = 0` on in-range values, and the measured
    /// tie-to-even for round.s.
    #[test]
    fn conversions_follow_measured_semantics() {
        use lp_xt_inst::{FpToIntOp, IntToFpOp};
        let mut emu = armed();

        for (v, want) in [
            (0i32, 0.0f32),
            (1, 1.0),
            (-1, -1.0),
            (i32::MIN, -2147483648.0),
        ] {
            emu.cpu.set_a(2, v as u32);
            exec(
                &mut emu,
                &Inst::IntToFp(IntToFpOp::FloatS, FReg::new(0), Reg::new(2), 0),
            );
            assert_eq!(emu.cpu.f(0), want.to_bits(), "float.s of {v}");
        }
        emu.cpu.set_a(2, 16_777_217u32);
        emu.cpu.fsr = 0;
        exec(
            &mut emu,
            &Inst::IntToFp(IntToFpOp::FloatS, FReg::new(0), Reg::new(2), 0),
        );
        assert_eq!(emu.cpu.f(0), (16_777_217i32 as f32).to_bits());
        assert_eq!(emu.cpu.fsr, FSR_INEXACT, "2^24+1 rounds");

        for (v, want) in [
            (1.9f32, 1i32),
            (-1.9, -1),
            (0.0, 0),
            (2_147_483_520.0, 2_147_483_520),
        ] {
            emu.cpu.set_f(1, v.to_bits());
            exec(
                &mut emu,
                &Inst::FpToInt(FpToIntOp::TruncS, Reg::new(3), FReg::new(1), 0),
            );
            assert_eq!(emu.cpu.a(3) as i32, want, "trunc.s of {v}");
        }

        // round.s ties to even — measured, family F6.
        for (v, want) in [(0.5f32, 0i32), (1.5, 2), (2.5, 2), (-2.5, -2), (-0.5, 0)] {
            emu.cpu.set_f(1, v.to_bits());
            exec(
                &mut emu,
                &Inst::FpToInt(FpToIntOp::RoundS, Reg::new(3), FReg::new(1), 0),
            );
            assert_eq!(emu.cpu.a(3) as i32, want, "round.s of {v}");
        }
    }

    /// The RM-falsifying finding: `utrunc.s` of an in-range negative wraps
    /// like the signed conversion (INVALID, no INEXACT), saturates below
    /// `i32::MIN`, and a negative truncating to zero is only INEXACT.
    #[test]
    fn utrunc_negative_wraps_like_signed_as_measured() {
        use lp_xt_inst::FpToIntOp;
        for (v, want, want_fsr) in [
            (-0.5f32, 0u32, FSR_INEXACT),
            (-1.5, 0xFFFF_FFFF, FSR_INVALID),
            (-2.5, 0xFFFF_FFFE, FSR_INVALID),
            (-1e30, 0x8000_0000, FSR_INVALID),
            (f32::NEG_INFINITY, 0x8000_0000, FSR_INVALID),
            (-0.0, 0, 0),
            (7.9, 7, FSR_INEXACT),
            (f32::NAN, 0xFFFF_FFFF, FSR_INVALID),
            (f32::INFINITY, 0xFFFF_FFFF, FSR_INVALID),
        ] {
            let mut emu = armed();
            emu.cpu.set_f(1, v.to_bits());
            exec(
                &mut emu,
                &Inst::FpToInt(FpToIntOp::UtruncS, Reg::new(3), FReg::new(1), 0),
            );
            assert_eq!(emu.cpu.a(3), want, "utrunc.s of {v}");
            assert_eq!(emu.cpu.fsr, want_fsr, "utrunc.s flags of {v}");
        }
    }

    /// `madd.s` rounds once (fused), and `maddn.s` computes the same value
    /// while staying flag-silent — both measured.
    #[test]
    fn madd_is_fused_and_maddn_is_its_flag_silent_twin() {
        let x = f32::from_bits(0x3F80_0800); // 1 + 2^-12
        let a = f32::from_bits(0xBF80_1000); // -(1 + 2^-11)
        let fused = x.mul_add(x, a);
        assert_eq!(fused.to_bits(), 0x3380_0000, "fused answer is 2^-24");

        let mut emu = armed();
        emu.cpu.set_f(0, a.to_bits());
        emu.cpu.set_f(1, x.to_bits());
        emu.cpu.set_f(2, x.to_bits());
        exec(
            &mut emu,
            &Inst::FpRrr(FpRrrOp::MaddS, FReg::new(0), FReg::new(1), FReg::new(2)),
        );
        assert_eq!(emu.cpu.f(0), fused.to_bits());
        // The fused answer is EXACT (that is the point of the detector: the
        // unfused path would have rounded), so no INEXACT flag.
        assert_eq!(emu.cpu.fsr, 0, "the fused result is exact");

        // A genuinely inexact madd flags its rounding.
        let mut emu = armed();
        emu.cpu.set_f(0, 1.0f32.to_bits());
        emu.cpu.set_f(1, std::f32::consts::PI.to_bits());
        emu.cpu.set_f(2, std::f32::consts::PI.to_bits());
        exec(
            &mut emu,
            &Inst::FpRrr(FpRrrOp::MaddS, FReg::new(0), FReg::new(1), FReg::new(2)),
        );
        assert!(emu.cpu.fsr & FSR_INEXACT != 0, "madd.s flags its rounding");

        let mut emu = armed();
        emu.cpu.set_f(0, a.to_bits());
        emu.cpu.set_f(1, x.to_bits());
        emu.cpu.set_f(2, x.to_bits());
        exec(
            &mut emu,
            &Inst::FpRrr(FpRrrOp::MaddnS, FReg::new(0), FReg::new(1), FReg::new(2)),
        );
        assert_eq!(emu.cpu.f(0), fused.to_bits(), "maddn.s == madd.s");
        assert_eq!(emu.cpu.fsr, 0, "maddn.s never sets a flag (measured)");
    }

    #[test]
    fn conditional_moves_are_bit_copies_predicated_on_ar_and_br() {
        use lp_xt_inst::{FpMovArOp, FpMovBrOp};
        let mut emu = armed();
        const PAY: u32 = 0x7F80_ABCD;
        emu.cpu.set_f(1, PAY);
        emu.cpu.set_a(2, 0);
        exec(
            &mut emu,
            &Inst::FpMovAr(FpMovArOp::MoveqzS, FReg::new(0), FReg::new(1), Reg::new(2)),
        );
        assert_eq!(emu.cpu.f(0), PAY);

        emu.cpu.set_f(3, 0);
        exec(
            &mut emu,
            &Inst::FpMovAr(FpMovArOp::MovnezS, FReg::new(3), FReg::new(1), Reg::new(2)),
        );
        assert_eq!(emu.cpu.f(3), 0, "movnez.s on zero does not move");

        emu.cpu.set_b(5, true);
        emu.cpu.set_f(4, 0);
        exec(
            &mut emu,
            &Inst::FpMovBr(FpMovBrOp::MovtS, FReg::new(4), FReg::new(1), BReg::new(5)),
        );
        assert_eq!(emu.cpu.f(4), PAY, "movt.s on a set bit moves");
        emu.cpu.set_f(6, 0);
        exec(
            &mut emu,
            &Inst::FpMovBr(FpMovBrOp::MovfS, FReg::new(6), FReg::new(1), BReg::new(5)),
        );
        assert_eq!(emu.cpu.f(6), 0, "movf.s on a set bit does not move");
    }

    /// The measured `const.s` table: `[0.0, 1.0, 2.0, 0.5]` selected by
    /// `imm & 3`.
    #[test]
    fn const_s_produces_the_measured_table() {
        let mut emu = armed();
        for (imm, want) in [
            (0u8, 0u32),
            (1, 0x3F80_0000),
            (2, 0x4000_0000),
            (3, 0x3F00_0000),
            (5, 0x3F80_0000),
            (15, 0x3F00_0000),
        ] {
            exec(&mut emu, &Inst::ConstS(FReg::new(0), imm));
            assert_eq!(emu.cpu.f(0), want, "const.s {imm}");
        }
    }

    /// Arithmetic is still behind the coprocessor gate.
    #[test]
    fn arithmetic_is_gated_by_cpenable() {
        let mut emu = Emulator::new();
        let mut t = NoopTracer;
        let trap = emu
            .execute(
                &Inst::FpRrr(FpRrrOp::AddS, FReg::new(0), FReg::new(1), FReg::new(2)),
                0x4000_0100,
                &mut t,
            )
            .expect_err("unarmed FP arithmetic must trap");
        assert_eq!(trap.cause, crate::error::EXC_COPROCESSOR0_DISABLED);
    }
}
