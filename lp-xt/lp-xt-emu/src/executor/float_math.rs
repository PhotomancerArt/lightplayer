//! Everything that computes a float value: arithmetic, compares, conversions,
//! the conditional moves, and the divide/sqrt helper family.
//!
//! # What is fixed and what is policy
//!
//! IEEE-754 binary32 under round-to-nearest-even fixes the *value* of `add.s`,
//! `sub.s`, and `mul.s` for every pair of operands that is normal, finite, and
//! non-zero, and Rust's `f32` is exactly that arithmetic — so for that part of
//! the space this module is a thin wrapper over `f32` and is bit-exact by
//! construction. IEEE also fixes the seven compare predicates completely,
//! including their unordered cases, and makes `abs.s`/`neg.s` sign-bit
//! operations that never touch a payload.
//!
//! Everything else goes through [`crate::fp_policy::FpPolicy`]: **which** NaN
//! propagates, which NaN gets generated, whether denormals flush at the input or
//! at the output, whether `madd.s` rounds once or twice, what a conversion does
//! outside range or on a NaN, how `round.s` breaks a tie, what `const.s`
//! produces, and what the estimate instructions return. Those fields are
//! [`crate::fp_policy::Unknown`] today and **reading one panics**. An unmeasured
//! corner is a question for silicon, not a default to assume: an emulator that
//! picked a plausible answer for each of them would look identical, pass its
//! tests, and silently mislead M7, M8, and M9.
//!
//! # Divide and square root
//!
//! There is no divide instruction and no sqrt instruction. Both are code
//! sequences over `div0.s`, `nexp01.s`, `const.s`, `maddn.s`, `mkdadj.s`,
//! `divn.s`, `recip0.s`, `rsqrt0.s`, `sqrt0.s` — all nine of which exist on the
//! S3 (M6 P1, measured; escalation A1 retired). Two different problems:
//!
//! - `recip0.s`, `rsqrt0.s`, `sqrt0.s`, `div0.s` return **implementation-defined
//!   estimates from a lookup ROM**. No document yields their contents. They sit
//!   behind [`crate::fp_policy::EstimateTables`], which P6 extracts
//!   exhaustively so they become exact *by construction*. There is deliberately
//!   no polynomial placeholder: an approximation that came close would pass
//!   casual tests while hiding that the real table was never captured.
//! - `nexp01.s`, `mkdadj.s`, `addexp.s`, `addexpm.s`, `maddn.s`, `divn.s` were
//!   assumed to be *architecturally* defined. M6 P5 read the ISA Reference
//!   Manual and found that they are not in it: its Table 4-46 (p. 67-68)
//!   enumerates the Floating-Point Coprocessor Option's instructions and the
//!   whole helper family is absent, as are the four estimates and `const.s`.
//!   They stay [`crate::fp_policy::DivideStepSemantics`] — a P6 measurement, not
//!   a document lookup — and they fail loudly meanwhile.
//!
//! # Rounding mode
//!
//! `docs/design/float.md` §2 makes round-to-nearest-even the only mode shader
//! code ever runs under, and P1 measured `FCR = 0` at reset. So a non-default
//! `FCR.RM` is **refused loudly** rather than silently ignored (D6); whether
//! silicon even honors the field is
//! [`crate::fp_policy::FpPolicy::fcr_rounding_honored`], and G2 decides whether
//! to implement the other three modes. The field's *encoding* is architectural
//! and lives in `cpu.rs` as `FCR_RM_*`.
//!
//! # Provenance
//!
//! Semantics come from IEEE-754 and from the Xtensa ISA Reference Manual, cited
//! by section and page at each use. No manual text is reproduced, and no QEMU,
//! binutils, or GCC source was read or adapted.

use lp_xt_inst::{FpCmpOp, FpMovArOp, FpMovBrOp, FpRrOp, FpRrrOp, FpToIntOp, Inst, IntToFpOp};

use crate::emu::{Emulator, Flow};
use crate::error::Trap;
use crate::fp_policy::{NanRule, OutOfRangeRule, ScaleRule, TiesRule};
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

