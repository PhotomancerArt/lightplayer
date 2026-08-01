#![no_std]
#![forbid(unsafe_code)]
//! The M6 Xtensa FP conformance corpus: a deterministic, **float-free**,
//! `no_std` generator for the vectors the emulator predicts and the desk
//! ESP32-S3 answers.
//!
//! # Three properties, each for a reason
//!
//! **Deterministic, with no dependencies.** The PRNG is written out below
//! rather than pulled from `rand`, and the crate has no dependencies at all.
//! Determinism is the whole contract: the host predicts vector 41 337 and the
//! device runs vector 41 337, and if those are not the same vector the campaign
//! measures nothing.
//!
//! **Float-free.** The generator emits raw `u32` bit patterns and performs no
//! floating-point arithmetic anywhere. If it built its inputs with `f32` it
//! would be constructing them with the very semantics under test — and on the
//! device side it would need the FPU it is trying to characterize. There is a
//! test at the bottom of this file that reads the crate's own source and fails
//! if `f32` or `f64` appears in a code position, so this is a checked property
//! and not a promise.
//!
//! **Index-addressable and pure.** [`vector`] is a function of `(family,
//! index)` with no state, so P6 can re-run one vector alone while bisecting a
//! divergence instead of replaying a batch to reach it.
//!
//! # The same code on both sides
//!
//! This crate compiles into `fw-esp32s3`'s conformance harness as well as into
//! host tests, so the device **regenerates** its inputs rather than receiving
//! them: no vector transfer protocol, no reflash per batch. [`fingerprint`]
//! makes that checkable rather than assumed — both sides print it and the
//! campaign aborts on a mismatch.
//!
//! # The six families
//!
//! | ID | [`Family`] | Reaches |
//! |---|---|---|
//! | F1 | [`Family::Rounding`] | exact ties and near-ties for add/sub/mul, replayed under all four FCR modes |
//! | F2 | [`Family::NanPayload`] | every binary op × {qNaN, sNaN} × {A, B, both} × several payloads, plus hardware-*generated* NaNs |
//! | F3 | [`Family::Denormal`] | subnormal-in/normal-out, normal-in/subnormal-out, and both-subnormal — kept separable |
//! | F4 | [`Family::SignedZero`] | ±0 through negate, abs, multiply, add, the compares, and the conversions |
//! | F5 | [`Family::DivSqrt`] | the estimate instructions, and the manual's divide/sqrt sequences over a wide operand sweep |
//! | F6 | [`Family::Convert`] | int↔float at the boundaries, plus a scale-immediate sweep |
//!
//! Sizes are chosen by "cheap to get wrong, expensive to discover later", not by
//! volume. F5's estimate block here is a *representative* sweep; the exhaustive
//! table extraction is a separate P6 mechanism, because sampling a lookup ROM
//! would let the emulator be merely close.

pub mod helpers;

// ---------------------------------------------------------------------------
// Families and operations
// ---------------------------------------------------------------------------

/// One vector family. The discriminants are stable: they appear in the corpus
/// files and in the device's serial output.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Family {
    Rounding = 1,
    NanPayload = 2,
    Denormal = 3,
    SignedZero = 4,
    DivSqrt = 5,
    Convert = 6,
}

impl Family {
    /// Every family, in corpus order.
    pub const ALL: [Family; 6] = [
        Family::Rounding,
        Family::NanPayload,
        Family::Denormal,
        Family::SignedZero,
        Family::DivSqrt,
        Family::Convert,
    ];

    /// The `F1`..`F6` label used in the plan, the ADR, and the file names.
    pub const fn label(self) -> &'static str {
        match self {
            Family::Rounding => "F1",
            Family::NanPayload => "F2",
            Family::Denormal => "F3",
            Family::SignedZero => "F4",
            Family::DivSqrt => "F5",
            Family::Convert => "F6",
        }
    }

    /// The lowercase file-stem name (`rounding`, `nan_payload`, …).
    pub const fn name(self) -> &'static str {
        match self {
            Family::Rounding => "rounding",
            Family::NanPayload => "nan_payload",
            Family::Denormal => "denormal",
            Family::SignedZero => "signed_zero",
            Family::DivSqrt => "div_sqrt",
            Family::Convert => "convert",
        }
    }

    /// Parse a [`Family::name`].
    pub fn from_name(s: &str) -> Option<Family> {
        Family::ALL.into_iter().find(|f| f.name() == s)
    }
}

