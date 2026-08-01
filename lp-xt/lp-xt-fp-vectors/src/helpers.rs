//! Probe grids for the divide/sqrt helper family and `const.s` (M6 P6).
//!
//! # Why this is not a seventh corpus family
//!
//! The six families in [`crate`] carry **predictions committed before
//! hardware** (M6 D2). The helper instructions cannot be predicted at all: the
//! ISA Reference Manual's Table 4-46 does not list them, and the license rules
//! keep binutils/GCC/QEMU source off the table. There is nothing to predict
//! *from* — so these grids exist to **characterize**, not to conform. The
//! campaign runs them on silicon first, derives candidate semantics from the
//! answers, implements those semantics in `lp-xt-emu`, and only then replays
//! this same grid against the committed capture as a regression.
//!
//! The direction of travel is therefore inverted relative to the families, and
//! that is stated here so nobody generalizes it: **for the helpers, silicon is
//! the only source there is.** The safeguard against a harness bug masquerading
//! as semantics is the second, independent oracle — the toolchain's complete
//! divide and square-root sequences run end-to-end over F5's operand sweep, and
//! the derived semantics must reproduce those 272 results exactly too.
//!
//! # Same code on both sides
//!
//! Exactly like the families: this module compiles into the device harness and
//! into host tests, both sides regenerate the grid, and [`fingerprint`] makes
//! agreement checkable rather than assumed.

use crate::mix;

/// One helper probe: raw bit patterns, never values.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct HelperVector {
    pub op: HelperOp,
    pub index: u32,
    /// The destination register's **initial** bits. The RR-shaped helpers may
    /// read `fr` as well as write it (`mkdadj.s` demonstrably combines both
    /// operands), so every probe stages it explicitly.
    pub r: u32,
    /// The `fs` operand's bits.
    pub s: u32,
    /// The `ft` operand's bits (ternary ops only; zero otherwise).
    pub t: u32,
}

/// The helper instructions under characterization.
///
/// `madd.s` is deliberately on the list even though it is *not* a helper: it
/// runs over the identical ternary grid so `maddn.s`'s deviations from it are
/// directly visible in one capture, rather than inferred across two runs.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum HelperOp {
    Nexp01S = 1,
    MksadjS = 2,
    MkdadjS = 3,
    AddexpS = 4,
    AddexpmS = 5,
    MaddnS = 6,
    DivnS = 7,
    /// The reference row: architecturally understood, measured for contrast.
    MaddS = 8,
}

impl HelperOp {
    pub const ALL: [HelperOp; 8] = [
        HelperOp::Nexp01S,
        HelperOp::MksadjS,
        HelperOp::MkdadjS,
        HelperOp::AddexpS,
        HelperOp::AddexpmS,
        HelperOp::MaddnS,
        HelperOp::DivnS,
        HelperOp::MaddS,
    ];

    /// The capture key (`D <name> …` lines). Prefixed so a helper grid can
    /// never be mistaken for a conformance family.
    pub const fn name(self) -> &'static str {
        match self {
            HelperOp::Nexp01S => "h_nexp01",
            HelperOp::MksadjS => "h_mksadj",
            HelperOp::MkdadjS => "h_mkdadj",
            HelperOp::AddexpS => "h_addexp",
            HelperOp::AddexpmS => "h_addexpm",
            HelperOp::MaddnS => "h_maddn",
            HelperOp::DivnS => "h_divn",
            HelperOp::MaddS => "h_madd",
        }
    }

    pub fn from_name(s: &str) -> Option<HelperOp> {
        HelperOp::ALL.into_iter().find(|o| o.name() == s)
    }

    /// Whether the instruction takes a third (`ft`) operand.
    pub const fn is_ternary(self) -> bool {
        matches!(self, HelperOp::MaddnS | HelperOp::DivnS | HelperOp::MaddS)
    }
}

/// The unary probes' `fs` sweep: an exponent ladder, significand patterns that
/// make bit movement visible, and every special class.
pub const UNARY_VALUES: [u32; 24] = [
    0x0000_0000, // +0
    0x8000_0000, // -0
    0x3F80_0000, // 1.0
    0xBF80_0000, // -1.0
    0x4000_0000, // 2.0
    0x3F00_0000, // 0.5
    0x4049_0FDB, // pi
    0xC049_0FDB, // -pi
    0x3EAA_AAAB, // ~1/3 — a busy significand
    0x3F80_0001, // 1 + 2^-23
    0x3FFF_FFFF, // just under 2
    0x0080_0000, // min normal
    0x0080_0001,
    0x7F7F_FFFF, // max normal
    0x7F00_0000, // 2^127
    0x5F00_0000, // 2^63
    0x1F80_0000, // 2^-64
    0x0000_0001, // min subnormal
    0x007F_FFFF, // max subnormal
    0x7F80_0000, // +inf
    0xFF80_0000, // -inf
    0x7FC0_0000, // canonical qNaN
    0x7FD5_AA55, // qNaN, distinctive payload
    0x7F80_0001, // sNaN
];

/// Initial `fr` bits for the RR-shaped probes: distinctive patterns, so whether
/// (and how) the destination's old value participates is readable from the
/// answer instead of assumed away.
pub const UNARY_R_INITS: [u32; 6] = [
    0x0000_0000, // +0
    0x3F80_0000, // 1.0
    0x4040_0000, // 3.0
    0xBF00_0000, // -0.5
    0x7FC0_0000, // qNaN
    0x1234_5678, // arbitrary, low-exponent, busy bits
];

