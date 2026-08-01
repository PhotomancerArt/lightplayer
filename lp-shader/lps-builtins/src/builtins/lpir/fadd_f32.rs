//! Native f32 addition.
//!
//! Reference implementation, the f32 sibling of [`super::fadd_q32`]. The
//! shipped lowering inlines a native `f32.add`; this exists as the semantic
//! reference and for callers that reach the builtin through an import.
//!
//! `+` is a `docs/design/float.md` §3 **Guaranteed** row: correctly rounded
//! (round-to-nearest-even), NaN propagates, `inf - inf` is NaN, signed zero is
//! preserved. So the body is the operator — there is nothing to approximate,
//! and saturation (the Q32 sibling's whole job) would be a spec violation.

/// f32 addition: `a + b`.
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpir_fadd_f32(a: f32, b: f32) -> f32 {
    a + b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds() {
        assert_eq!(__lp_lpir_fadd_f32(1.5, 2.5), 4.0);
    }

    #[test]
    fn does_not_saturate_where_q32_would() {
        // The point of f32: 1e30 + 1e30 is a number, not a clamp.
        assert_eq!(__lp_lpir_fadd_f32(1e30, 1e30), 2e30);
    }

    #[test]
    fn nan_propagates() {
        assert!(__lp_lpir_fadd_f32(f32::NAN, 1.0).is_nan());
        assert!(__lp_lpir_fadd_f32(f32::INFINITY, f32::NEG_INFINITY).is_nan());
    }
}
