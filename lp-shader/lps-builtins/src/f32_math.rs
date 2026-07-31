//! `no_std` f32 primitives for the native-f32 builtin family.
//!
//! `core` has no `f32::floor`/`ceil`/`trunc`/`sqrt`/`abs` — those live in
//! `std`, which the device builds do not have. Every f32 builtin therefore
//! needs the same handful of shims, and they belong in exactly one place so a
//! decision like "`fract` is `x - floor(x)`, not the truncated remainder"
//! cannot drift between 80 call sites.
//!
//! Semantics follow `docs/design/float.md`: `floor`/`ceil`/`trunc` and `sqrt`
//! are §3 Guaranteed rows and must be the exact operations, not approximations.

/// `floor(x)` — largest integer ≤ `x`. Exact (§3).
#[inline(always)]
pub fn floor(x: f32) -> f32 {
    libm::floorf(x)
}

/// `ceil(x)` — smallest integer ≥ `x`. Exact (§3).
#[inline(always)]
pub fn ceil(x: f32) -> f32 {
    libm::ceilf(x)
}

/// `trunc(x)` — round toward zero. Exact (§3).
#[inline(always)]
pub fn trunc(x: f32) -> f32 {
    libm::truncf(x)
}

/// Round to nearest integer, ties to even — the `f32.nearest` / `fnearest`
/// rule. `round()` (ties away from zero) is a *different* operation and is
/// target-defined per float.md §4; this one is not, so it is spelled
/// explicitly everywhere it is meant.
#[inline(always)]
pub fn round_ties_even(x: f32) -> f32 {
    libm::rintf(x)
}

/// Correctly-rounded square root (§3 Guaranteed).
#[inline(always)]
pub fn sqrt(x: f32) -> f32 {
    libm::sqrtf(x)
}

/// `|x|` as a sign-bit clear: exact for NaN and ±0, which `x < 0.0` is not
/// (float.md §3 — negate and abs never normalize their operand).
#[inline(always)]
pub fn abs(x: f32) -> f32 {
    f32::from_bits(x.to_bits() & 0x7fff_ffff)
}

/// GLSL `fract(x) = x - floor(x)`; always in `[0, 1)` for finite `x`,
/// including negatives. Not the truncated remainder.
#[inline(always)]
pub fn fract(x: f32) -> f32 {
    x - floor(x)
}

/// GLSL `mod(x, y) = x - y * floor(x / y)`.
///
/// This is **not** Rust's `%` and not `fmodf`: both truncate, so they take the
/// sign of the dividend, while GLSL's takes the sign of the divisor.
#[inline(always)]
pub fn glsl_mod(x: f32, y: f32) -> f32 {
    x - y * floor(x / y)
}

/// GLSL `mix(a, b, t) = a + t * (b - a)`.
#[inline(always)]
pub fn mix(a: f32, b: f32, t: f32) -> f32 {
    a + t * (b - a)
}

/// GLSL `clamp(x, lo, hi)`.
#[inline(always)]
pub fn clamp(x: f32, lo: f32, hi: f32) -> f32 {
    if x < lo {
        lo
    } else if x > hi {
        hi
    } else {
        x
    }
}

/// GLSL `step(edge, x)`.
#[inline(always)]
pub fn step(edge: f32, x: f32) -> f32 {
    if x < edge { 0.0 } else { 1.0 }
}

/// GLSL `sign(x)`.
#[inline(always)]
pub fn sign(x: f32) -> f32 {
    if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fract_is_floor_based_not_truncated() {
        // The whole reason this shim exists: `-0.25 % 1.0` is `-0.25`.
        assert_eq!(fract(-0.25), 0.75);
        assert_eq!(fract(0.25), 0.25);
        assert_eq!(fract(3.0), 0.0);
    }

    #[test]
    fn glsl_mod_takes_the_sign_of_the_divisor() {
        assert_eq!(glsl_mod(-1.0, 3.0), 2.0);
        assert_eq!(glsl_mod(1.0, -3.0), -2.0);
        assert_eq!(glsl_mod(7.5, 2.0), 1.5);
    }

    #[test]
    fn abs_clears_the_sign_bit_without_normalizing() {
        assert!(abs(f32::NAN).is_nan());
        assert_eq!(abs(-0.0f32).to_bits(), 0.0f32.to_bits());
        assert_eq!(abs(-3.5), 3.5);
    }

    #[test]
    fn round_ties_even_matches_wasm_f32_nearest() {
        assert_eq!(round_ties_even(0.5), 0.0);
        assert_eq!(round_ties_even(1.5), 2.0);
        assert_eq!(round_ties_even(2.5), 2.0);
        assert_eq!(round_ties_even(-0.5), -0.0);
        assert_eq!(round_ties_even(-1.5), -2.0);
    }
}