/// The operation a vector exercises.
///
/// Mostly one Xtensa instruction each. Two are **pseudo-ops**: divide and square
/// root are not instructions on this chip but code sequences, so [`OpCode::Div`]
/// and [`OpCode::Sqrt`] name the sequence and the two sides implement it —
/// the device from the manual's sequence, the emulator from its helper
/// executors. Both currently answer `UNKNOWN`, which is the honest state.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum OpCode {
    AddS = 1,
    SubS = 2,
    MulS = 3,
    MaddS = 4,
    MsubS = 5,
    AbsS = 6,
    NegS = 7,
    MovS = 8,
    OeqS = 9,
    OltS = 10,
    OleS = 11,
    UeqS = 12,
    UltS = 13,
    UleS = 14,
    UnS = 15,
    FloatS = 16,
    UfloatS = 17,
    TruncS = 18,
    UtruncS = 19,
    RoundS = 20,
    FloorS = 21,
    CeilS = 22,
    Recip0S = 23,
    Rsqrt0S = 24,
    Sqrt0S = 25,
    Div0S = 26,
    /// Pseudo-op: the manual's divide sequence, `a / b`.
    Div = 27,
    /// Pseudo-op: the manual's square-root sequence, `sqrt(a)`.
    Sqrt = 28,
}

impl OpCode {
    /// How many of `a`, `b`, `c` are meaningful.
    pub const fn arity(self) -> u8 {
        match self {
            OpCode::AbsS
            | OpCode::NegS
            | OpCode::MovS
            | OpCode::FloatS
            | OpCode::UfloatS
            | OpCode::TruncS
            | OpCode::UtruncS
            | OpCode::RoundS
            | OpCode::FloorS
            | OpCode::CeilS
            | OpCode::Recip0S
            | OpCode::Rsqrt0S
            | OpCode::Sqrt0S
            | OpCode::Div0S
            | OpCode::Sqrt => 1,
            OpCode::MaddS | OpCode::MsubS => 3,
            _ => 2,
        }
    }

    /// Whether the result is a boolean register rather than a float register.
    pub const fn writes_boolean(self) -> bool {
        matches!(
            self,
            OpCode::OeqS
                | OpCode::OltS
                | OpCode::OleS
                | OpCode::UeqS
                | OpCode::UltS
                | OpCode::UleS
                | OpCode::UnS
        )
    }

    /// Whether the result is an address register (a float→int conversion).
    pub const fn writes_integer(self) -> bool {
        matches!(
            self,
            OpCode::TruncS | OpCode::UtruncS | OpCode::RoundS | OpCode::FloorS | OpCode::CeilS
        )
    }

    /// The objdump-style mnemonic, for corpus files and diagnostics.
    pub const fn name(self) -> &'static str {
        match self {
            OpCode::AddS => "add.s",
            OpCode::SubS => "sub.s",
            OpCode::MulS => "mul.s",
            OpCode::MaddS => "madd.s",
            OpCode::MsubS => "msub.s",
            OpCode::AbsS => "abs.s",
            OpCode::NegS => "neg.s",
            OpCode::MovS => "mov.s",
            OpCode::OeqS => "oeq.s",
            OpCode::OltS => "olt.s",
            OpCode::OleS => "ole.s",
            OpCode::UeqS => "ueq.s",
            OpCode::UltS => "ult.s",
            OpCode::UleS => "ule.s",
            OpCode::UnS => "un.s",
            OpCode::FloatS => "float.s",
            OpCode::UfloatS => "ufloat.s",
            OpCode::TruncS => "trunc.s",
            OpCode::UtruncS => "utrunc.s",
            OpCode::RoundS => "round.s",
            OpCode::FloorS => "floor.s",
            OpCode::CeilS => "ceil.s",
            OpCode::Recip0S => "recip0.s",
            OpCode::Rsqrt0S => "rsqrt0.s",
            OpCode::Sqrt0S => "sqrt0.s",
            OpCode::Div0S => "div0.s",
            OpCode::Div => "<div-sequence>",
            OpCode::Sqrt => "<sqrt-sequence>",
        }
    }

    /// Parse a [`OpCode::name`].
    pub fn from_name(s: &str) -> Option<OpCode> {
        ALL_OPS.into_iter().find(|o| o.name() == s)
    }
}

const ALL_OPS: [OpCode; 28] = [
    OpCode::AddS,
    OpCode::SubS,
    OpCode::MulS,
    OpCode::MaddS,
    OpCode::MsubS,
    OpCode::AbsS,
    OpCode::NegS,
    OpCode::MovS,
    OpCode::OeqS,
    OpCode::OltS,
    OpCode::OleS,
    OpCode::UeqS,
    OpCode::UltS,
    OpCode::UleS,
    OpCode::UnS,
    OpCode::FloatS,
    OpCode::UfloatS,
    OpCode::TruncS,
    OpCode::UtruncS,
    OpCode::RoundS,
    OpCode::FloorS,
    OpCode::CeilS,
    OpCode::Recip0S,
    OpCode::Rsqrt0S,
    OpCode::Sqrt0S,
    OpCode::Div0S,
    OpCode::Div,
    OpCode::Sqrt,
];

