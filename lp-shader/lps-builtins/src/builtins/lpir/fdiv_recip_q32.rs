//! Fixed-point 16.16 division via reciprocal multiplication.
//!
//! Faster than [`__lp_lpir_fdiv_q32`] (one i32 udiv + 2 muls + shift +
//! sign fixup vs one i64 div), at the cost of small precision loss:
//! ~0.01% typical error, up to ~2-3% at edges (saturated dividends, very
//! small divisors).
//!
//! Selected when the shader opts into `Q32Options { div: Reciprocal, .. }`.
//! See `docs/plans-old/2026-04-18-q32-options-dispatch/00-design.md`.
//!
//! ## Algorithm
//!
//! Ported from `lp-glsl/.../div_recip.rs` (deleted in commit `1daa516`).
//! The `divisor == 0` guard is new — original would panic on integer
//! divide; we saturate instead, matching `__lp_lpir_fdiv_q32`.
//!
//! ```text
//! recip = 0x8000_0000 / |divisor|              (one i32 udiv, truncates)
//! quot  = (|dividend| * recip * 2) >> 16       (u64 multiply intermediate)
//! quot *= sign(dividend) ^ sign(divisor)
//! ```
//!
//! For `divisor == 0`: returns `0` for `0/0`, `MAX_FIXED` for positive/0,
//! `MIN_FIXED` for negative/0.

const MAX_FIXED: i32 = 0x7FFF_FFFF;
const MIN_FIXED: i32 = i32::MIN;

/// Q16.16 division by reciprocal multiplication.
///
/// See module docs for algorithm and precision notes.
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpir_fdiv_recip_q32(dividend: i32, divisor: i32) -> i32 {
    if divisor == 0 {
        if dividend == 0 {
            return 0;
        } else if dividend > 0 {
            return MAX_FIXED;
        } else {
            return MIN_FIXED;
        }
    }

    let result_sign = if (dividend ^ divisor) < 0 {
        -1i32
    } else {
        1i32
    };

    let abs_dividend = dividend.unsigned_abs();
    let abs_divisor = divisor.unsigned_abs();

    let recip = 0x8000_0000u32 / abs_divisor;
    let quot = (((abs_dividend as u64) * (recip as u64) * 2u64) >> 16) as u32;

    (quot as i32).wrapping_mul(result_sign)
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use crate::util::test_helpers::{fixed_to_float, float_to_fixed};

    /// Tolerance used for "approximately equal" comparisons in tests.
    /// Reciprocal mul has documented ~0.01% typical error; we use a slightly
    /// looser bound to keep tests stable across platforms.
    const TOL: f32 = 0.001;

    fn check(dividend: f32, divisor: f32, expected: f32) {
        let d = float_to_fixed(dividend);
        let s = float_to_fixed(divisor);
        let r = fixed_to_float(__lp_lpir_fdiv_recip_q32(d, s));
        assert!(
            (r - expected).abs() < TOL,
            "fdiv_recip_q32({dividend}, {divisor}) = {r}, expected {expected}"
        );
    }

    #[test]
    fn basic_unsigned() {
        check(10.0, 2.0, 5.0);
        check(15.0, 3.0, 5.0);
        check(20.0, 2.0, 10.0);
        check(7.5, 1.0, 7.5);
        check(0.999, 0.998, 0.999 / 0.998);
    }

    #[test]
    fn basic_signed() {
        check(10.0, -2.0, -5.0);
        check(-10.0, 2.0, -5.0);
        check(-10.0, -2.0, 5.0);
        check(-7.5, 3.0, -2.5);
    }

    #[test]
    fn small_divisors() {
        check(1.0, 0.5, 2.0);
        check(0.25, 0.5, 0.5);
    }

    #[test]
    fn divide_by_zero_saturates() {
        // Match __lp_lpir_fdiv_q32 policy.
        assert_eq!(__lp_lpir_fdiv_recip_q32(0, 0), 0);
        assert_eq!(__lp_lpir_fdiv_recip_q32(float_to_fixed(1.0), 0), MAX_FIXED);
        assert_eq!(__lp_lpir_fdiv_recip_q32(float_to_fixed(-1.0), 0), MIN_FIXED);
    }

    #[test]
    fn matches_saturating_helper_within_tolerance() {
        // For "normal" cases (non-edge), the reciprocal helper should be
        // within ~0.01% of the saturating helper.
        let cases: &[(f32, f32)] = &[
            (10.0, 3.0),
            (1.5, 0.25),
            (-7.0, 2.5),
            (100.0, 7.0),
            (0.5, 0.125),
        ];
        for &(a, b) in cases {
            let af = float_to_fixed(a);
            let bf = float_to_fixed(b);
            let sat = fixed_to_float(crate::builtins::lpir::fdiv_q32::__lp_lpir_fdiv_q32(af, bf));
            let recip = fixed_to_float(__lp_lpir_fdiv_recip_q32(af, bf));
            // Within 0.1% relative or 0.001 absolute, whichever is larger.
            let tol = (sat.abs() * 0.001).max(0.001);
            assert!(
                (sat - recip).abs() < tol,
                "fdiv divergence at {a}/{b}: sat={sat}, recip={recip}"
            );
        }
    }
}