/// A non-zero subnormal: exponent zero, significand non-zero.
#[inline]
fn is_subnormal(bits: u32) -> bool {
    bits & EXP == 0 && bits & FRAC != 0
}

impl Emulator {
    pub(super) fn exec_float_math(
        &mut self,
        inst: &Inst,
        tracer: &mut dyn Tracer,
    ) -> Result<Flow, Trap> {
        self.require_fpu()?;
        self.refuse_non_default_rounding();

        match *inst {
            Inst::FpRrr(op, fr, fs, ft) => {
                let (a, b) = (self.rfreg(fs.num()), self.rfreg(ft.num()));
                let out = match op {
                    FpRrrOp::AddS => self.fp_binop(a, b, |x, y| x + y),
                    FpRrrOp::SubS => self.fp_binop(a, b, |x, y| x - y),
                    FpRrrOp::MulS => self.fp_binop(a, b, |x, y| x * y),
                    // madd/msub accumulate INTO fr, so it is a third operand.
                    FpRrrOp::MaddS => self.fp_madd(self.rfreg(fr.num()), a, b, false),
                    FpRrrOp::MsubS => self.fp_madd(self.rfreg(fr.num()), a, b, true),
                    // Divide-sequence steps: architecturally defined, not
                    // sourced here. Fails loudly rather than approximating.
                    FpRrrOp::MaddnS => {
                        divide_step("maddn.s", self.fp_policy.divide_step_helpers.get())
                    }
                    FpRrrOp::DivnS => {
                        divide_step("divn.s", self.fp_policy.divide_step_helpers.get())
                    }
                };
                self.wfreg(fr.num(), out, tracer);
            }

            Inst::FpRr(op, fr, fs) => {
                let a = self.rfreg(fs.num());
                let out = match op {
                    // Sign-bit operations, not `f32::abs()` / `-x`: they must
                    // not canonicalize a NaN and must give the right signed
                    // zero. `float.md` §3 makes both Guaranteed.
                    FpRrOp::AbsS => a & !SIGN,
                    FpRrOp::NegS => a ^ SIGN,
                    // Implementation-defined lookup ROMs (D5).
                    FpRrOp::Recip0S => estimate("recip0.s", self.fp_policy.estimates.get()),
                    FpRrOp::Sqrt0S => estimate("sqrt0.s", self.fp_policy.estimates.get()),
                    FpRrOp::Rsqrt0S => estimate("rsqrt0.s", self.fp_policy.estimates.get()),
                    FpRrOp::Div0S => estimate("div0.s", self.fp_policy.estimates.get()),
                    FpRrOp::Nexp01S => {
                        divide_step("nexp01.s", self.fp_policy.divide_step_helpers.get())
                    }
                    FpRrOp::MksadjS => {
                        divide_step("mksadj.s", self.fp_policy.divide_step_helpers.get())
                    }
                    FpRrOp::MkdadjS => {
                        divide_step("mkdadj.s", self.fp_policy.divide_step_helpers.get())
                    }
                    FpRrOp::AddexpS => {
                        divide_step("addexp.s", self.fp_policy.divide_step_helpers.get())
                    }
                    FpRrOp::AddexpmS => {
                        divide_step("addexpm.s", self.fp_policy.divide_step_helpers.get())
                    }
                    // `mov.s` is data movement and lives in `float.rs`.
                    FpRrOp::MovS => unreachable!("mov.s is handled by exec_float"),
                };
                self.wfreg(fr.num(), out, tracer);
            }

            // Sixteen architecturally-defined constants that this environment
            // cannot source. Sixteen vectors settle it.
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

            // Compares write a BOOLEAN register, never an AR.
            Inst::FpCmp(op, br, fs, ft) => {
                let a = self.fp_input(self.rfreg(fs.num()));
                let b = self.fp_input(self.rfreg(ft.num()));
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
                self.wbreg(br.num(), r, tracer);
            }

            Inst::FpToInt(op, ar, fs, imm) => {
                let bits = self.fp_input(self.rfreg(fs.num()));
                let out = self.fp_to_int(op, bits, imm);
                self.wreg(ar.num(), out, tracer);
            }

            Inst::IntToFp(op, fr, ars, imm) => {
                let v = self.rreg(ars.num());
                let x = match op {
                    IntToFpOp::FloatS => v as i32 as f32,
                    IntToFpOp::UfloatS => v as f32,
                };
                let scaled = self.apply_scale(x, imm, true);
                let out = self.fp_result(scaled.to_bits());
                self.wfreg(fr.num(), out, tracer);
            }

            _ => unreachable!("exec_float_math got {inst:?}"),
        }
        Ok(Flow::Next)
    }

