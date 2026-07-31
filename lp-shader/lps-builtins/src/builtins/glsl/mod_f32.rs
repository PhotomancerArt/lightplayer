//! Native f32 `mod` — the f32 sibling of [`super::mod_q32`].
//!
//! GLSL `mod(x, y) = x - y * floor(x / y)`. This is **not** Rust's `%` and not
//! `libm::fmodf`: both compute the truncated remainder, which takes the sign of
//! the *dividend*, while GLSL's takes the sign of the *divisor*.
//! `mod(-1.0, 3.0)` is `2.0` in GLSL and `-1.0` under `%`. Reaching for `%`
//! here is the single most likely way to get this builtin wrong, which is why
//! the shared helper spells the formula out once.
//!
//! **Tolerance (`docs/design/float.md` §6):** `1e-6` relative with a `1e-6`
//! absolute floor. The subtraction can cancel badly when `x / y` is large, and
//! that is inherent to the canonical formula — not something to "fix" with a
//! different algorithm, since the canonical GLSL is normative.
//!
//! **Domain:** `y = 0` is GLSL-undefined (float.md §5); it yields NaN here and
//! never traps.

/// GLSL `mod(x, y)`.
#[unsafe(no_mangle)]
pub extern "C" fn __lps_mod_f32(x: f32, y: f32) -> f32 {
    crate::f32_math::glsl_mod(x, y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_the_sign_of_the_divisor_not_the_dividend() {
        assert_eq!(__lps_mod_f32(-1.0, 3.0), 2.0);
        assert_eq!(__lps_mod_f32(1.0, -3.0), -2.0);
        // Rust's `%` would answer -1.0 and 1.0 respectively.
        assert_ne!(__lps_mod_f32(-1.0, 3.0), -1.0 % 3.0);
    }

    #[test]
    fn ordinary_cases() {
        assert_eq!(__lps_mod_f32(7.5, 2.0), 1.5);
        assert_eq!(__lps_mod_f32(5.0, 5.0), 0.0);
        assert_eq!(__lps_mod_f32(0.5, 1.0), 0.5);
    }

    #[test]
    fn result_stays_inside_the_divisor_interval() {
        for ix in -40..=40 {
            for y in [0.5f32, 1.0, 3.0, 7.25] {
                let x = ix as f32 * 0.37;
                let m = __lps_mod_f32(x, y);
                assert!(m >= 0.0 && m < y, "mod({x},{y}) = {m}");
            }
        }
    }

    #[test]
    fn zero_divisor_does_not_trap() {
        // float.md §5: value Unspecified, so only "it returned" is asserted.
        let _ = __lps_mod_f32(1.0, 0.0);
    }
}