/// One test case. All operand fields are **raw bit patterns**, never values.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Vector {
    pub family: Family,
    pub index: u32,
    pub op: OpCode,
    /// First operand's bits. For `float.s`/`ufloat.s` this is the integer.
    pub a: u32,
    /// Second operand's bits (see [`OpCode::arity`]).
    pub b: u32,
    /// Accumulator's bits, for `madd.s`/`msub.s`.
    pub c: u32,
    /// The conversion instructions' 0..=15 binary scale.
    pub imm: u8,
    /// The `FCR` rounding-mode field to install before the operation. Non-zero
    /// only in [`Family::Rounding`]; `docs/design/float.md` §2 puts it out of
    /// shader reach, and M6 measures whether silicon honors it at all.
    pub fcr: u8,
}

// ---------------------------------------------------------------------------
// Bit construction — integer arithmetic only
// ---------------------------------------------------------------------------

/// Assemble a binary32 bit pattern from its fields.
const fn bits(sign: u32, exp: u32, frac: u32) -> u32 {
    (sign << 31) | ((exp & 0xFF) << 23) | (frac & 0x007F_FFFF)
}

/// A deterministic counter-based hash. Written out here rather than taken from a
/// dependency so both sides of the campaign cannot drift, and so the corpus is
/// reproducible from this file alone.
pub(crate) const fn mix(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846C_A68B);
    x ^= x >> 16;
    x
}

// ---------------------------------------------------------------------------
// F1 — rounding
// ---------------------------------------------------------------------------

/// Biased exponents big enough that half an ulp is still a normal number
/// (`exp - 24 >= 1`), so the tie construction below is exact.
const F1_EXPS: [u32; 6] = [127, 130, 100, 150, 60, 200];
/// Significands chosen so the round-to-nearest-**even** tie-break is exercised
/// in both directions: even lsb rounds down, odd lsb rounds up.
const F1_FRACS: [u32; 6] = [0, 1, 0x40_0000, 0x7F_FFFF, 2, 3];
/// Exact tie, one ulp below it, one ulp above it.
const F1_OFFSETS: u32 = 3;

const F1_PAIRS: u32 = 6 * 6 * 2 * F1_OFFSETS;
const F1_OPS: [OpCode; 3] = [OpCode::AddS, OpCode::SubS, OpCode::MulS];
const F1_COUNT: u32 = 3 * 4 * F1_PAIRS;

fn f1(index: u32) -> Vector {
    let pair = index % F1_PAIRS;
    let rest = index / F1_PAIRS;
    let fcr = (rest % 4) as u8;
    let op = F1_OPS[(rest / 4) as usize];

    let off = pair % F1_OFFSETS;
    let p = pair / F1_OFFSETS;
    let sign = p % 2;
    let p = p / 2;
    let frac = F1_FRACS[(p % 6) as usize];
    let exp = F1_EXPS[(p / 6) as usize];

    let a = bits(sign, exp, frac);
    // Half an ulp of `a`: `a`'s ulp has exponent `exp - 23`, so half of it has
    // exponent `exp - 24`. `a + b` therefore lands exactly on a midpoint.
    let tie = bits(sign, exp - 24, 0);
    let b = match off {
        0 => tie,
        1 => tie - 1, // the next representable below — just short of the tie
        _ => tie + 1, // just past it
    };
    Vector {
        family: Family::Rounding,
        index,
        op,
        a,
        b,
        c: 0,
        imm: 0,
        fcr,
    }
}

// ---------------------------------------------------------------------------
// F2 — NaN payloads
// ---------------------------------------------------------------------------

/// Both NaN kinds (quiet bit set and clear), several distinct payloads, both
/// signs — so "which operand's payload survives" is answerable rather than
/// inferable.
const F2_NANS: [u32; 6] = [
    0x7FC0_0000, // canonical qNaN
    0x7FD5_AA55, // qNaN, distinctive payload
    0xFFC0_0000, // negative qNaN
    0x7F80_0001, // sNaN, minimal payload
    0x7FA5_A5A5, // sNaN, distinctive payload
    0xFF80_0001, // negative sNaN
];
const F2_PARTNERS: [u32; 3] = [0x3F80_0000, 0x4049_0FDB, 0xC000_0000];
const F2_OPS: [OpCode; 12] = [
    OpCode::AddS,
    OpCode::SubS,
    OpCode::MulS,
    OpCode::MaddS,
    OpCode::MsubS,
    OpCode::OeqS,
    OpCode::OltS,
    OpCode::OleS,
    OpCode::UeqS,
    OpCode::UltS,
    OpCode::UleS,
    OpCode::UnS,
];
const F2_PRODUCT: u32 = 12 * 6 * 3 * 3;