    /// `float.md` §2: round-to-nearest-even, always; shader code never touches
    /// `FCR`. A non-default `FCR.RM` is refused rather than quietly ignored,
    /// because silently pretending RNE is exactly how a rounding-mode bug
    /// reaches a board (D6).
    ///
    /// The test is on the `RM` field (ISA RM Table 4-47, p. 69-70) and not on
    /// the whole register: the other bits are exception *enables*, and refusing
    /// a rounding change that never happened would be its own kind of wrong.
    ///
    /// Refusal is spelled as a read of `fcr_rounding_honored`, which is
    /// unresolved and therefore panics with the field's name. That is not a
    /// dodge: whether this silicon honors the field *at all* is genuinely a
    /// measurement (family F1 replays the tie set under all four modes), and
    /// routing the refusal through the policy means the corpus records those
    /// rows as `UNKNOWN` naming the right field instead of as an assertion
    /// failure with no home.
    ///
    /// # Panics
    /// If `FCR.RM` is not [`crate::cpu::FCR_RM_NEAREST`].
    fn refuse_non_default_rounding(&self) {
        if self.cpu.fcr_rounding_mode() != crate::cpu::FCR_RM_NEAREST {
            self.fp_policy.fcr_rounding_honored.get();
        }
    }

    /// Apply the input-denormal policy to an operand.
    fn fp_input(&self, bits: u32) -> u32 {
        if is_subnormal(bits) && *self.fp_policy.flush_input_denormals.get() {
            bits & SIGN
        } else {
            bits
        }
    }

    /// Apply the output-denormal and generated-NaN policies to a result.
    fn fp_result(&self, bits: u32) -> u32 {
        if is_nan(bits) {
            return *self.fp_policy.default_generated_nan.get();
        }
        if is_subnormal(bits) && *self.fp_policy.flush_output_denormals.get() {
            return bits & SIGN;
        }
        bits
    }

    /// Which NaN survives, when at least one operand is one.
    fn fp_nan_out(&self, operands: &[u32]) -> u32 {
        match *self.fp_policy.nan_propagation.get() {
            NanRule::PropagateFirstOperand => operands
                .iter()
                .copied()
                .find(|b| is_nan(*b))
                .expect("called only when an operand is NaN"),
            NanRule::PropagateSecondOperand => operands
                .iter()
                .rev()
                .copied()
                .find(|b| is_nan(*b))
                .expect("called only when an operand is NaN"),
            NanRule::Canonicalize => *self.fp_policy.default_generated_nan.get(),
        }
    }

    /// `add.s` / `sub.s` / `mul.s`: IEEE-fixed for normal finite operands, and
    /// policy everywhere else.
    fn fp_binop(&self, a: u32, b: u32, f: impl Fn(f32, f32) -> f32) -> u32 {
        let (a, b) = (self.fp_input(a), self.fp_input(b));
        if is_nan(a) || is_nan(b) {
            return self.fp_nan_out(&[a, b]);
        }
        self.fp_result(f(f32::from_bits(a), f32::from_bits(b)).to_bits())
    }

