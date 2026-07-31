//! Native f32 round-to-nearest — the f32 sibling of [`super::fnearest_q32`].
//!
//! **Ties go to even**, matching wasm's `f32.nearest` and the Q32 sibling's
//! documented rule. This is a different operation from GLSL `round()`, whose
//! tie behavior `docs/design/float.md` §4 marks *target-defined*; `fnearest` is
//! the one with a fixed rule, so it is the one shared lowerings use.

/// f32 round to nearest integer, ties to even.
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpir_fnearest_f32(x: f32) -> f32 {
    crate::f32_math::round_ties_even(x)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ties_go_to_even_matching_the_q32_sibling() {
        assert_eq!(__lp_lpir_fnearest_f32(0.5), 0.0);
        assert_eq!(__lp_lpir_fnearest_f32(1.5), 2.0);
        assert_eq!(__lp_lpir_fnearest_f32(2.5), 2.0);
        assert_eq!(__lp_lpir_fnearest_f32(3.5), 4.0);
        assert_eq!(__lp_lpir_fnearest_f32(-1.5), -2.0);
        assert_eq!(__lp_lpir_fnearest_f32(-2.5), -2.0);
    }

    #[test]
    fn non_ties_round_normally() {
        assert_eq!(__lp_lpir_fnearest_f32(1.4), 1.0);
        assert_eq!(__lp_lpir_fnearest_f32(1.6), 2.0);
        assert_eq!(__lp_lpir_fnearest_f32(-1.6), -2.0);
    }
}
