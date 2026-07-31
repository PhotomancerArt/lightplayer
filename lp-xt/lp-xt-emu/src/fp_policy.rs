//! The explicit policy layer between the FP executors and any actual
//! arithmetic (M6 D7).
//!
//! # Why this exists
//!
//! Rust's `f32` is IEEE-754 binary32 under round-to-nearest-even, and for the
//! overwhelming majority of the input space that is exactly what the Xtensa FPU
//! does. But Rust cannot express three things this milestone exists to pin
//! down: **which NaN** an operation propagates or generates, **whether
//! denormals are flushed** at the input or the output, and **what rounding mode
//! is in force**. Computing FP results with bare `f32` operators would therefore
//! produce an emulator that is right about 99% of the space and silently
//! confident about the rest — the wrong-but-plausible failure M6 was written to
//! prevent.
//!
//! So every behavior IEEE-754 does not fix is a named field on [`FpPolicy`],
//! and every such field is either **resolved by a measurement with a citation**
//! or [`Unknown`]. Reading an unresolved field **panics**. It does not fall back
//! to a default, because a plausible default is indistinguishable from knowledge
//! once it is in the code.
//!
//! At the end of M6 P3 almost every field is `Unknown`. **That is the correct
//! state**, not an incomplete one: the unresolved list is precisely the row list
//! of §4 of the FP-contract ADR, and P6 closes it from silicon.
//!
//! # How the corpus copes
//!
//! A vector whose prediction would require an unresolved field is not a failure
//! — it is a question addressed to silicon. `tests/fp_conformance.rs` runs each
//! vector inside `catch_unwind`, recognizes the panic via
//! [`parse_unresolved`], and records the row as `UNKNOWN` naming the field. The
//! UNKNOWN set is therefore *derived* from the policy rather than maintained by
//! hand, so it cannot drift away from what the executors actually need.

use core::fmt;

/// Prefix of the panic message raised by [`Unknown::get`]. Stable, because
/// `tests/fp_conformance.rs` matches on it — see [`parse_unresolved`].
pub const UNRESOLVED_PANIC_PREFIX: &str = "unresolved Xtensa FP policy field `";

/// A behavior of the Xtensa FPU that is either **measured** — with a citation
/// naming the measurement — or not yet known.
///
/// The distinction is the whole point, so there is deliberately no `Default`,
/// no `unwrap_or`, and no way to read an unresolved value without a panic.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Unknown<T> {
    field: &'static str,
    /// What would settle this: the vector family, or the extraction step.
    resolved_by: &'static str,
    value: Option<T>,
    /// Provenance of a resolved value. `None` iff `value` is `None`.
    citation: Option<&'static str>,
}

impl<T> Unknown<T> {
    /// An open question, with the name of the thing that closes it.
    pub const fn unknown(field: &'static str, resolved_by: &'static str) -> Unknown<T> {
        Unknown {
            field,
            resolved_by,
            value: None,
            citation: None,
        }
    }

    /// A behavior settled by a measurement. `citation` names it — a planning
    /// record, an ADR section, a dated desk session — never "obviously".
    pub const fn measured(
        field: &'static str,
        resolved_by: &'static str,
        value: T,
        citation: &'static str,
    ) -> Unknown<T> {
        Unknown {
            field,
            resolved_by,
            value: Some(value),
            citation: Some(citation),
        }
    }

    /// The field's name, as it appears in the panic message and the ADR row.
    pub const fn field(&self) -> &'static str {
        self.field
    }

    /// What would resolve this field.
    pub const fn resolved_by(&self) -> &'static str {
        self.resolved_by
    }

    /// The measurement a resolved value came from, or `None`.
    pub const fn citation(&self) -> Option<&'static str> {
        self.citation
    }

    /// Whether the behavior is known.
    pub const fn is_resolved(&self) -> bool {
        self.value.is_some()
    }

    /// The value, or `None` if unresolved. For callers that want to branch
    /// rather than fault — the corpus generator, and the inventory test.
    pub const fn try_get(&self) -> Option<&T> {
        self.value.as_ref()
    }

    /// The value, **panicking** if it is not yet known.
    ///
    /// This is what the executors call. A panic here is not a bug in the
    /// emulator: it means a vector reached a corner nothing has measured, and
    /// the honest answer is to say so loudly rather than to invent one.
    ///
    /// # Panics
    /// If the field is unresolved.
    pub fn get(&self) -> &T {
        match self.value.as_ref() {
            Some(v) => v,
            None => panic!(
                "{UNRESOLVED_PANIC_PREFIX}{}` — resolved by: {}. \
                 This corner has not been measured on silicon; the corpus row \
                 must be UNKNOWN, not a guess (M6 P6 closes it).",
                self.field, self.resolved_by
            ),
        }
    }
}