    /// `madd.s` (`acc + x*y`) and `msub.s` (`acc - x*y`). Whether that is one
    /// rounding or two is `madd_fused` — both are one line, and the point is
    /// not to choose between them by guessing.
    fn fp_madd(&self, acc: u32, x: u32, y: u32, subtract: bool) -> u32 {
        let (acc, x, y) = (self.fp_input(acc), self.fp_input(x), self.fp_input(y));
        if is_nan(acc) || is_nan(x) || is_nan(y) {
            return self.fp_nan_out(&[acc, x, y]);
        }
        let (a, mut p, q) = (f32::from_bits(acc), f32::from_bits(x), f32::from_bits(y));
        if subtract {
            p = -p;
        }
        let r = if *self.fp_policy.madd_fused.get() {
            p.mul_add(q, a)
        } else {
            (p * q) + a
        };
        self.fp_result(r.to_bits())
    }

    /// The conversion instructions' 0..=15 scale immediate.
    ///
    /// `to_float` selects the direction: `float.s`/`ufloat.s` scale *down* after
    /// converting, `trunc.s` and friends scale *up* before. Which association is
    /// which is `conversion_scale`, resolved from the ISA RM (`FLOAT.S` p. 346
    /// scales the converted integer by 2^-t, `TRUNC.S` p. 548 scales the float
    /// by 2^+t before converting). Only consulted for `imm != 0` — M7 emits
    /// `imm = 0`, where the question does not arise.
    fn apply_scale(&self, x: f32, imm: u8, to_float: bool) -> f32 {
        if imm == 0 {
            return x;
        }
        let up = match *self.fp_policy.conversion_scale.get() {
            ScaleRule::FractionalBits => !to_float,
            ScaleRule::Inverted => to_float,
        };
        let factor = f32::from_bits(((127i32 + i32::from(imm)) as u32) << 23);
        // A single-precision multiply, matching the manual's `×s pows(2.0,±t)`.
        // `2^-imm` is exact for imm <= 15, so the down direction is spelled as a
        // divide by the same factor rather than a second constant.
        if up { x * factor } else { x / factor }
    }

    /// `trunc.s` / `utrunc.s` / `round.s` / `floor.s` / `ceil.s`.
    ///
    /// The out-of-range and NaN answers are per-instruction facts stated on each
    /// instruction's own manual page — `TRUNC.S` p. 548, `ROUND.S` p. 497,
    /// `FLOOR.S` p. 347, `CEIL.S` p. 311, `UTRUNC.S` p. 555 — and the unsigned
    /// form is not simply the signed one with different limits: it answers
    /// `0xffff_ffff` for NaN and `0x8000_0000` for anything negative.
    fn fp_to_int(&self, op: FpToIntOp, bits: u32, imm: u8) -> u32 {
        let x = f32::from_bits(bits);
        let unsigned = matches!(op, FpToIntOp::UtruncS);

        if x.is_nan() {
            // `float.md` §5 leaves this unspecified at the *product* level,
            // which is precisely why the target's answer has to be recorded.
            return if unsigned {
                U32_SATURATE_MAX
            } else {
                *self.fp_policy.float_to_int_nan.get()
            };
        }
        let x = self.apply_scale(x, imm, false);

        if unsigned {
            // Tested on the value, before rounding toward zero: the manual says
            // "negative numbers", and -0.5 is a negative number even though it
            // truncates to -0.0. -0.0 itself is not negative and converts to 0.
            if x < 0.0 {
                return *self.fp_policy.utrunc_negative.get();
            }
            let r = x.trunc();
            // 0xffff_ffff is not representable in f32; the nearest above the
            // largest in-range value is 2^32, so that is the real threshold.
            if r >= 4_294_967_296.0 {
                return out_of_range(
                    r,
                    *self.fp_policy.float_to_int_out_of_range.get(),
                    0,
                    U32_SATURATE_MAX,
                );
            }
            return r as u32;
        }

        let r = match op {
            FpToIntOp::TruncS | FpToIntOp::UtruncS => x.trunc(),
            FpToIntOp::FloorS => x.floor(),
            FpToIntOp::CeilS => x.ceil(),
            FpToIntOp::RoundS => {
                // A tie is an exact halfway case; only then does the tie-break
                // rule matter, and only then is the policy read. The manual says
                // ROUND.S rounds "toward the nearest" without naming a tie rule,
                // so this one stays open for silicon.
                if (x - x.floor()) == 0.5 {
                    match *self.fp_policy.round_s_ties.get() {
                        TiesRule::ToEven => round_ties_even(x),
                        TiesRule::AwayFromZero => x.round(),
                    }
                } else {
                    x.round()
                }
            }
        };

        // `i32::MAX` is not representable in f32 — the smallest f32 above it is
        // 2^31 exactly — so the upper test is `>= 2^31`, and the saturation
        // value is written as the integer constant rather than round-tripped
        // through `i32::MAX as f32`, which lands on 2^31 and would answer
        // 0x8000_0000 where the manual says 0x7fff_ffff.
        if r >= 2_147_483_648.0 || r < -2_147_483_648.0 {
            return out_of_range(
                r,
                *self.fp_policy.float_to_int_out_of_range.get(),
                I32_SATURATE_MIN,
                I32_SATURATE_MAX,
            );
        }
        r as i32 as u32
    }
}