/// NaNs the hardware *generates* from non-NaN operands. Their result is the
/// default generated NaN, which is the second thing F2 has to learn.
const F2_GENERATED: [(OpCode, u32, u32); 10] = [
    (OpCode::AddS, 0x7F80_0000, 0xFF80_0000), //  inf + -inf
    (OpCode::AddS, 0xFF80_0000, 0x7F80_0000), // -inf +  inf
    (OpCode::SubS, 0x7F80_0000, 0x7F80_0000), //  inf -  inf
    (OpCode::SubS, 0xFF80_0000, 0xFF80_0000), // -inf - -inf
    (OpCode::MulS, 0x7F80_0000, 0x0000_0000), //  inf *  0
    (OpCode::MulS, 0x0000_0000, 0x7F80_0000),
    (OpCode::MulS, 0xFF80_0000, 0x0000_0000),
    (OpCode::MulS, 0x0000_0000, 0xFF80_0000),
    (OpCode::MulS, 0x7F80_0000, 0x8000_0000), //  inf * -0
    (OpCode::MulS, 0x8000_0000, 0x7F80_0000),
];
const F2_COUNT: u32 = F2_PRODUCT + 10;

fn f2(index: u32) -> Vector {
    if index >= F2_PRODUCT {
        let (op, a, b) = F2_GENERATED[(index - F2_PRODUCT) as usize];
        return Vector {
            family: Family::NanPayload,
            index,
            op,
            a,
            b,
            c: 0x3F80_0000,
            imm: 0,
            fcr: 0,
        };
    }
    let position = index % 3;
    let p = index / 3;
    let partner = F2_PARTNERS[(p % 3) as usize];
    let p = p / 3;
    let nan = F2_NANS[(p % 6) as usize];
    let op = F2_OPS[(p / 6) as usize];
    // `other` is the second NaN when both operands are NaN, so the two are
    // distinguishable and the answer is not ambiguous.
    let other = F2_NANS[((p % 6) as usize + 1) % 6];
    let (a, b) = match position {
        0 => (nan, partner),
        1 => (partner, nan),
        _ => (nan, other),
    };
    Vector {
        family: Family::NanPayload,
        index,
        op,
        a,
        b,
        // madd/msub need a third operand; a plain 1.0 keeps the NaN question
        // about `a` and `b`.
        c: 0x3F80_0000,
        imm: 0,
        fcr: 0,
    }
}

// ---------------------------------------------------------------------------
// F3 — denormals
// ---------------------------------------------------------------------------

const F3_SUBS: [u32; 5] = [
    0x0000_0001, // min subnormal
    0x0000_0002,
    0x003F_FFFF,
    0x0040_0000,
    0x007F_FFFF, // max subnormal
];
const F3_NORMS: [u32; 5] = [
    0x0080_0000, // min normal
    0x3F80_0000, // 1.0
    0x0080_0001,
    0x3400_0000, // 2^-23
    0x3F00_0000, // 0.5
];
/// Small normals whose product with a small factor lands in subnormal range.
const F3_SMALL_NORMS: [u32; 5] = [
    0x0080_0000,
    0x0080_0001,
    0x0100_0000,
    0x008F_FFFF,
    0x0200_0000,
];
const F3_FACTORS: [u32; 5] = [
    0x3F00_0000, // 0.5
    0x3E80_0000, // 0.25
    0x3400_0000, // 2^-23
    0x3380_0000, // 2^-24
    0x0C80_0000, // 2^-102
];
const F3_OPS: [OpCode; 3] = [OpCode::AddS, OpCode::SubS, OpCode::MulS];

const F3_A: u32 = 5 * 2 * 5 * 3; // subnormal in, normal partner
const F3_B: u32 = 5 * 2 * 5; // normal in, subnormal out (mul only)
const F3_C: u32 = 5 * 2 * 5 * 3; // both subnormal
const F3_COUNT: u32 = F3_A + F3_B + F3_C;

fn f3(index: u32) -> Vector {
    let (op, a, b) = if index < F3_A {
        let i = index;
        let op = F3_OPS[(i % 3) as usize];
        let i = i / 3;
        let b = F3_NORMS[(i % 5) as usize];
        let i = i / 5;
        let sign = i % 2;
        let a = F3_SUBS[(i / 2) as usize] | (sign << 31);
        (op, a, b)
    } else if index < F3_A + F3_B {
        let i = index - F3_A;
        let b = F3_FACTORS[(i % 5) as usize];
        let i = i / 5;
        let sign = i % 2;
        let a = F3_SMALL_NORMS[(i / 2) as usize] | (sign << 31);
        (OpCode::MulS, a, b)
    } else {
        let i = index - F3_A - F3_B;
        let op = F3_OPS[(i % 3) as usize];
        let i = i / 3;
        let b = F3_SUBS[(i % 5) as usize];
        let i = i / 5;
        let sign = i % 2;
        let a = F3_SUBS[(i / 2) as usize] | (sign << 31);
        (op, a, b)
    };
    Vector {
        family: Family::Denormal,
        index,
        op,
        a,
        b,
        c: 0,
        imm: 0,
        fcr: 0,
    }
}