/// Recover the field name from an [`Unknown::get`] panic message.
///
/// Returns `None` for any other panic, so a real bug still surfaces as a real
/// failure instead of being quietly filed as an open question.
pub fn parse_unresolved(msg: &str) -> Option<&str> {
    let rest = msg.strip_prefix(UNRESOLVED_PANIC_PREFIX)?;
    rest.split('`').next()
}

/// Which NaN survives an operation with one or more NaN operands.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NanRule {
    /// The first NaN operand's bits pass through unchanged.
    PropagateFirstOperand,
    /// The second NaN operand's bits pass through unchanged.
    PropagateSecondOperand,
    /// Any NaN operand is replaced by the default generated NaN.
    Canonicalize,
}

/// Whether the `imm` scale field multiplies before or divides after.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ScaleRule {
    /// `float.s` divides by `2^imm`; `trunc.s` multiplies by `2^imm` — the
    /// fixed-point reading, where `imm` is the fractional bit count.
    FractionalBits,
    /// The opposite association.
    Inverted,
}

/// What a float→int conversion does with a value it cannot represent.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OutOfRangeRule {
    /// Clamp to the destination's min/max. `docs/design/float.md` §3 makes this
    /// the *product's* Guaranteed behavior; whether the instruction does it, or
    /// M7 owes a clamp, is the measurement.
    Saturate,
    /// Take the low bits.
    Wrap,
}

/// How `round.s` breaks an exact halfway case. `float.md` §4 files this under
/// target-defined, so both answers are legal and only one is true here.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TiesRule {
    ToEven,
    AwayFromZero,
}

/// The `FSR` flag layout: which bit is which IEEE exception, and which
/// operations set which flag.
///
/// P1 measured that the register **accumulates** (0 on a fresh boot, `0x400`
/// after a 24-instruction FP sweep) but not what `0x400` *is*. Modeled as a
/// whole because the layout and the setting rules are one question, and
/// `float.md` §2 puts the register out of shader reach either way.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct FsrFlagBits {
    pub invalid: u32,
    pub div_by_zero: u32,
    pub overflow: u32,
    pub underflow: u32,
    pub inexact: u32,
}

/// The implementation-defined estimate tables behind `recip0.s`, `rsqrt0.s`,
/// `sqrt0.s`, and `div0.s`.
///
/// Deliberately a *table*, not a formula. These instructions read a lookup ROM;
/// no document yields its contents, and a polynomial approximation that came
/// close would pass casual tests while hiding the fact that the real table was
/// never captured. P6 extracts them exhaustively (sweep the significand for a
/// representative exponent, run-length encode, then confirm the exponent rule
/// separates) and this becomes exact by construction.
///
/// The shape is fixed here so P6 only has to fill it: an index into the leading
/// significand bits, plus an exponent rule, plus — for `rsqrt0.s` — a second
/// table selected by exponent parity.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct EstimateTables {
    /// Number of leading significand bits used as the table index. P6 confirms
    /// this by sweeping rather than assuming it.
    pub index_bits: u8,
    pub recip0: Vec<u32>,
    pub sqrt0: Vec<u32>,
    /// Indexed `[exponent_parity][significand]`.
    pub rsqrt0_by_parity: [Vec<u32>; 2],
    pub div0: Vec<u32>,
}

/// Semantics of the non-estimate members of the divide/sqrt helper family:
/// `nexp01.s`, `mkdadj.s`, `addexp.s`, `addexpm.s`, `maddn.s`, `divn.s`.
///
/// These are architecturally defined — the Xtensa ISA Reference Manual fixes
/// them — but the manual is not available in this working environment, and
/// `AGENTS.md`'s license rule forbids reading binutils, GCC, or QEMU source to
/// recover them. Recording them as an open question is the honest position:
/// either a manual read or a P6 measurement closes it, and neither is a guess.
/// Their *presence* on silicon is settled (M6 P1, all nine helpers executed).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DivideStepSemantics {
    /// Marker: this type is populated wholesale when the semantics are sourced.
    /// Left opaque on purpose — inventing its shape now would be inventing the
    /// answer's shape.
    pub sourced_from: &'static str,
}

