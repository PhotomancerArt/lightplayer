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
    x - y * floor(div(x, y))
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

/// `a / b`, correctly rounded, through the FPU on Xtensa.
///
/// `a / b` in Rust compiles, on Xtensa, to `compiler_builtins`' software
/// `__divsf3`. This runs the FPU's own divide sequence there instead (see
/// `builtins/lpir/fdiv_f32.rs` for where it comes from and how it was
/// checked), and is plain `a / b` everywhere else.
#[inline(always)]
pub fn div(a: f32, b: f32) -> f32 {
    #[cfg(target_arch = "xtensa")]
    {
        // SAFETY: a leaf kernel that reads its two arguments and writes only
        // FP registers, none of which the windowed ABI preserves across a call.
        f32::from_bits(unsafe { lps_fdiv_seq_f32(a.to_bits(), b.to_bits()) })
    }
    #[cfg(not(target_arch = "xtensa"))]
    {
        a / b
    }
}

#[cfg(target_arch = "xtensa")]
core::arch::global_asm!(
    ".section .text.lps_fdiv_seq_f32,\"ax\",@progbits",
    ".align 4",
    ".global lps_fdiv_seq_f32",
    ".type lps_fdiv_seq_f32,@function",
    "lps_fdiv_seq_f32:",
    "  entry a1, 32",
    "  wfr f1, a2",
    "  wfr f2, a3",
    "  div0.s f3, f2",
    "  nexp01.s f4, f2",
    "  const.s f5, 1",
    "  maddn.s f5, f4, f3",
    "  mov.s f6, f3",
    "  mov.s f7, f2",
    "  nexp01.s f2, f1",
    "  maddn.s f6, f5, f6",
    "  const.s f5, 1",
    "  const.s f0, 0",
    "  neg.s f8, f2",
    "  maddn.s f5, f4, f6",
    "  maddn.s f0, f8, f3",
    "  mkdadj.s f7, f1",
    "  maddn.s f6, f5, f6",
    "  maddn.s f8, f4, f0",
    "  const.s f3, 1",
    "  maddn.s f3, f4, f6",
    "  maddn.s f0, f8, f6",
    "  neg.s f2, f2",
    "  maddn.s f6, f3, f6",
    "  maddn.s f2, f4, f0",
    "  addexpm.s f0, f7",
    "  addexp.s f6, f7",
    "  divn.s f0, f2, f6",
    "  rfr a2, f0",
    "  retw",
    ".size lps_fdiv_seq_f32, .-lps_fdiv_seq_f32",
);

#[cfg(target_arch = "xtensa")]
unsafe extern "C" {
    fn lps_fdiv_seq_f32(a: u32, b: u32) -> u32;
}

/// `(sin(x), cos(x))` in single precision: one range reduction, two short
/// polynomials, nothing wider than `f32`.
///
/// This is the f32 family's sine and cosine. `libm::sinf`/`cosf` compute their
/// kernels in `f64`, which on the ESP32-S3 is software emulation: the fps
/// spike (2026-10-23) found more than half of an f32 shader's sampling time in
/// `__muldf3`/`__subdf3`, and Float mode 4.9× slower than Q32 because of it.
/// Here every operation is one the LX7's single-precision FPU executes.
///
/// - **Reduction:** `k` = `x · 2/π` rounded to nearest; `r = x − k·π/2` with
///   π/2 split into three `f32` parts (Cody–Waite). The first two carry 8
///   significant bits each, so `k·A` and `k·B` are exact for `|k| < 2¹⁶`, and
///   the third is π/2's next 24 bits (the split's residual is 5·10⁻¹⁴).
/// - **Polynomials:** Taylor series of `sin` to `r⁹` and of `cos` to `r¹⁰` on
///   `|r| ≤ π/4`; truncation is under 2·10⁻⁹, far inside an `f32` ulp. The
///   coefficients are `1/n!`: arithmetic, not a fitted table.
/// - **Large or non-finite `x`** (`|x| > 100 000`, NaN, ±∞): `libm`, which
///   reduces exactly. Slow, correct, and rare in a shader.
///
/// Within the family's declared band (`docs/design/float.md` §6: `1e-6`
/// relative, `1e-6` absolute floor) over the fast range; the tests sweep it.
pub fn sincos(x: f32) -> (f32, f32) {
    if !(abs(x) <= SINCOS_FAST_MAX) {
        return (libm::sinf(x), libm::cosf(x));
    }
    // sin(x) = x to f32 precision here (the next term is x³/6, under 2·10⁻⁹
    // relative), and returning `x` keeps `sin(-0) = -0`, which the polynomial
    // would round to +0.
    if abs(x) < 1.0e-4 {
        return (x, 1.0 - 0.5 * x * x);
    }
    // Round to nearest by the 1.5·2²³ trick: exact for |x·2/π| < 2²², which
    // the bound above keeps (|k| ≤ 63 662).
    let kf = (x * FRAC_2_PI + ROUND_MAGIC) - ROUND_MAGIC;
    let r = ((x - kf * PIO2_A) - kf * PIO2_B) - kf * PIO2_C;
    let z = r * r;
    let s =
        r + r * z * (-1.0 / 6.0 + z * (1.0 / 120.0 + z * (-1.0 / 5040.0 + z * (1.0 / 362_880.0))));
    let c = 1.0 - 0.5 * z
        + z * z
            * (1.0 / 24.0 + z * (-1.0 / 720.0 + z * (1.0 / 40_320.0 + z * (-1.0 / 3_628_800.0))));
    match (kf as i32) & 3 {
        0 => (s, c),
        1 => (c, -s),
        2 => (-s, -c),
        _ => (-c, s),
    }
}