// ---------------------------------------------------------------------------
// F4 — signed zero
// ---------------------------------------------------------------------------

const F4_ZEROS: [u32; 2] = [0x0000_0000, 0x8000_0000];
const F4_PARTNERS: [u32; 8] = [
    0x0000_0000,
    0x8000_0000,
    0x3F80_0000,
    0xBF80_0000,
    0x7F80_0000,
    0xFF80_0000,
    0x0080_0000,
    0x8080_0000,
];
const F4_UNARY: [OpCode; 3] = [OpCode::NegS, OpCode::AbsS, OpCode::MovS];
const F4_BINARY: [OpCode; 6] = [
    OpCode::AddS,
    OpCode::SubS,
    OpCode::MulS,
    OpCode::OeqS,
    OpCode::OltS,
    OpCode::OleS,
];
const F4_CONVERT: [OpCode; 4] = [
    OpCode::TruncS,
    OpCode::RoundS,
    OpCode::FloorS,
    OpCode::CeilS,
];

const F4_U: u32 = 3 * 2;
const F4_B: u32 = 6 * 2 * 8 * 2; // both operand orders
const F4_C: u32 = 4 * 2;
const F4_COUNT: u32 = F4_U + F4_B + F4_C;

fn f4(index: u32) -> Vector {
    let (op, a, b) = if index < F4_U {
        let op = F4_UNARY[(index % 3) as usize];
        (op, F4_ZEROS[(index / 3) as usize], 0)
    } else if index < F4_U + F4_B {
        let i = index - F4_U;
        let swapped = i % 2 == 1;
        let i = i / 2;
        let partner = F4_PARTNERS[(i % 8) as usize];
        let i = i / 8;
        let zero = F4_ZEROS[(i % 2) as usize];
        let op = F4_BINARY[(i / 2) as usize];
        if swapped {
            (op, partner, zero)
        } else {
            (op, zero, partner)
        }
    } else {
        let i = index - F4_U - F4_B;
        let op = F4_CONVERT[(i % 4) as usize];
        (op, F4_ZEROS[(i / 4) as usize], 0)
    };
    Vector {
        family: Family::SignedZero,
        index,
        op,
        a,
        b,
        c: 0,
        imm: 0,
        fcr: 0,
    }
}

// ---------------------------------------------------------------------------
// F5 — divide and square root
// ---------------------------------------------------------------------------

const F5_EST_OPS: [OpCode; 4] = [
    OpCode::Recip0S,
    OpCode::Rsqrt0S,
    OpCode::Sqrt0S,
    OpCode::Div0S,
];
/// Four adjacent exponents, so the exponent rule's *separability* is visible;
/// two of them differ in parity, which is what `rsqrt0.s` keys on.
const F5_EST_EXPS: [u32; 4] = [126, 127, 128, 129];
/// A strided significand sweep. Representative, not exhaustive — the exhaustive
/// extraction is P6's own mechanism, because sampling a lookup ROM would only
/// ever make the emulator *close*.
const F5_EST_STEPS: u32 = 64;
const F5_EST: u32 = 4 * 4 * F5_EST_STEPS;

/// The sequence sweep's operands: powers of two (exact answers), values near 1,
/// very large and very small, a denormal, ±0, ±inf, and NaN.
const F5_SEQ_VALUES: [u32; 16] = [
    0x3F80_0000, // 1.0
    0x4000_0000, // 2.0
    0x3F00_0000, // 0.5
    0x4080_0000, // 4.0
    0x3F80_0001, // just above 1
    0x3F7F_FFFF, // just below 1
    0x7F7F_FFFF, // max normal
    0x0080_0000, // min normal
    0x0000_0001, // min subnormal
    0x0000_0000, // +0
    0x8000_0000, // -0
    0x7F80_0000, // +inf
    0xFF80_0000, // -inf
    0x7FC0_0000, // qNaN
    0xC049_0FDB, // -pi
    0x4049_0FDB, // pi
];
const F5_SEQ_DIV: u32 = 16 * 16;
const F5_SEQ_SQRT: u32 = 16;
const F5_COUNT: u32 = F5_EST + F5_SEQ_DIV + F5_SEQ_SQRT;

