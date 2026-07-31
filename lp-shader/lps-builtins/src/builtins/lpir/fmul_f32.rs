//! Native f32 multiplication — the f32 sibling of [`super::fmul_q32`].
//!
//! `*` is a `docs/design/float.md` §3 Guaranteed row (correctly rounded, RNE).
//! Unlike Q32 there is no scale correction: the whole i64-staging dance the
//! fixed-point sibling performs exists only to undo the Q16.16 scale factor.

/// f32 multiplication: `a * b`.
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpir_fmul_f32(a: f32, b: f32) -> f32 {
    a * b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multiplies() {
        assert_eq!(__lp_lpir_fmul_f32(2.5, 4.0), 10.0);
    }

    #[test]
    fn inf_times_zero_is_nan() {
        assert!(__lp_lpir_fmul_f32(f32::INFINITY, 0.0).is_nan());
    }

    #[test]
    fn signed_zero_is_preserved() {
        assert_eq!(__lp_lpir_fmul_f32(-1.0, 0.0).to_bits(), (-0.0f32).to_bits());
    }
}