/// Ternary accumulator (`fr`) initial values.
pub const TERNARY_ACCS: [u32; 6] = [
    0x0000_0000, // +0
    0x3F80_0000, // 1.0
    0xBF80_0000, // -1.0
    0x3F80_0001, // 1 + 2^-23
    0x3380_0000, // 2^-24 — where a second rounding shows
    0x7FC0_0000, // qNaN
];

/// Ternary `fs`/`ft` sweep. Includes the fused-vs-unfused detector pair
/// (`1 + 2^-12` squared against `-(1 + 2^-11)`) so `maddn.s`'s rounding
/// behavior is separable from `madd.s`'s in the same capture.
pub const TERNARY_VALUES: [u32; 16] = [
    0x3F80_0000, // 1.0
    0x3F80_0001, // 1 + 2^-23
    0x3F80_0800, // 1 + 2^-12
    0xBF80_1000, // -(1 + 2^-11)
    0x4000_0000, // 2.0
    0xC000_0000, // -2.0
    0x3F00_0000, // 0.5
    0x4049_0FDB, // pi
    0x3EAA_AAAB, // ~1/3
    0x7F7F_FFFF, // max normal
    0x0080_0000, // min normal
    0x0000_0001, // min subnormal
    0x8000_0000, // -0
    0x7F80_0000, // +inf
    0xFF80_0000, // -inf
    0x7FC0_0000, // qNaN
];

const UNARY_COUNT: u32 = (UNARY_R_INITS.len() * UNARY_VALUES.len()) as u32;
const TERNARY_COUNT: u32 =
    (TERNARY_ACCS.len() * TERNARY_VALUES.len() * TERNARY_VALUES.len()) as u32;

/// How many probes an op's grid has.
pub const fn count(op: HelperOp) -> u32 {
    if op.is_ternary() {
        TERNARY_COUNT
    } else {
        UNARY_COUNT
    }
}

/// Every probe across every op.
pub fn total() -> u32 {
    HelperOp::ALL.iter().map(|o| count(*o)).sum()
}

/// The probe at `index` within `op`'s grid — pure, index-addressable, exactly
/// like [`crate::vector`].
///
/// # Panics
/// If `index >= count(op)`.
pub fn probe(op: HelperOp, index: u32) -> HelperVector {
    assert!(index < count(op), "helper probe index out of range");
    if op.is_ternary() {
        let t = TERNARY_VALUES[(index as usize) % TERNARY_VALUES.len()];
        let i = index as usize / TERNARY_VALUES.len();
        let s = TERNARY_VALUES[i % TERNARY_VALUES.len()];
        let r = TERNARY_ACCS[i / TERNARY_VALUES.len()];
        HelperVector { op, index, r, s, t }
    } else {
        let s = UNARY_VALUES[(index as usize) % UNARY_VALUES.len()];
        let r = UNARY_R_INITS[index as usize / UNARY_VALUES.len()];
        HelperVector {
            op,
            index,
            r,
            s,
            t: 0,
        }
    }
}

/// A hash over the whole helper grid, printed by both sides — the same
/// contract as [`crate::fingerprint`], for the same reason.
pub fn fingerprint() -> u32 {
    let mut h: u32 = 0x811C_9DC5;
    for op in HelperOp::ALL {
        h = mix(h ^ op as u32);
        for i in 0..count(op) {
            let v = probe(op, i);
            for word in [v.r, v.s, v.t] {
                h = mix(h ^ word).wrapping_add(word);
            }
        }
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grids_are_pure_and_sized_as_documented() {
        assert_eq!(count(HelperOp::Nexp01S), 144);
        assert_eq!(count(HelperOp::MaddnS), 1536);
        assert_eq!(total(), 5 * 144 + 3 * 1536);
        for op in HelperOp::ALL {
            for i in [0, count(op) / 2, count(op) - 1] {
                assert_eq!(probe(op, i), probe(op, i), "{op:?}[{i}] is not pure");
            }
        }
    }

    /// The ternary grid must contain a rounding-sensitive product — the case
    /// where one rounding and two roundings give different bits — for both
    /// `maddn.s` and its `madd.s` reference, or the capture cannot separate
    /// them. `(1 + 2^-12)² = 1 + 2^-11 + 2^-24` is a tie one bit past f32's
    /// significand, and with the `2^-24` accumulator staged the fused and
    /// unfused answers differ in the last place.
    #[test]
    fn ternary_grid_contains_the_rounding_detector() {
        let mut pair_with_tiny_acc = false;
        for i in 0..count(HelperOp::MaddnS) {
            let v = probe(HelperOp::MaddnS, i);
            if v.s == 0x3F80_0800 && v.t == 0x3F80_0800 && v.r == 0x3380_0000 {
                pair_with_tiny_acc = true;
            }
        }
        assert!(
            pair_with_tiny_acc,
            "no s = t = 1 + 2^-12 probe with the 2^-24 accumulator"
        );
    }

    /// Drift is loud, exactly like the corpus fingerprint: if this number
    /// changes after captures are committed, every committed helper capture is
    /// for a different grid and the replay tests must refuse it.
    #[test]
    fn the_helper_fingerprint_is_stable() {
        assert_eq!(fingerprint(), 0x9715_768F);
    }
}