fn f5(index: u32) -> Vector {
    let (op, a, b) = if index < F5_EST {
        let i = index;
        let step = i % F5_EST_STEPS;
        let i = i / F5_EST_STEPS;
        let exp = F5_EST_EXPS[(i % 4) as usize];
        let op = F5_EST_OPS[(i / 4) as usize];
        // Stride the significand rather than sample it randomly: the estimate
        // is a step function over the significand, so an even stride shows the
        // step structure and a random scatter would not.
        (op, bits(0, exp, step * (0x80_0000 / F5_EST_STEPS)), 0)
    } else if index < F5_EST + F5_SEQ_DIV {
        let i = index - F5_EST;
        (
            OpCode::Div,
            F5_SEQ_VALUES[(i / 16) as usize],
            F5_SEQ_VALUES[(i % 16) as usize],
        )
    } else {
        let i = index - F5_EST - F5_SEQ_DIV;
        (OpCode::Sqrt, F5_SEQ_VALUES[i as usize], 0)
    };
    Vector {
        family: Family::DivSqrt,
        index,
        op,
        a,
        b,
        c: 0,
        imm: 0,
        fcr: 0,
    }
}

// ---------------------------------------------------------------------------
// F6 — conversions
// ---------------------------------------------------------------------------

const F6_INTS: [u32; 16] = [
    0,
    1,
    0xFFFF_FFFF, // -1 signed / u32::MAX unsigned
    2,
    0xFFFF_FFFE, // -2
    0x7FFF_FFFF, // i32::MAX
    0x8000_0000, // i32::MIN
    0x00FF_FFFF, // 2^24 - 1, the last exactly representable
    0x0100_0000, // 2^24
    0x0100_0001, // 2^24 + 1 — needs rounding
    0x0100_0003, // 2^24 + 3 — rounds the other way
    0x075B_CD15, // 123456789
    0xF8A4_32EB, // -123456789
    0x4000_0000,
    0xC000_0000,
    0x0000_00FF,
];
const F6_TO_INT_OPS: [OpCode; 5] = [
    OpCode::TruncS,
    OpCode::UtruncS,
    OpCode::RoundS,
    OpCode::FloorS,
    OpCode::CeilS,
];
const F6_FLOATS: [u32; 20] = [
    0x0000_0000, // +0
    0x8000_0000, // -0
    0x3F00_0000, // 0.5   — a tie for round.s
    0xBF00_0000, // -0.5  — a tie the other way
    0x3FC0_0000, // 1.5   — a tie
    0xBFC0_0000, // -1.5
    0x4020_0000, // 2.5   — a tie whose even neighbour is below
    0xC020_0000, // -2.5
    0x3FF3_3333, // 1.9
    0xBFF3_3333, // -1.9
    0x4F00_0000, // 2^31  — just out of i32 range
    0xCF00_0000, // -2^31 — exactly i32::MIN
    0x4EFF_FFFF, // 2^31 - 128, the largest in-range f32
    0x7149_F2CA, // 1e30
    0xF149_F2CA, // -1e30
    0x7F80_0000, // +inf
    0xFF80_0000, // -inf
    0x7FC0_0000, // NaN
    0x0000_0001, // min subnormal
    0x4F80_0000, // 2^32 — just out of u32 range
];
const F6_IMMS: [u8; 4] = [0, 1, 4, 15];

const F6_FROM_INT: u32 = 16 * 2 * 4;
const F6_TO_INT: u32 = 20 * 5 * 4;
const F6_COUNT: u32 = F6_FROM_INT + F6_TO_INT;

