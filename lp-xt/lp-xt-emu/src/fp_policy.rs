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
//!
//! # Two kinds of resolution
//!
//! A field leaves [`Unknown`] one of two ways, and the citation says which:
//!
//! - **From the ISA Reference Manual.** Six fields were closed this way in M6
//!   P5, once the manual became readable. A manual citation names the section
//!   and page; the text itself is never reproduced (AGENTS.md license rule), and
//!   no binutils, GCC, or QEMU source was consulted.
//! - **From silicon.** Anything the 2011 RM does not cover — NaN payload
//!   choices, denormal flush, the estimate lookup ROMs, the `div0.s` helper
//!   family, whether this implementation honors `FCR.RM` at all — stays open
//!   until the M6 P6 campaign measures it. The RM's own Table 4-46 does not list
//!   the estimate/helper instructions, so for those "the manual is silent" is a
//!   checked fact rather than an assumption.
//!
//! A manual-sourced answer is still falsifiable: P6 runs the same vectors on the
//! board, and a row where the RM says one thing and the S3 does another is
//! exactly the finding this campaign exists to surface.

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

/// Install a panic hook that prints nothing for an [`Unknown::get`] panic and
/// defers to the previous hook for everything else. Idempotent.
///
/// A harness that expects thousands of these — `tests/fp_conformance.rs` — would
/// otherwise bury its own output in backtraces. Installed **once**, globally,
/// rather than swapped around each `catch_unwind`: the hook is process-wide
/// state, and swapping it per call races with any other test running
/// concurrently, which shows up as one test's panic printing under another's
/// name.
pub fn suppress_unresolved_panic_output() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let msg = info
                .payload()
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| info.payload().downcast_ref::<&str>().copied())
                .unwrap_or("");
            if parse_unresolved(msg).is_none() {
                prev(info);
            }
        }));
    });
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

/// **Which operations set which `FSR` flag.**
///
/// The *bit layout* is no longer part of this question: the ISA RM's Table 4-48
/// (§4.3.11.3, p. 70) fixes it, and it is transcribed as
/// [`crate::cpu::FSR_INEXACT`]…[`crate::cpu::FSR_INVALID`]. That also explains
/// P1's measurement — `0x400` is [`crate::cpu::FSR_DIV_BY_ZERO`], and the P1
/// sweep ran `div0.s`/`recip0.s`/`rsqrt0.s` on a staged zero.
///
/// What remains open is the *setting rule*, and it is genuinely open rather than
/// merely undocumented: §4.3.11.4 states that current implementations set no
/// FSR flags at all, and the desk S3 demonstrably set one. So the document is
/// falsified here and only the campaign can say what the rule actually is.
///
/// Each field holds the flag mask an operation of that class raises, so a
/// resolved value is a mapping and not a restatement of the layout.
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
/// P3 recorded these as open because the ISA Reference Manual was not available.
/// It is available now, and **it does not cover them**: the manual's Table 4-46
/// (§4.3.11, p. 67-68) enumerates the Floating-Point Coprocessor Option's
/// instruction additions and none of `div0.s`, `divn.s`, `nexp01.s`,
/// `mkdadj.s`, `maddn.s`, `recip0.s`, `rsqrt0.s`, `sqrt0.s`, or `const.s`
/// appears in it — nor anywhere else in the document. They belong to a later
/// extension of the FP option than the 2011 edition describes.
///
/// So the field stays open for a better reason than before: the manual read has
/// happened and came back empty, `AGENTS.md`'s license rule keeps binutils, GCC,
/// and QEMU off the table, and P6's measurement is the remaining route. Their
/// *presence* on silicon is settled (M6 P1, all nine helpers executed).
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
    /// The **signed** integer a NaN converts to (`trunc.s`, `round.s`,
    /// `floor.s`, `ceil.s`). `utrunc.s` has its own answer and is not covered
    /// here. `float.md` §5 leaves this unspecified at the product level, which
    /// is exactly why the *target's* answer has to be recorded.
    pub float_to_int_nan: Unknown<u32>,
    /// The unsigned integer `utrunc.s` produces for a negative input.
    ///
    /// A raw value rather than a rule, because the answer turns out not to be a
    /// rule: the ISA RM specifies a fixed sentinel here, which is neither the
    /// saturation an [`OutOfRangeRule`] would name nor a wrap.
    pub utrunc_negative: Unknown<u32>,
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