/// The behaviors of the Xtensa FPU that IEEE-754 does not fix.
///
/// Construct with [`FpPolicy::m6`]. Every field is `pub` so the inventory test
/// can enumerate them; none has a default.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct FpPolicy {
    /// Which NaN survives an operation with NaN operands. `float.md` §4 files
    /// NaN bit patterns under target-defined, so this is a real measurement and
    /// not a spec lookup.
    pub nan_propagation: Unknown<NanRule>,
    /// The NaN an operation *generates* from non-NaN operands (`inf - inf`,
    /// `inf * 0`, `0/0`, `sqrt(-x)`).
    pub default_generated_nan: Unknown<u32>,
    /// Whether a subnormal **operand** reads as zero.
    pub flush_input_denormals: Unknown<bool>,
    /// Whether a subnormal **result** is written as zero. Separable from the
    /// input question on purpose: they are different behaviors, and a corpus
    /// that conflates them cannot say which one this silicon does.
    pub flush_output_denormals: Unknown<bool>,
    /// Whether `madd.s`/`msub.s` round once (a true FMA) or twice.
    pub madd_fused: Unknown<bool>,
    /// Which direction the conversion instructions' scale immediate applies.
    /// Only consulted for `imm != 0`; M7 emits `imm = 0`, where the question
    /// does not arise.
    pub conversion_scale: Unknown<ScaleRule>,
    /// What `trunc.s`/`round.s`/`floor.s`/`ceil.s` do with a finite value
    /// outside `i32`, and with `±inf`.
    pub float_to_int_out_of_range: Unknown<OutOfRangeRule>,
    /// The integer a NaN converts to. `float.md` §5 leaves this unspecified at
    /// the product level, which is exactly why the *target's* answer has to be
    /// recorded rather than assumed.
    pub float_to_int_nan: Unknown<u32>,
    /// What `utrunc.s` does with a negative input.
    pub utrunc_negative: Unknown<OutOfRangeRule>,
    /// How `round.s` breaks an exact tie.
    pub round_s_ties: Unknown<TiesRule>,
    /// The sixteen constants `const.s` can produce. Architecturally defined and
    /// not available here; sixteen vectors settle it.
    pub const_s_table: Unknown<[u32; 16]>,
    /// The `FSR` flag layout.
    pub fsr_flag_bits: Unknown<FsrFlagBits>,
    /// Whether `FSR` accumulates rather than replacing. **Resolved**: it does.
    pub fsr_sticky: Unknown<bool>,
    /// Whether an ordered compare on a signalling NaN raises Invalid.
    pub snan_compare_signals: Unknown<bool>,
    /// Whether writing a non-default rounding mode to `FCR` actually changes
    /// results, or is ignored. Until this is measured the emulator **refuses**
    /// a non-zero rounding field rather than silently pretending RNE (D6).
    pub fcr_rounding_honored: Unknown<bool>,
    /// The implementation-defined estimate tables.
    pub estimates: Unknown<EstimateTables>,
    /// The non-estimate divide/sqrt helpers.
    pub divide_step_helpers: Unknown<DivideStepSemantics>,
}

impl FpPolicy {
    /// The policy as M6 P3 leaves it: two rows measured, the rest open.
    pub const fn m6() -> FpPolicy {
        FpPolicy {
            nan_propagation: Unknown::unknown("nan_propagation", "vector family F2 (NaN payloads)"),
            default_generated_nan: Unknown::unknown(
                "default_generated_nan",
                "vector family F2 (hardware-generated NaNs)",
            ),
            flush_input_denormals: Unknown::unknown(
                "flush_input_denormals",
                "vector family F3 (denormals, input half)",
            ),
            flush_output_denormals: Unknown::unknown(
                "flush_output_denormals",
                "vector family F3 (denormals, output half)",
            ),
            madd_fused: Unknown::unknown("madd_fused", "vector family F1 (rounding)"),
            conversion_scale: Unknown::unknown(
                "conversion_scale",
                "vector family F6 (conversions, scale sweep)",
            ),
            float_to_int_out_of_range: Unknown::unknown(
                "float_to_int_out_of_range",
                "vector family F6 (conversions, boundaries)",
            ),
            float_to_int_nan: Unknown::unknown(
                "float_to_int_nan",
                "vector family F6 (conversions, NaN)",
            ),
            utrunc_negative: Unknown::unknown(
                "utrunc_negative",
                "vector family F6 (conversions, unsigned)",
            ),
            round_s_ties: Unknown::unknown("round_s_ties", "vector family F6 (conversions, ties)"),
            const_s_table: Unknown::unknown(
                "const_s_table",
                "vector family F5 (divide/sqrt sequence inputs)",
            ),
            fsr_flag_bits: Unknown::unknown(
                "fsr_flag_bits",
                "the FSR column of every vector family",
            ),
            fsr_sticky: Unknown::measured(
                "fsr_sticky",
                "the FSR column of every vector family",
                true,
                "M6 P1 desk session 2026-07-31: FSR read 0 on a fresh boot and \
                 0x400 after a 24-instruction FP sweep with no intervening write \
                 (p1-silicon-results.md)",
            ),
            snan_compare_signals: Unknown::unknown(
                "snan_compare_signals",
                "vector family F2 (NaN payloads, compare half)",
            ),
            fcr_rounding_honored: Unknown::unknown(
                "fcr_rounding_honored",
                "vector family F1 (rounding, all four FCR modes)",
            ),
            estimates: Unknown::unknown(
                "estimates",
                "the M6 P6 exhaustive estimate-table extraction",
            ),
            divide_step_helpers: Unknown::unknown(
                "divide_step_helpers",
                "the Xtensa ISA Reference Manual, or an M6 P6 measurement",
            ),
        }
    }