/// The signed conversions' negative-overflow answer (`32'h80000000`).
const I32_SATURATE_MIN: u32 = 0x8000_0000;
/// The signed conversions' positive-overflow and NaN answer (`32'h7fffffff`).
const I32_SATURATE_MAX: u32 = 0x7FFF_FFFF;
/// `utrunc.s`'s positive-overflow and NaN answer (`32'hffffffff`).
const U32_SATURATE_MAX: u32 = 0xFFFF_FFFF;

/// One of the non-estimate divide/sqrt helper steps, once its semantics exist.
///
/// Reaching this means [`crate::fp_policy::FpPolicy::divide_step_helpers`] has
/// been *resolved* but nobody wrote the executors to go with it — which is the
/// other half of resolving it. Deliberately a loud panic naming the instruction
/// rather than a plausible formula: a formula that looked right here would pass
/// casual tests and hide that the semantics were never actually sourced.
fn divide_step(mnemonic: &'static str, semantics: &crate::fp_policy::DivideStepSemantics) -> ! {
    panic!(
        "`{mnemonic}` semantics are recorded as sourced from {}, but its \
         executor was never written. Resolving `divide_step_helpers` means \
         implementing the six helper steps in `executor/float_math.rs`, not \
         only recording that they are known.",
        semantics.sourced_from
    )
}

/// One of the four implementation-defined estimate instructions, once its table
/// exists.
///
/// Same contract as [`divide_step`]: P6's extraction is not finished until the
/// lookup that consumes the table is written here. The table's *shape* is fixed
/// (an index over the leading significand bits, a per-op exponent rule, and a
/// parity-selected pair for `rsqrt0.s`); the index width and the exponent rule
/// are confirmed by sweeping, not assumed, so the lookup lands with the data.
fn estimate(mnemonic: &'static str, tables: &crate::fp_policy::EstimateTables) -> ! {
    panic!(
        "`{mnemonic}` has an extracted table ({} index bits) but no lookup. \
         The M6 P6 extraction is only complete when the exponent rule it \
         confirmed is implemented here alongside the data.",
        tables.index_bits
    )
}

/// Round half to even, spelled out rather than taken from a Rust intrinsic so
/// the tie-break is visible at the point the policy selects it.
fn round_ties_even(x: f32) -> f32 {
    let lo = x.floor();
    if lo as i64 % 2 == 0 { lo } else { lo + 1.0 }
}

/// Resolve a float→int conversion that cannot be represented.
///
/// `Saturate` is what the ISA RM states per instruction and what
/// `docs/design/float.md` §3 makes the product's Guaranteed behavior. `Wrap`
/// stays modeled because P6 can still falsify the manual on silicon, and an
/// emulator that could not express the other answer would have no way to record
/// the finding.
fn out_of_range(r: f32, rule: OutOfRangeRule, min: u32, max: u32) -> u32 {
    match rule {
        OutOfRangeRule::Saturate => {
            if r.is_sign_negative() {
                min
            } else {
                max
            }
        }
        OutOfRangeRule::Wrap => {
            assert!(
                r.is_finite(),
                "wrap semantics for an infinite float->int conversion are not \
                 modeled — if P6 measures Wrap, it must also say what an \
                 infinity produces"
            );
            r as i64 as u32
        }
    }
}