/// [`sincos`]'s fast range: `|x|` at most this reduces in `f32` alone.
const SINCOS_FAST_MAX: f32 = 100_000.0;
const FRAC_2_PI: f32 = 0.636_619_8;
const ROUND_MAGIC: f32 = 12_582_912.0;
/// π/2 = A + B + C: A and B with 8 significant bits (`0x3FC9_0000`,
/// `0x39FD_0000`), C its next 24 (`0x35AA_2217`).
const PIO2_A: f32 = 1.570_312_5;
const PIO2_B: f32 = 4.825_592_041_015_625e-4;
const PIO2_C: f32 = 1.267_590_846_509_847_3e-6;

#[cfg(test)]
mod tests {
    use super::*;

    /// The family's band (`float.md` §6) against the `f64` reference.
    fn within_band(got: f32, want: f64) -> bool {
        (got as f64 - want).abs() <= (1e-6 * want.abs()).max(1e-6)
    }

    #[test]
    fn sincos_is_within_the_band_over_the_fast_range() {
        // Dense near zero, then out to the bound, plus every multiple of π/2
        // up to it and its neighbours (where sin or cos crosses zero).
        extern crate std;
        let mut xs: std::vec::Vec<f32> = (-200_000..=200_000).map(|i| i as f32 * 1e-4).collect();
        let mut x = 20.0f32;
        while x <= SINCOS_FAST_MAX {
            xs.push(x);
            xs.push(-x);
            x *= 1.000_37;
        }
        for k in 0..63_600 {
            let m = (k as f64 * core::f64::consts::FRAC_PI_2) as f32;
            xs.extend([
                m,
                f32::from_bits(m.to_bits() + 1),
                f32::from_bits(m.to_bits().saturating_sub(1)),
            ]);
        }
        for x in xs {
            let (s, c) = sincos(x);
            let (ws, wc) = ((x as f64).sin(), (x as f64).cos());
            assert!(within_band(s, ws), "sin({x:e}) = {s:e}, want {ws:e}");
            assert!(within_band(c, wc), "cos({x:e}) = {c:e}, want {wc:e}");
        }
    }

    #[test]
    fn sincos_keeps_the_signed_zero_and_the_edges() {
        assert_eq!(sincos(0.0), (0.0, 1.0));
        assert_eq!(sincos(-0.0).0.to_bits(), (-0.0f32).to_bits());
        let (s, c) = sincos(f32::NAN);
        assert!(s.is_nan() && c.is_nan());
        let (s, c) = sincos(f32::INFINITY);
        assert!(s.is_nan() && c.is_nan());
        // Past the fast range: libm's exact reduction.
        assert_eq!(sincos(1.0e6), (libm::sinf(1.0e6), libm::cosf(1.0e6)));
    }

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
