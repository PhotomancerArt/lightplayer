//! Native f32 `round` — the f32 sibling of [`super::round_q32`].
//!
//! **Ties round away from zero**, not to even. `docs/design/float.md` §4 lists
//! `round()` tie behavior as *target-defined*, so both are legal — but this is
//! not a free choice in practice:
//!
//! - the Q32 sibling rounds ties away from zero, and the same shader should
//!   not change answer between Fixed and Float mode where it does not have to;
//! - `interp.f32`, the conformance oracle, is `libm::roundf`, which rounds ties
//!   away;
//! - the corpus (`builtins/common-round.glsl`) asserts `round(2.5) == 3.0` and
//!   `round(-2.5) == -3.0` on both, with a comment saying so.
//!
//! `f32.nearest` (ties to even) is a *different* operation and lives in
//! [`crate::builtins::lpir::fnearest_f32`]. Do not conflate them: WGSL's
//! `round()` is ties-to-even, which is why the GPU tier carries
//! `@unsupported(wgpu.f32)` on the tie cases.
//!
//! **Tolerance:** exact. Every result is representable.

/// GLSL `round(x)` — nearest integer, ties away from zero.
#[unsafe(no_mangle)]
pub extern "C" fn __lps_round_f32(x: f32) -> f32 {
    libm::roundf(x)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ties_round_away_from_zero_matching_q32_and_the_corpus() {
        assert_eq!(__lps_round_f32(2.5), 3.0);
        assert_eq!(__lps_round_f32(-2.5), -3.0);
        assert_eq!(__lps_round_f32(0.5), 1.0);
        assert_eq!(__lps_round_f32(-0.5), -1.0);
        assert_eq!(__lps_round_f32(1.5), 2.0);
    }

    #[test]
    fn differs_from_fnearest_on_exactly_the_ties() {
        use crate::builtins::lpir::fnearest_f32::__lp_lpir_fnearest_f32;
        // Non-ties agree; ties are where the two operations part company.
        for x in [1.4f32, 1.6, -1.4, -1.6, 3.0] {
            assert_eq!(__lps_round_f32(x), __lp_lpir_fnearest_f32(x), "{x}");
        }
        assert_ne!(__lps_round_f32(2.5), __lp_lpir_fnearest_f32(2.5));
    }

    #[test]
    fn non_ties_round_normally() {
        assert_eq!(__lps_round_f32(5.0), 5.0);
        assert_eq!(__lps_round_f32(3.7), 4.0);
        assert_eq!(__lps_round_f32(3.2), 3.0);
        assert_eq!(__lps_round_f32(-1.7), -2.0);
        assert_eq!(__lps_round_f32(-2.6), -3.0);
    }
}