/// Citation stem for every field closed by the M6 P5 manual read; the argument
/// names the section or instruction page. One macro so the provenance of the
/// manual-sourced rows is greppable and cannot drift apart row by row.
macro_rules! isa_rm {
    ($where:literal) => {
        concat!(
            "Xtensa ISA Reference Manual (2011), read for M6 P5 — ",
            $where
        )
    };
}

impl FpPolicy {
    /// The policy after M6 P5: one row measured on silicon, six read out of the
    /// ISA Reference Manual, ten still open for the P6 campaign.
    pub const fn m6() -> FpPolicy {
        FpPolicy {
            nan_propagation: Unknown::unknown("nan_propagation", "vector family F2 (NaN payloads)"),
            default_generated_nan: Unknown::unknown(
                "default_generated_nan",
                "vector family F2 (hardware-generated NaNs)",
            ),
            // Deliberately NOT closed from the manual. §4.3.11.2 (p. 69) says
            // the ISA includes sub-normal "representations and processing
            // rules", which is an architectural claim about the option and not
            // a statement about what this implementation does at run time —
            // flush-to-zero is precisely the thing a specific FPU deviates on,
            // and `docs/design/float.md` files it as target-defined. Reading
            // that one clause as a measurement would be the wrong-but-plausible
            // answer M6 exists to avoid. F3 settles it on the board.
            flush_input_denormals: Unknown::unknown(
                "flush_input_denormals",
                "vector family F3 (denormals, input half)",
            ),
            flush_output_denormals: Unknown::unknown(
                "flush_output_denormals",
                "vector family F3 (denormals, output half)",
            ),
            // `MADD.S` (p. 406) states the product is added without an
            // intermediate round, and its Operation line annotates the multiply
            // as non-rounding. That is a fused multiply-add, so `mul_add` — not
            // `(p * q) + a` — is the modelled behavior.
            madd_fused: Unknown::measured(
                "madd_fused",
                "vector family F1 (rounding)",
                true,
                isa_rm!("MADD.S, p. 406: the product is added with no intermediate round"),
            ),
            // `FLOAT.S` (p. 346) and `UFLOAT.S` (p. 550) scale the converted
            // integer by 2^-t; `TRUNC.S` (p. 548), `ROUND.S` (p. 497),
            // `FLOOR.S` (p. 347), `CEIL.S` (p. 311) and `UTRUNC.S` (p. 555)
            // scale the float by 2^+t before converting. `t` is therefore the
            // fractional-bit count in both directions — `ScaleRule::FractionalBits`.
            conversion_scale: Unknown::measured(
                "conversion_scale",
                "vector family F6 (conversions, scale sweep)",
                ScaleRule::FractionalBits,
                isa_rm!("FLOAT.S p. 346 scales by 2^-t, TRUNC.S p. 548 by 2^+t"),
            ),
            // The four signed conversions and `UTRUNC.S` all specify the
            // saturating answer explicitly, per instruction, on their own pages.
            float_to_int_out_of_range: Unknown::measured(
                "float_to_int_out_of_range",
                "vector family F6 (conversions, boundaries)",
                OutOfRangeRule::Saturate,
                isa_rm!(
                    "TRUNC.S p. 548 / ROUND.S p. 497 / FLOOR.S p. 347 / CEIL.S \
                     p. 311: overflow and infinity return the destination extreme"
                ),
            ),
            // Same pages: NaN takes the *positive* extreme, not zero and not the
            // negative one. `UTRUNC.S` p. 555 gives the unsigned counterpart
            // (0xffff_ffff), which `executor/float_math.rs` applies separately.
            float_to_int_nan: Unknown::measured(
                "float_to_int_nan",
                "vector family F6 (conversions, NaN)",
                0x7FFF_FFFF,
                isa_rm!("TRUNC.S p. 548 and the other signed conversions: NaN returns 0x7fffffff"),
            ),
            // The odd one out, and the reason this field is a value rather than
            // a rule: a negative input does not saturate to zero.
            utrunc_negative: Unknown::measured(
                "utrunc_negative",
                "vector family F6 (conversions, unsigned)",
                0x8000_0000,
                isa_rm!("UTRUNC.S p. 555: negative numbers and -inf return 0x80000000"),
            ),
            round_s_ties: Unknown::unknown("round_s_ties", "vector family F6 (conversions, ties)"),
            const_s_table: Unknown::unknown(
                "const_s_table",
                "vector family F5 (divide/sqrt sequence inputs)",
            ),
            fsr_flag_bits: Unknown::unknown(
                "fsr_flag_bits",
                "the FSR column of every vector family (the layout is now \
                 architectural — see cpu::FSR_*; what is open is which \
                 operation raises which flag)",
            ),
            fsr_sticky: Unknown::measured(
                "fsr_sticky",
                "the FSR column of every vector family",
                true,
                "M6 P1 desk session 2026-07-31: FSR read 0 on a fresh boot and \
                 0x400 after a 24-instruction FP sweep with no intervening write \
                 (p1-silicon-results.md)",
            ),
            // §4.3.11.2 (p. 69) says the ISA includes IEEE754 signed zero,
            // infinity, quiet NaN and sub-normal handling but **not** signaling
            // NaNs or exceptions, and §4.3.11.4 (p. 71) adds that current
            // implementations raise none. A bit pattern with the quiet bit clear
            // is therefore just a NaN here; nothing signals on it.
            snan_compare_signals: Unknown::measured(
                "snan_compare_signals",
                "vector family F2 (NaN payloads, compare half)",
                false,
                isa_rm!("§4.3.11.2 p. 69: the ISA has no IEEE754 signaling NaNs or exceptions"),
            ),
            // The RM fixes the field's *encoding* (Table 4-47, p. 69-70 — now
            // in `cpu.rs` as `FCR_RM_*`) and names it for `FLOAT.S`/`UFLOAT.S`
            // only. It never says which arithmetic instructions consult it, and
            // §4.3.11.4 (p. 71) shows the document is willing to describe
            // architectural machinery that current implementations do not
            // provide. Whether add.s/sub.s/mul.s on *this* silicon round
            // differently under RM != 0 is therefore still a measurement, and
            // implementing three directed-rounding modes on the strength of an
            // unnamed "various instructions" would be a guess with 1944 corpus
            // rows riding on it.
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
        let err = std::panic::catch_unwind(|| *p.nan_propagation.get()).expect_err("must panic");
        let msg = err
            .downcast_ref::<String>()
            .expect("the panic payload is the message");
        assert_eq!(parse_unresolved(msg), Some("nan_propagation"));
        assert!(
            msg.contains("vector family F2"),
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
            vec![
                "madd_fused",
                "conversion_scale",
                "float_to_int_out_of_range",
                "float_to_int_nan",
                "utrunc_negative",
                "fsr_sticky",
                "snan_compare_signals",
            ],
            "the settled list is pinned so a field cannot acquire a value \
             quietly; if this changed, the new entry needs a citation and an \
             ADR row"
        );
    }

    /// Every settled row must say *where* it came from, and the two provenances
    /// have to stay distinguishable: a manual reading is falsifiable by the P6
    /// campaign, a silicon measurement is the campaign. Collapsing them would
    /// hide which claims the board has actually tested.
    #[test]
    fn settled_rows_declare_manual_or_silicon_provenance() {
        let p = FpPolicy::m6();
        let mut from_manual = Vec::new();
        let mut from_silicon = Vec::new();
        for (name, _, cite) in p.inventory() {
            let Some(c) = cite else { continue };
            if c.contains("ISA Reference Manual") {
                from_manual.push(name);
            } else if c.contains("desk session") {
                from_silicon.push(name);
            } else {
                panic!("{name}'s citation names neither the manual nor a desk session: {c}");
            }
        }
        assert_eq!(from_silicon, vec!["fsr_sticky"]);
        assert_eq!(from_manual.len(), 6, "manual-sourced rows: {from_manual:?}");
        // A manual citation without a page number is not a citation.
        for (name, _, cite) in p.inventory() {
            if let Some(c) = cite
                && c.contains("ISA Reference Manual")
            {
                assert!(
                    c.contains("p. ") || c.contains("§"),
                    "{name} cites the manual without a page or section: {c}"
                );
            }
        }
    }
}