#[cfg(test)]
mod tests {
    use lp_xt_inst::{BReg, FReg, FpCmpOp, FpRrOp, FpRrrOp, Inst, Reg};

    use crate::cpu::CPENABLE_FPU;
    use crate::emu::Emulator;
    use crate::fp_policy::parse_unresolved;
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

    /// Run `f`, returning the policy field name if it panicked on an unresolved
    /// field. Any other panic is re-raised, so a real bug is still a real bug.
    fn unresolved_field(f: impl FnOnce() + std::panic::UnwindSafe) -> Option<String> {
        crate::fp_policy::suppress_unresolved_panic_output();
        let r = std::panic::catch_unwind(f);
        match r {
            Ok(()) => None,
            Err(e) => {
                let msg = e
                    .downcast_ref::<String>()
                    .cloned()
                    .unwrap_or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()).unwrap());
                match parse_unresolved(&msg) {
                    Some(f) => Some(f.to_string()),
                    None => panic!("unexpected panic: {msg}"),
                }
            }
        }
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

    /// The real oracle for the ~99% of the space IEEE fixes: for normal, finite,
    /// non-zero operands with a normal finite result, `add.s`/`sub.s`/`mul.s`
    /// must equal Rust's `f32` bit for bit. No policy field is read on this
    /// path, which is the other half of the claim.
    #[test]
    fn ieee_fixed_core_agrees_with_host_f32_bit_for_bit() {
        let mut emu = armed();
        let mut state = 0x1234_5678u32;
        let mut checked = 0;
        for _ in 0..20_000 {
            let a = xorshift(&mut state);
            let b = xorshift(&mut state);
            let (fa, fb) = (f32::from_bits(a), f32::from_bits(b));
            if !fa.is_normal() || !fb.is_normal() {
                continue;
            }
            for (op, expect) in [
                (FpRrrOp::AddS, fa + fb),
                (FpRrrOp::SubS, fa - fb),
                (FpRrrOp::MulS, fa * fb),
            ] {
                if !expect.is_normal() {
                    continue; // subnormal / inf / NaN results are policy, not IEEE
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

    /// The full ordered/unordered matrix, including both NaN kinds. IEEE fixes
    /// all of it, so none of these rows is an `UNKNOWN`.
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
            // Any ordered compare with a NaN is false; the unordered forms are
            // true. Both NaN kinds, both operand positions.
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

    /// The safety property: an unmeasured corner faults, naming the field and
    /// the family that closes it. If any of these silently produced a value,
    /// M6's whole premise would be gone.
    #[test]
    fn unmeasured_corners_fail_loudly_and_name_their_field() {
        let qnan = 0x7FC0_0000u32;
        let subnormal = 0x0000_0001u32;
        let inf = f32::INFINITY.to_bits();
        let cases: &[(&str, Inst, [(u8, u32); 3])] = &[
            (
                "nan_propagation",
                Inst::FpRrr(FpRrrOp::AddS, FReg::new(0), FReg::new(1), FReg::new(2)),
                [(1, qnan), (2, 1.0f32.to_bits()), (0, 0)],
            ),
            (
                "flush_input_denormals",
                Inst::FpRrr(FpRrrOp::MulS, FReg::new(0), FReg::new(1), FReg::new(2)),
                [(1, subnormal), (2, 1.0f32.to_bits()), (0, 0)],
            ),
            (
                "default_generated_nan",
                Inst::FpRrr(FpRrrOp::SubS, FReg::new(0), FReg::new(1), FReg::new(2)),
                [(1, inf), (2, inf), (0, 0)],
            ),
            (
                "const_s_table",
                Inst::ConstS(FReg::new(0), 1),
                [(0, 0), (1, 0), (2, 0)],
            ),
            (
                "estimates",
                Inst::FpRr(FpRrOp::Recip0S, FReg::new(0), FReg::new(1)),
                [(1, 2.0f32.to_bits()), (0, 0), (2, 0)],
            ),
            (
                "divide_step_helpers",
                Inst::FpRr(FpRrOp::Nexp01S, FReg::new(0), FReg::new(1)),
                [(1, 2.0f32.to_bits()), (0, 0), (2, 0)],
            ),
        ];
        for (want, inst, regs) in cases {
            let got = unresolved_field(|| {
                let mut emu = armed();
                for (r, v) in regs {
                    emu.cpu.set_f(*r, *v);
                }
                let mut t = NoopTracer;
                let _ = emu.execute(inst, 0x4000_0100, &mut t);
            });
            assert_eq!(got.as_deref(), Some(*want), "for {inst:?}");
        }
    }

    /// Conversions at `imm = 0` on in-range values are fully determined; the
    /// boundaries are not, and must say so.
    #[test]
    fn conversions_are_exact_in_range_and_unknown_outside_it() {
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
        // >2^24 needs rounding; RNE is IEEE-fixed and Rust's `as` does it.
        emu.cpu.set_a(2, 16_777_217u32);
        exec(
            &mut emu,
            &Inst::IntToFp(IntToFpOp::FloatS, FReg::new(0), Reg::new(2), 0),
        );
        assert_eq!(emu.cpu.f(0), (16_777_217i32 as f32).to_bits());

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
        for (v, want) in [(1.9f32, 1i32), (-1.9, -2)] {
            emu.cpu.set_f(1, v.to_bits());
            exec(
                &mut emu,
                &Inst::FpToInt(FpToIntOp::FloorS, Reg::new(3), FReg::new(1), 0),
            );
            assert_eq!(emu.cpu.a(3) as i32, want, "floor.s of {v}");
        }

        // A tie is still a question for silicon: the manual says ROUND.S rounds
        // "toward the nearest" and never names the tie-break.
        let got = unresolved_field(|| {
            let mut e = armed();
            e.cpu.set_f(1, 2.5f32.to_bits());
            let mut t = NoopTracer;
            let _ = e.execute(
                &Inst::FpToInt(FpToIntOp::RoundS, Reg::new(3), FReg::new(1), 0),
                0x4000_0100,
                &mut t,
            );
        });
        assert_eq!(got.as_deref(), Some("round_s_ties"));
    }

    /// The out-of-range, infinite and NaN answers the ISA RM states per
    /// instruction. The signed positive answer is the one a naive
    /// `i32::MAX as f32` round-trip gets wrong — 2^31 is the nearest f32 and
    /// converts back to 0x80000000 — so it is asserted explicitly.
    #[test]
    fn conversion_boundaries_follow_the_manuals_per_instruction_answers() {
        use lp_xt_inst::FpToIntOp;
        let mut emu = armed();
        let signed = [
            FpToIntOp::TruncS,
            FpToIntOp::RoundS,
            FpToIntOp::FloorS,
            FpToIntOp::CeilS,
        ];
        for op in signed {
            for (v, want) in [
                (1e30f32, 0x7FFF_FFFFu32),
                (f32::INFINITY, 0x7FFF_FFFF),
                (f32::NAN, 0x7FFF_FFFF),
                (-1e30, 0x8000_0000),
                (f32::NEG_INFINITY, 0x8000_0000),
            ] {
                emu.cpu.set_f(1, v.to_bits());
                exec(&mut emu, &Inst::FpToInt(op, Reg::new(3), FReg::new(1), 0));
                assert_eq!(emu.cpu.a(3), want, "{op:?} of {v}");
            }
        }
        // utrunc.s answers differently in every one of those places.
        for (v, want) in [
            (f32::NAN, 0xFFFF_FFFFu32),
            (f32::INFINITY, 0xFFFF_FFFF),
            (1e30, 0xFFFF_FFFF),
            (-1.0, 0x8000_0000),
            (-0.5, 0x8000_0000),
            (f32::NEG_INFINITY, 0x8000_0000),
            // -0.0 is not a negative *number*, so it converts normally.
            (-0.0, 0),
            (7.9, 7),
        ] {
            emu.cpu.set_f(1, v.to_bits());
            exec(
                &mut emu,
                &Inst::FpToInt(FpToIntOp::UtruncS, Reg::new(3), FReg::new(1), 0),
            );
            assert_eq!(emu.cpu.a(3), want, "utrunc.s of {v}");
        }
    }

    /// The scale immediate, both directions. `t` is the fractional-bit count:
    /// `float.s` divides the converted integer by `2^t`, the float→int
    /// conversions multiply by `2^t` first.
    #[test]
    fn the_scale_immediate_is_a_fractional_bit_count_in_both_directions() {
        use lp_xt_inst::{FpToIntOp, IntToFpOp};
        let mut emu = armed();

        emu.cpu.set_a(2, 3);
        exec(
            &mut emu,
            &Inst::IntToFp(IntToFpOp::FloatS, FReg::new(0), Reg::new(2), 4),
        );
        assert_eq!(emu.cpu.f(0), (3.0f32 / 16.0).to_bits(), "float.s 3, imm=4");

        emu.cpu.set_f(1, 0.1875f32.to_bits()); // 3 / 16
        exec(
            &mut emu,
            &Inst::FpToInt(FpToIntOp::TruncS, Reg::new(3), FReg::new(1), 4),
        );
        assert_eq!(emu.cpu.a(3), 3, "trunc.s 0.1875, imm=4");
    }

    /// `madd.s` rounds once, not twice — and the corpus needs a case where the
    /// two answers actually differ, or the claim is untested.
    #[test]
    fn madd_is_fused_and_the_two_roundings_are_distinguishable() {
        let mut emu = armed();
        // (1 + 2^-12)^2 is exactly 1 + 2^-11 + 2^-24, a tie one bit past f32's
        // significand: rounding it first lands on 1 + 2^-11, which the
        // accumulator then cancels to zero. Keeping the exact product leaves
        // 2^-24. So the two answers are 2^-24 and 0 — as far apart as a single
        // rounding difference gets.
        let x = f32::from_bits(0x3F80_0800); // 1 + 2^-12
        let y = x;
        let a = f32::from_bits(0xBF80_1000); // -(1 + 2^-11)
        let fused = x.mul_add(y, a);
        let unfused = (x * y) + a;
        assert_eq!(fused.to_bits(), 0x3380_0000, "fused answer is 2^-24");
        assert_eq!(unfused.to_bits(), 0, "unfused answer cancels to zero");
        assert_ne!(
            fused.to_bits(),
            unfused.to_bits(),
            "the case must distinguish one rounding from two"
        );
        emu.cpu.set_f(0, a.to_bits());
        emu.cpu.set_f(1, x.to_bits());
        emu.cpu.set_f(2, y.to_bits());
        exec(
            &mut emu,
            &Inst::FpRrr(FpRrrOp::MaddS, FReg::new(0), FReg::new(1), FReg::new(2)),
        );
        assert_eq!(emu.cpu.f(0), fused.to_bits());
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
        assert_eq!(
            emu.cpu.f(0),
            PAY,
            "moveqz.s on zero moves, without touching \
                                        the payload"
        );

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

    /// D6: a non-default rounding mode is refused, not ignored — and the
    /// refusal names the field a measurement would settle.
    #[test]
    fn a_non_default_fcr_rounding_mode_is_refused() {
        let got = unresolved_field(|| {
            let mut emu = armed();
            emu.cpu.fcr = 1;
            emu.cpu.set_f(1, 1.0f32.to_bits());
            emu.cpu.set_f(2, 1.0f32.to_bits());
            let mut t = NoopTracer;
            let _ = emu.execute(
                &Inst::FpRrr(FpRrrOp::AddS, FReg::new(0), FReg::new(1), FReg::new(2)),
                0x4000_0100,
                &mut t,
            );
        });
        assert_eq!(got.as_deref(), Some("fcr_rounding_honored"));
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
