//! Native f32 division — the f32 sibling of [`super::fdiv_q32`].
//!
//! `/` is a `docs/design/float.md` §3 Guaranteed row: correctly rounded (RNE),
//! `x/0 = ±inf` for finite non-zero `x`, `0/0 = NaN`. Nothing traps — the Q32
//! sibling's divide-by-zero guard has no f32 analogue and adding one would
//! contradict the spec.

/// f32 division: `a / b`.
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpir_fdiv_f32(a: f32, b: f32) -> f32 {
    a / b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn divides() {
        assert_eq!(__lp_lpir_fdiv_f32(10.0, 4.0), 2.5);
    }

    #[test]
    fn divide_by_zero_is_infinity_not_a_trap() {
        assert_eq!(__lp_lpir_fdiv_f32(1.0, 0.0), f32::INFINITY);
        assert_eq!(__lp_lpir_fdiv_f32(-1.0, 0.0), f32::NEG_INFINITY);
        assert!(__lp_lpir_fdiv_f32(0.0, 0.0).is_nan());
    }
}
