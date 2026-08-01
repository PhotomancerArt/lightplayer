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
//! At the end of M6 P3 almost every field was `Unknown` — deliberately. The
//! **P6 campaign closed all of them**: every field now carries a measurement
//! citation, two of them recording where silicon *falsified* the ISA
//! Reference Manual. The `Unknown` machinery stays, so any future field
//! arrives loud instead of defaulted.
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
    /// The **last** NaN operand (in `fs`, `ft` order) passes through with the
    /// quiet bit forced and the payload preserved. The measured rule.
    LastOperandQuieted,
    /// The first NaN operand, quieted.
    FirstOperandQuieted,
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

/// What `utrunc.s` does with a negative input.
///
/// The ISA RM claims a fixed `0x80000000` sentinel; silicon **falsified**
/// that for in-range negatives (16 DIVERGE rows, family F6), so this is a
/// rule rather than a value.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UtruncNegativeRule {
    /// Truncate toward zero; a negative integer result is returned as its
    /// two's-complement bits (saturating at `i32::MIN` below range), with
    /// INVALID raised — except when it truncates to zero, which is merely
    /// inexact. The measured behavior.
    WrapLikeSignedSaturating,
    /// The ISA RM's claim: a fixed sentinel for any negative input.
    Sentinel(u32),
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

/// The implementation-defined estimate ROMs behind `recip0.s`, `rsqrt0.s`,
/// `sqrt0.s`, and `div0.s` — **extracted from silicon** (D5).
///
/// The data and the bit-exact model live in [`crate::fp_rom`]; this policy
/// row records the provenance and the measured shape. The extraction found
/// only **three** underlying ROMs behind the four instructions: a 128-entry
/// table shared by `recip0.s`/`div0.s` (7 index bits) and an odd/even pair of
/// 64-entry tables shared by `rsqrt0.s`/`sqrt0.s` (6 index bits), each entry
/// carrying 7 result bits. The model reproduces every run of every captured
/// sweep — `tests/fp_silicon_replay.rs` re-derives it from the committed
/// capture on every test run, with no board.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct EstimateTables {
    /// Index width of the wide (recip/div) ROM, confirmed by sweeping.
    pub index_bits: u8,
    /// Where the committed capture lives, relative to `lp-xt-emu`.
    pub capture: &'static str,
}

/// Semantics of the non-estimate members of the divide/sqrt helper family:
/// `nexp01.s`, `mksadj.s`, `mkdadj.s`, `addexp.s`, `addexpm.s`, `maddn.s`,
/// `divn.s` — **measured on silicon**; no document covers them (the ISA RM's
/// Table 4-46 omits the whole family) and the license rules keep binutils,
/// GCC, and QEMU source off the table.
///
/// The implementations live in [`crate::fp_rom`] and
/// `executor/float_math.rs`; this row records the provenance. The honest
/// caveat: `divn.s` is modeled as the fused accumulate, which is exact on the
/// divide/sqrt-sequence envelope (all 272 end-to-end sequence rows) but not
/// across its full probe grid — the campaign record documents the
/// off-envelope behavior it measured.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DivideStepSemantics {
    /// Where the measurements live.
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
    /// What `utrunc.s` produces for a negative input. Began life as the ISA
    /// RM's fixed sentinel; the campaign **falsified** that on silicon, so it
    /// is a rule again — see [`UtruncNegativeRule`].
    pub utrunc_negative: Unknown<UtruncNegativeRule>,
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

/// Citation stem for every field closed by the M6 P6 silicon campaign. One
/// macro so the campaign rows are greppable, exactly like [`isa_rm!`].
/// The board: ESP32-S3 chip rev v0.2, MAC `d8:3b:da:47:29:70`, desk session
/// 2026-07-31; captures committed under `tests/fixtures/fp/captures/`.
macro_rules! silicon {
    ($what:literal) => {
        concat!(
            "M6 P6 desk session 2026-07-31 (ESP32-S3 d8:3b:da:47:29:70) — ",
            $what
        )
    };
}

