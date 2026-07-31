//! Native f32 `fabs` / `fmin` / `fmax` / `ffloor` / `fceil` / `ftrunc` — the
//! f32 sibling of [`super::float_misc_q32`].
//!
//! **Reference implementations.** As in Q32, the primary lowering inlines
//! `fabs`, `fmin`, and `fmax`; these remain the authoritative semantics and the
//! fallback for callers arriving through `sym_call`.
//!
//! Per `docs/design/float.md`:
//!
//! - `abs` is a §3 Guaranteed sign-bit operation — it must not normalize NaN or
//!   ±0, which is why it is a bit mask and not `if x < 0.0 { -x }`.
//! - `floor`/`ceil`/`trunc` are §3 Guaranteed and exact.
//! - **`min`/`max` with a NaN operand is §5 Unspecified.** IEEE-754 defines two
//!   competing operations and GLSL declares the case undefined, so each target
//!   uses its native instruction. Rust's `f32::min`/`max` return the non-NaN
//!   operand (IEEE-2008 `minNum`/`maxNum`); wasm propagates the NaN. Both are
//!   legal. Nothing may assert this, here or in the corpus.

use crate::f32_math;

/// `|x|` — sign-bit clear (exact for NaN and ±0).
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpir_fabs_f32(v: f32) -> f32 {
    f32_math::abs(v)
}

/// `min(a, b)`. NaN behavior is float.md §5 Unspecified — target-native.
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpir_fmin_f32(a: f32, b: f32) -> f32 {
    a.min(b)
}

/// `max(a, b)`. NaN behavior is float.md §5 Unspecified — target-native.
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpir_fmax_f32(a: f32, b: f32) -> f32 {
    a.max(b)
}

/// `floor(x)` — toward negative infinity.
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpir_ffloor_f32(v: f32) -> f32 {
    f32_math::floor(v)
}

/// `ceil(x)` — toward positive infinity.
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpir_fceil_f32(v: f32) -> f32 {
    f32_math::ceil(v)
}

/// `trunc(x)` — toward zero.
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpir_ftrunc_f32(v: f32) -> f32 {
    f32_math::trunc(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abs_does_not_normalize_nan_or_negative_zero() {
        assert!(__lp_lpir_fabs_f32(f32::NAN).is_nan());
        assert_eq!(__lp_lpir_fabs_f32(-0.0).to_bits(), 0.0f32.to_bits());
        assert_eq!(__lp_lpir_fabs_f32(-7.25), 7.25);
    }

    #[test]
    fn rounding_directions_differ_on_negatives() {
        // The three are the same for positives and diverge below zero; that
        // divergence is the only thing worth pinning.
        assert_eq!(__lp_lpir_ffloor_f32(-1.5), -2.0);
        assert_eq!(__lp_lpir_fceil_f32(-1.5), -1.0);
        assert_eq!(__lp_lpir_ftrunc_f32(-1.5), -1.0);
        assert_eq!(__lp_lpir_ffloor_f32(1.5), 1.0);
        assert_eq!(__lp_lpir_fceil_f32(1.5), 2.0);
        assert_eq!(__lp_lpir_ftrunc_f32(1.5), 1.0);
    }

    #[test]
    fn min_max_without_nan_are_ordinary() {
        // float.md §5: the NaN cases are Unspecified and are deliberately
        // NOT tested here.
        assert_eq!(__lp_lpir_fmin_f32(1.0, 2.0), 1.0);
        assert_eq!(__lp_lpir_fmax_f32(1.0, 2.0), 2.0);
        assert_eq!(__lp_lpir_fmin_f32(-1.0, -2.0), -2.0);
    }
}