    /// Every field, as `(name, resolved_by, citation)` — the inventory the ADR's
    /// §4 rows are drawn from, and what the inventory test walks.
    ///
    /// Written out by hand on purpose: a derive would let a new field be added
    /// without anyone deciding what resolves it.
    pub fn inventory(&self) -> Vec<(&'static str, &'static str, Option<&'static str>)> {
        macro_rules! row {
            ($f:expr) => {
                ($f.field(), $f.resolved_by(), $f.citation())
            };
        }
        vec![
            row!(self.nan_propagation),
            row!(self.default_generated_nan),
            row!(self.flush_input_denormals),
            row!(self.flush_output_denormals),
            row!(self.madd_fused),
            row!(self.conversion_scale),
            row!(self.float_to_int_out_of_range),
            row!(self.float_to_int_nan),
            row!(self.utrunc_negative),
            row!(self.round_s_ties),
            row!(self.const_s_table),
            row!(self.fsr_flag_bits),
            row!(self.fsr_sticky),
            row!(self.snan_compare_signals),
            row!(self.fcr_rounding_honored),
            row!(self.estimates),
            row!(self.divide_step_helpers),
        ]
    }
}

impl Default for FpPolicy {
    fn default() -> FpPolicy {
        FpPolicy::m6()
    }
}

impl fmt::Display for FpPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (name, by, cite) in self.inventory() {
            match cite {
                Some(c) => writeln!(f, "{name}: MEASURED ({c})")?,
                None => writeln!(f, "{name}: UNKNOWN (resolved by {by})")?,
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reading_an_unresolved_field_panics_with_a_useful_message() {
        let p = FpPolicy::m6();
        let err = std::panic::catch_unwind(|| *p.madd_fused.get()).expect_err("must panic");
        let msg = err
            .downcast_ref::<String>()
            .expect("the panic payload is the message");
        assert_eq!(parse_unresolved(msg), Some("madd_fused"));
        assert!(
            msg.contains("vector family F1"),
            "the message names what resolves it: {msg}"
        );
    }

    #[test]
    fn parse_unresolved_ignores_unrelated_panics() {
        assert_eq!(parse_unresolved("index out of bounds"), None);
    }

    #[test]
    fn a_measured_field_reads_without_panicking_and_carries_its_citation() {
        let p = FpPolicy::m6();
        assert!(*p.fsr_sticky.get(), "P1 measured FSR as sticky");
        assert!(p.fsr_sticky.citation().unwrap().contains("2026-07-31"));
    }

    /// The safety property: no field may acquire a value without a citation,
    /// and none may be dropped from the inventory. A field quietly given a
    /// plausible default later fails here.
    #[test]
    fn every_policy_field_is_either_measured_with_a_citation_or_explicitly_unknown() {
        let p = FpPolicy::m6();
        let inv = p.inventory();
        assert_eq!(inv.len(), 17, "a field was added or removed silently");
        for (name, by, cite) in &inv {
            assert!(!by.is_empty(), "{name} does not say what resolves it");
            if let Some(c) = cite {
                assert!(
                    c.len() > 20,
                    "{name} claims to be measured but its citation is not a real one: {c}"
                );
            }
        }
        let resolved: Vec<_> = inv
            .iter()
            .filter(|(_, _, c)| c.is_some())
            .map(|(n, ..)| *n)
            .collect();
        assert_eq!(
            resolved,
            vec!["fsr_sticky"],
            "exactly one row is settled before the hardware campaign; if this \
             list grew, the new entry needs a citation and an ADR row"
        );
    }
}