fn f6(index: u32) -> Vector {
    let (op, a, imm) = if index < F6_FROM_INT {
        let i = index;
        let imm = F6_IMMS[(i % 4) as usize];
        let i = i / 4;
        let op = if i % 2 == 0 {
            OpCode::FloatS
        } else {
            OpCode::UfloatS
        };
        (op, F6_INTS[(i / 2) as usize], imm)
    } else {
        let i = index - F6_FROM_INT;
        let imm = F6_IMMS[(i % 4) as usize];
        let i = i / 4;
        let op = F6_TO_INT_OPS[(i % 5) as usize];
        (op, F6_FLOATS[(i / 5) as usize], imm)
    };
    Vector {
        family: Family::Convert,
        index,
        op,
        a,
        b: 0,
        c: 0,
        imm,
        fcr: 0,
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// How many vectors a family has.
pub const fn count(family: Family) -> u32 {
    match family {
        Family::Rounding => F1_COUNT,
        Family::NanPayload => F2_COUNT,
        Family::Denormal => F3_COUNT,
        Family::SignedZero => F4_COUNT,
        Family::DivSqrt => F5_COUNT,
        Family::Convert => F6_COUNT,
    }
}

/// Every vector in the corpus.
pub const fn total() -> u32 {
    F1_COUNT + F2_COUNT + F3_COUNT + F4_COUNT + F5_COUNT + F6_COUNT
}

/// The vector at `index` within `family` — pure, so P6 can re-run one alone.
///
/// # Panics
/// If `index >= count(family)`.
pub fn vector(family: Family, index: u32) -> Vector {
    assert!(
        index < count(family),
        "vector index out of range for its family"
    );
    match family {
        Family::Rounding => f1(index),
        Family::NanPayload => f2(index),
        Family::Denormal => f3(index),
        Family::SignedZero => f4(index),
        Family::DivSqrt => f5(index),
        Family::Convert => f6(index),
    }
}

/// A hash over **every** vector of every family.
///
/// Both the host and the device print this, and the campaign aborts on a
/// mismatch. That is what turns "the same code generates both sides" from an
/// assumption into a check: a generator edit that silently invalidates a
/// committed corpus fails here rather than at the next desk session.
pub fn fingerprint() -> u32 {
    let mut h: u32 = 0x811C_9DC5;
    for family in Family::ALL {
        h = mix(h ^ family as u32);
        for i in 0..count(family) {
            let v = vector(family, i);
            for word in [
                v.op as u32,
                v.a,
                v.b,
                v.c,
                u32::from(v.imm),
                u32::from(v.fcr),
            ] {
                h = mix(h ^ word).wrapping_add(word);
            }
        }
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The generator must not perform floating-point arithmetic: it would be
    /// building its inputs with the semantics under test, and on the device it
    /// would need the very FPU it is characterizing. Asserted structurally
    /// rather than left to reviewer attention.
    #[test]
    fn the_generator_contains_no_floating_point_arithmetic() {
        let src = concat!(include_str!("lib.rs"), "\n", include_str!("helpers.rs"));
        for (n, line) in src.lines().enumerate() {
            // Strip line comments (`//`, `///`, `//!`) — prose mentions f32
            // constantly, and prose is not arithmetic.
            let code = line.split("//").next().unwrap_or("");
            // Spelled with `concat!` so this check does not trip over its own
            // source line.
            for tok in [concat!("f", "32"), concat!("f", "64")] {
                assert!(
                    !code.contains(tok),
                    "line {}: `{tok}` in a code position — the generator must \
                     stay float-free:\n{line}",
                    n + 1
                );
            }
        }
    }

    #[test]
    fn every_family_is_index_addressable_and_pure() {
        for family in Family::ALL {
            assert!(count(family) > 0, "{family:?} is empty");
            for i in [0, 1, count(family) / 2, count(family) - 1] {
                let a = vector(family, i);
                let b = vector(family, i);
                assert_eq!(a, b, "{family:?}[{i}] is not pure");
                assert_eq!(a.index, i);
                assert_eq!(a.family, family);
            }
        }
    }

    /// Drift is loud. If this number changes, every committed prediction file is
    /// invalid and `fp_conformance.rs` will say so — which is the point.
    #[test]
    fn the_fingerprint_is_stable() {
        assert_eq!(fingerprint(), 0xA0A3_6DC3);
    }

    /// F1 must actually construct exact ties, or the family measures nothing.
    #[test]
    fn f1_builds_exact_midpoints() {
        // `a + b` where b is half an ulp of a lands exactly on a midpoint, so
        // the tie-break rule decides the answer. Checked in integer terms: b's
        // exponent is exactly 24 below a's.
        let mut exact_ties = 0;
        for i in 0..count(Family::Rounding) {
            let v = vector(Family::Rounding, i);
            let (ea, eb) = ((v.a >> 23) & 0xFF, (v.b >> 23) & 0xFF);
            if v.b & 0x007F_FFFF == 0 && ea == eb + 24 {
                exact_ties += 1;
            }
        }
        assert!(
            exact_ties >= count(Family::Rounding) / 4,
            "too few exact ties: {exact_ties}"
        );
    }

    /// All four FCR modes must appear, or the ADR cannot say whether the field
    /// is honored.
    #[test]
    fn f1_replays_every_fcr_mode() {
        let mut seen = [false; 4];
        for i in 0..count(Family::Rounding) {
            seen[vector(Family::Rounding, i).fcr as usize] = true;
        }
        assert_eq!(seen, [true; 4]);
    }

    /// Every binary op × every NaN position. Coverage that is asserted is
    /// coverage; coverage that is described is a hope.
    #[test]
    fn f2_covers_every_op_in_every_nan_position() {
        let is_nan = |b: u32| b & 0x7F80_0000 == 0x7F80_0000 && b & 0x007F_FFFF != 0;
        for op in F2_OPS {
            let (mut a_only, mut b_only, mut both) = (false, false, false);
            for i in 0..F2_PRODUCT {
                let v = vector(Family::NanPayload, i);
                if v.op != op {
                    continue;
                }
                match (is_nan(v.a), is_nan(v.b)) {
                    (true, false) => a_only = true,
                    (false, true) => b_only = true,
                    (true, true) => both = true,
                    _ => {}
                }
            }
            assert!(a_only && b_only && both, "{op:?} misses a NaN position");
        }
    }

    #[test]
    fn f2_includes_hardware_generated_nans() {
        let generated = (F2_PRODUCT..F2_COUNT)
            .map(|i| vector(Family::NanPayload, i))
            .count();
        assert_eq!(generated, 10);
    }

    /// The two flush questions must stay separable: input flush and output flush
    /// are different behaviors, and a family that conflates them cannot say
    /// which one this silicon does.
    #[test]
    fn f3_separates_the_input_and_output_flush_questions() {
        let is_sub = |b: u32| b & 0x7F80_0000 == 0 && b & 0x007F_FFFF != 0;
        let (mut sub_in, mut sub_out_shape, mut both_sub) = (false, false, false);
        for i in 0..count(Family::Denormal) {
            let v = vector(Family::Denormal, i);
            match (is_sub(v.a), is_sub(v.b)) {
                (true, false) => sub_in = true,
                (true, true) => both_sub = true,
                // The normal-in / subnormal-out block: a small normal times a
                // small factor. Recognised by shape, since asserting the
                // *result* would need the arithmetic this crate must not do.
                (false, false) => {
                    if v.op == OpCode::MulS && (v.a >> 23) & 0xFF <= 4 {
                        sub_out_shape = true;
                    }
                }
                _ => {}
            }
        }
        assert!(sub_in, "no subnormal-in case");
        assert!(sub_out_shape, "no normal-in/subnormal-out case");
        assert!(both_sub, "no both-subnormal case");
    }

    #[test]
    fn f4_reaches_both_zeros_through_every_shape() {
        let (mut neg, mut abs, mut cmp, mut conv, mut arith) = (0, 0, 0, 0, 0);
        for i in 0..count(Family::SignedZero) {
            let v = vector(Family::SignedZero, i);
            assert!(
                v.a & 0x7FFF_FFFF == 0 || v.b & 0x7FFF_FFFF == 0,
                "every F4 vector must involve a zero"
            );
            match v.op {
                OpCode::NegS => neg += 1,
                OpCode::AbsS => abs += 1,
                OpCode::OeqS | OpCode::OltS | OpCode::OleS => cmp += 1,
                OpCode::TruncS | OpCode::RoundS | OpCode::FloorS | OpCode::CeilS => conv += 1,
                _ => arith += 1,
            }
        }
        assert!(neg >= 2 && abs >= 2 && cmp > 0 && conv >= 8 && arith > 0);
    }

    #[test]
    fn f5_sweeps_both_exponent_parities_for_rsqrt() {
        let mut parities = [false; 2];
        for i in 0..F5_EST {
            let v = vector(Family::DivSqrt, i);
            if v.op == OpCode::Rsqrt0S {
                parities[((v.a >> 23) & 1) as usize] = true;
            }
        }
        assert_eq!(parities, [true; 2], "rsqrt0.s keys on exponent parity");
    }

    #[test]
    fn f5_sequences_reach_the_special_values() {
        let mut saw = (false, false, false, false);
        for i in F5_EST..count(Family::DivSqrt) {
            let v = vector(Family::DivSqrt, i);
            if v.a == 0 && v.b == 0 {
                saw.0 = true; // 0/0
            }
            if v.a == 0x7F80_0000 && v.b == 0x7F80_0000 {
                saw.1 = true; // inf/inf
            }
            if v.op == OpCode::Div && v.a == v.b && v.a == 0x4049_0FDB {
                saw.2 = true; // x/x
            }
            if v.op == OpCode::Sqrt && v.a == 0xC049_0FDB {
                saw.3 = true; // sqrt of a negative
            }
        }
        assert_eq!(saw, (true, true, true, true));
    }

    #[test]
    fn f6_reaches_the_conversion_boundaries_and_sweeps_the_scale() {
        let mut imms = [false; 16];
        let (mut int_min, mut int_max, mut u32_max, mut needs_rounding) =
            (false, false, false, false);
        let (mut out_of_range, mut infinite, mut nan) = (false, false, false);
        for i in 0..count(Family::Convert) {
            let v = vector(Family::Convert, i);
            imms[v.imm as usize] = true;
            if matches!(v.op, OpCode::FloatS | OpCode::UfloatS) {
                int_min |= v.a == 0x8000_0000;
                int_max |= v.a == 0x7FFF_FFFF;
                u32_max |= v.a == 0xFFFF_FFFF;
                needs_rounding |= v.a == 0x0100_0001;
            } else {
                out_of_range |= v.a == 0x4F00_0000;
                infinite |= v.a == 0x7F80_0000;
                nan |= v.a == 0x7FC0_0000;
            }
        }
        assert!(int_min && int_max && u32_max && needs_rounding);
        assert!(out_of_range && infinite && nan);
        for imm in F6_IMMS {
            assert!(imms[imm as usize], "scale {imm} never generated");
        }
    }

    #[test]
    fn the_corpus_is_a_workable_size() {
        // Large enough to be a real sweep, small enough that the device runs it
        // in one serial session and the committed files stay reviewable.
        assert!((2_000..20_000).contains(&total()), "total = {}", total());
    }
}
