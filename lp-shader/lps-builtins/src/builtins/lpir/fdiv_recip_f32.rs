//! Native f32 reciprocal division — the f32 sibling of [`super::fdiv_recip_q32`].
//!
//! **This is deliberately not `a / b`.** It mirrors the Q32 reciprocal mode:
//! compute `1/b` once and multiply, which is the shape that pays off when one
//! divisor is reused across a vector.
//!
//! The consequence is a **second rounding**, so this builtin is *not* covered
//! by the `docs/design/float.md` §3 Guaranteed row for `/` — it is a builtin
//! with a tolerance (§6), and its tolerance is "within 2 ulp of `a / b`".
//! Callers that need the correctly-rounded quotient must use
//! [`super::fdiv_f32`].

/// f32 reciprocal division: `a * (1 / b)`.
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpir_fdiv_recip_f32(a: f32, b: f32) -> f32 {
    a * (1.0 / b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_for_power_of_two_divisors() {
        // 1/b is exact here, so the second rounding cannot bite.
        assert_eq!(__lp_lpir_fdiv_recip_f32(10.0, 4.0), 2.5);
        assert_eq!(__lp_lpir_fdiv_recip_f32(-3.0, 2.0), -1.5);
    }

    #[test]
    fn within_two_ulp_of_true_division() {
        for (a, b) in [(1.0f32, 3.0f32), (7.0, 9.0), (1e-5, 3.7), (123.456, 7.89)] {
            let got = __lp_lpir_fdiv_recip_f32(a, b);
            let want = a / b;
            let ulps = (got.to_bits() as i64 - want.to_bits() as i64).abs();
            assert!(ulps <= 2, "{a}/{b}: got {got}, want {want} ({ulps} ulp)");
        }
    }

    #[test]
    fn divide_by_zero_is_infinity() {
        assert_eq!(__lp_lpir_fdiv_recip_f32(1.0, 0.0), f32::INFINITY);
    }
}
