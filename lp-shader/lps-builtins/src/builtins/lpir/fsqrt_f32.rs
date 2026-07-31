//! Native f32 square root — the f32 sibling of [`super::fsqrt_q32`].
//!
//! `sqrt` is a `docs/design/float.md` §3 **Guaranteed** row: correctly rounded
//! (RNE) on every target. So this is the hardware/`libm` square root, not an
//! approximation — unlike [`super::super::glsl::inversesqrt_f32`], which is a
//! §6 builtin and may approximate.
//!
//! `sqrt` of a negative is NaN (and never traps), matching IEEE.
//!
//! This is the one lpir builtin the corpus actually reaches as an *import*:
//! `LpirOp::Fsqrt` lowers to a native `f32.sqrt` on wasm, but shaders that call
//! `@lpir::sqrt` directly (`builtins/exp-sqrt.glsl`,
//! `control/torture/intrin_sqrt.glsl`) go through this symbol.

/// f32 square root.
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpir_fsqrt_f32(x: f32) -> f32 {
    crate::f32_math::sqrt(x)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_on_perfect_squares() {
        assert_eq!(__lp_lpir_fsqrt_f32(4.0), 2.0);
        assert_eq!(__lp_lpir_fsqrt_f32(0.25), 0.5);
        assert_eq!(__lp_lpir_fsqrt_f32(0.0), 0.0);
    }

    #[test]
    fn correctly_rounded_on_non_squares() {
        // Guaranteed row: bit-exact, not "close".
        assert_eq!(__lp_lpir_fsqrt_f32(2.0).to_bits(), 1.4142135f32.to_bits());
    }

    #[test]
    fn negative_is_nan_and_does_not_trap() {
        assert!(__lp_lpir_fsqrt_f32(-1.0).is_nan());
    }
}