impl FpPolicy {
    /// The policy after the M6 P6 campaign: **every field is measured**, from
    /// the ISA Reference Manual where it spoke (and silicon agreed), from
    /// silicon alone everywhere else — including two rows where silicon
    /// **falsified** the manual (`utrunc_negative`, `snan_compare_signals`).
    pub const fn m6() -> FpPolicy {
        FpPolicy {
            nan_propagation: Unknown::measured(
                "nan_propagation",
                "vector family F2 (NaN payloads)",
                NanRule::LastOperandQuieted,
                silicon!(
                    "family F2: all 540 propagation rows show the LAST NaN \
                     operand surviving with the quiet bit forced and the \
                     payload preserved (270 RESOLVED rows + the qNaN rows \
                     that agreed by construction)"
                ),
            ),
            default_generated_nan: Unknown::measured(
                "default_generated_nan",
                "vector family F2 (hardware-generated NaNs)",
                0x7FC0_0000,
                silicon!(
                    "families F2 (10 generated-NaN rows) and F4 (8 rows of \
                     0 * inf): every hardware-generated NaN is 0x7fc00000, \
                     with FSR INVALID"
                ),
            ),
            // The RM's sub-normal clause was deliberately NOT read as an
            // answer (it describes the option, not this implementation).
            // Silicon answered: full IEEE subnormal arithmetic, no flushing
            // anywhere.
            flush_input_denormals: Unknown::measured(
                "flush_input_denormals",
                "vector family F3 (denormals, input half)",
                false,
                silicon!(
                    "family F3: every one of 350 rows is consistent with full \
                     IEEE subnormal arithmetic; the 80 rows where flushing \
                     would change the answer all came back IEEE (e.g. \
                     max-subnormal + max-subnormal = 0x00fffffe, a normal)"
                ),
            ),
            flush_output_denormals: Unknown::measured(
                "flush_output_denormals",
                "vector family F3 (denormals, output half)",
                false,
                silicon!(
                    "family F3, output half: subnormal results come back with \
                     their full subnormal bit patterns, never as zero (32 \
                     RESOLVED rows, all IEEE)"
                ),
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
            // The campaign's headline falsification: the RM's fixed sentinel
            // (UTRUNC.S p. 555, "negative numbers and -inf return
            // 0x80000000") is only true below i32::MIN. In range, silicon
            // wraps the signed truncation — the P5 prediction produced 16
            // DIVERGE rows, every one on an in-range negative.
            utrunc_negative: Unknown::measured(
                "utrunc_negative",
                "vector family F6 (conversions, unsigned)",
                UtruncNegativeRule::WrapLikeSignedSaturating,
                silicon!(
                    "family F6, 16 DIVERGE rows: utrunc.s(-1.5) = 0xffffffff, \
                     utrunc.s(-1.9 * 2^15) = 0xffff0ccd, utrunc.s(-0.5) = 0 \
                     (inexact only); the RM sentinel survives only below \
                     i32::MIN. RM FALSIFIED for in-range negatives"
                ),
            ),
            round_s_ties: Unknown::measured(
                "round_s_ties",
                "vector family F6 (conversions, ties)",
                TiesRule::ToEven,
                silicon!(
                    "family F6: round.s(0.5) = 0, round.s(1.5) = 2, \
                     round.s(2.5) = 2, and the negative mirrors — ties to even"
                ),
            ),
            const_s_table: Unknown::measured(
                "const_s_table",
                "the helpers capture (const.s 0..=15)",
                [
                    0x0000_0000,
                    0x3F80_0000,
                    0x4000_0000,
                    0x3F00_0000,
                    0x0000_0000,
                    0x3F80_0000,
                    0x4000_0000,
                    0x3F00_0000,
                    0x0000_0000,
                    0x3F80_0000,
                    0x4000_0000,
                    0x3F00_0000,
                    0x0000_0000,
                    0x3F80_0000,
                    0x4000_0000,
                    0x3F00_0000,
                ],
                silicon!(
                    "helpers capture: const.s produces [0.0, 1.0, 2.0, 0.5] \
                     selected by imm & 3, all sixteen selectors measured"
                ),
            ),
            fsr_flag_bits: Unknown::measured(
                "fsr_flag_bits",
                "the FSR column of every vector family",
                FsrFlagBits {
                    invalid: crate::cpu::FSR_INVALID,
                    div_by_zero: crate::cpu::FSR_DIV_BY_ZERO,
                    overflow: crate::cpu::FSR_OVERFLOW,
                    underflow: crate::cpu::FSR_UNDERFLOW,
                    inexact: crate::cpu::FSR_INEXACT,
                },
                silicon!(
                    "the FSR column of all 5630 family rows + 5328 helper \
                     probes: INEXACT on rounded results, UNDERFLOW on \
                     tiny-and-inexact, OVERFLOW with INEXACT, INVALID on \
                     sNaN operands / NaN generation / invalid conversions / \
                     olt.s+ole.s with any NaN; DIV_BY_ZERO from the \
                     reciprocal estimates on zero. maddn.s and the exponent \
                     helpers are flag-silent. The op-class mapping is \
                     implemented in executor/float_math.rs and fp_rom.rs and \
                     replayed against the captures. RM §4.3.11.4 (no flags \
                     at all) FALSIFIED"
                ),
            ),
            fsr_sticky: Unknown::measured(
                "fsr_sticky",
                "the FSR column of every vector family",
                true,
                "M6 P1 desk session 2026-07-31: FSR read 0 on a fresh boot and \
                 0x400 after a 24-instruction FP sweep with no intervening write \
                 (p1-silicon-results.md)",
            ),
            // The RM (§4.3.11.2 p. 69) claims the ISA has no signaling-NaN
            // support and §4.3.11.4 that implementations raise nothing.
            // Silicon disagrees: an sNaN operand raises FSR INVALID on every
            // compare (and every arithmetic op), and olt.s/ole.s raise it on
            // quiet NaNs too — IEEE's signaling-predicate rule, working.
            snan_compare_signals: Unknown::measured(
                "snan_compare_signals",
                "vector family F2 (NaN payloads, compare half)",
                true,
                silicon!(
                    "family F2: every sNaN compare row reads FSR 0x800, and \
                     olt.s/ole.s read it on qNaN rows as well (oeq.s and the \
                     unordered forms stay silent). RM §4.3.11.2/4 FALSIFIED"
                ),
            ),
            // The question D6 was written for. Answered: FCR.RM is real.
            fcr_rounding_honored: Unknown::measured(
                "fcr_rounding_honored",
                "vector family F1 (rounding, all four FCR modes)",
                true,
                silicon!(
                    "family F1: 556 of 648 operand groups produce \
                     mode-dependent results, and all 1944 non-default-mode \
                     rows match IEEE-754 directed rounding exactly. Directed \
                     modes are implemented for add.s/sub.s/mul.s (the \
                     measured surface) and refused elsewhere"
                ),
            ),
            estimates: Unknown::measured(
                "estimates",
                "the M6 P6 exhaustive estimate-table extraction",
                EstimateTables {
                    index_bits: 7,
                    capture: "tests/fixtures/fp/captures/tables.txt",
                },
                silicon!(
                    "60 RLE sweeps of the full 2^23 significand space over 15 \
                     (sign, exponent) planes per op; three underlying ROMs \
                     (recip/div shared 128-entry, rsqrt/sqrt odd+even \
                     64-entry); fp_rom.rs reproduces every run of every \
                     sweep — replayed boardlessly by fp_silicon_replay.rs"
                ),
            ),
            divide_step_helpers: Unknown::measured(
                "divide_step_helpers",
                "the M6 P6 helper probe grids + end-to-end sequences",
                DivideStepSemantics {
                    sourced_from: "tests/fixtures/fp/captures/helpers.txt + the \
                                   div/sqrt sequence rows of families.txt",
                },
                silicon!(
                    "5328 probe points: nexp01/mksadj/mkdadj/addexp/addexpm \
                     reproduce 144/144 each, maddn.s is bit-identical to \
                     madd.s on all 1536 (and flag-silent); divn.s is modeled \
                     as the fused accumulate — exact on the sequence \
                     envelope, with off-envelope behavior recorded, not \
                     modeled"
                ),
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
        // No field of the shipped policy is unresolved anymore, so the panic
        // machinery is exercised on a synthetic field — it still guards any
        // future field that arrives without a measurement.
        let u: Unknown<bool> = Unknown::unknown("synthetic_field", "a future campaign");
        let err = std::panic::catch_unwind(|| *u.get()).expect_err("must panic");
        let msg = err
            .downcast_ref::<String>()
            .expect("the panic payload is the message");
        assert_eq!(parse_unresolved(msg), Some("synthetic_field"));
        assert!(
            msg.contains("a future campaign"),
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
        let unresolved: Vec<_> = inv
            .iter()
            .filter(|(_, _, c)| c.is_none())
            .map(|(n, ..)| *n)
            .collect();
        assert!(
            unresolved.is_empty(),
            "after the P6 campaign every field is measured; {unresolved:?} \
             lost its citation"
        );
    }

    /// Every settled row must say *where* it came from, and the two provenances
    /// have to stay distinguishable: a manual reading is falsifiable by the
    /// campaign, a silicon measurement *is* the campaign. After P6 the split
    /// is pinned: four rows stand on the manual (each silicon-confirmed by
    /// the families run), thirteen on silicon — including the two rows where
    /// silicon falsified the manual.
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
        assert_eq!(
            from_manual,
            vec![
                "madd_fused",
                "conversion_scale",
                "float_to_int_out_of_range",
                "float_to_int_nan",
            ],
            "manual-sourced rows are pinned"
        );
        assert_eq!(from_silicon.len(), 13, "silicon rows: {from_silicon:?}");
        // The falsified rows must say so, loudly and durably.
        for name in ["utrunc_negative", "snan_compare_signals", "fsr_flag_bits"] {
            let (_, _, cite) = p
                .inventory()
                .into_iter()
                .find(|(n, ..)| *n == name)
                .unwrap();
            assert!(
                cite.unwrap().contains("FALSIFIED"),
                "{name} falsified the RM and its citation must record that"
            );
        }
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
