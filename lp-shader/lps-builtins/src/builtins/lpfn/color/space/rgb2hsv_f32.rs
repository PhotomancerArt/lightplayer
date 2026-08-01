//! RGB to HSV conversion (native f32).
//!
//! Transliterated from the canonical GLSL
//! `glsl/lpfn/color/space/rgb2hsv.glsl` (normative).
//!
//! Algorithm: Sam Hocevar's branch-minimizing RGB→HSV
//! (<http://lolengine.net/blog/2013/07/27/rgb-to-hsv-in-glsl>), a widely
//! referenced standard formulation (see
//! docs/reports/2026-03-31-lpfx-license-audit.md).
//!
//! **The epsilon is `1/65536`, not `1e-10`.** That is the smallest positive
//! Q16.16 value, chosen so the canonical float semantics and the Q32 device
//! implementation use the same guard against division by zero — a deliberate
//! LightPlayer deviation from LYGIA's `1e-10`. Keeping it in the f32 port is
//! what makes the two float modes agree on greys; "improving" it to a smaller
//! epsilon would silently change every desaturated colour.
//!
//! **Tolerance:** exact against the canonical f32.

// The `p.zw` constants of Hocevar's formulation, as f32 *bit patterns*.
//
// **This is an upstream-LLVM workaround, not a preference.** Written the
// natural way — `if g < b { [b, g, -1.0, 2.0 / 3.0] } else { [g, b, 0.0,
// -1.0 / 3.0] }` — LLVM materialises the constant lane pair as a
// `[2 x float]` constant-pool entry, and the esp Xtensa backend cannot select
// `XtensaISD::PCREL_WRAPPER` over a `TargetConstantPool`, so the whole f32
// builtins family fails to build for `xtensa-esp32s3-none-elf` at every
// optimisation level. Selecting the integer bit patterns and bitcasting after
// the fact keeps the constants out of the float constant pool.
//
// Defect: docs/defects/2026-08-01-xtensa-backend-cannot-select-float-constant-pool.md
// Upstream: not yet filed.
//
// `f32::to_bits` is const-evaluated, so these are the same floats the literals
// were; `hocevar_p_is_bit_identical_to_the_literal_form` pins that, and is the
// test to keep green if this is ever reverted.
const P_Z_IF: u32 = (-1.0f32).to_bits();
const P_W_IF: u32 = (2.0f32 / 3.0).to_bits();
const P_Z_ELSE: u32 = (0.0f32).to_bits();
const P_W_ELSE: u32 = (-1.0f32 / 3.0).to_bits();

/// Hocevar's `p` term: the green/blue ordering step.
///
/// Split out of `rgb2hsv` only so the constant-pool workaround above can be
/// tested against the literal form it replaced.
#[inline(always)]
fn hocevar_p(g: f32, b: f32) -> [f32; 4] {
    let g_lt_b = g < b;
    [
        if g_lt_b { b } else { g },
        if g_lt_b { g } else { b },
        f32::from_bits(if g_lt_b { P_Z_IF } else { P_Z_ELSE }),
        f32::from_bits(if g_lt_b { P_W_IF } else { P_W_ELSE }),
    ]
}

/// Rust-facing form.
#[inline]
fn rgb2hsv(r: f32, g: f32, b: f32) -> [f32; 3] {
    const EPSILON: f32 = 1.0 / 65536.0;

    let p = hocevar_p(g, b);
    let q: [f32; 4] = if r < p[0] {
        [p[0], p[1], p[3], r]
    } else {
        [r, p[1], p[2], p[0]]
    };

    let d = q[0] - if q[3] < q[1] { q[3] } else { q[1] };
    let h = crate::f32_math::abs(q[2] + (q[3] - q[1]) / (6.0 * d + EPSILON));
    let s = d / (q[0] + EPSILON);
    let v = q[0];
    [h, s, v]
}

/// RGB to HSV (float version).
///
/// # Arguments
/// * `result_ptr` - Pointer to memory where the vec3 result is written
/// * `x` / `y` / `z` - Red / green / blue
#[lpfn_impl_macro::lpfn_impl(f32, "vec3 lpfn_rgb2hsv(vec3 rgb)")]
#[allow(
    clippy::not_unsafe_ptr_arg_deref,
    reason = "builtin C ABI writes to caller-provided out-pointer"
)]
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpfn_rgb2hsv_f32(result_ptr: *mut f32, x: f32, y: f32, z: f32) {
    let hsv = rgb2hsv(x, y, z);
    unsafe {
        *result_ptr = hsv[0];
        *result_ptr.add(1) = hsv[1];
        *result_ptr.add(2) = hsv[2];
    }
}

/// RGB to HSV, vec4 form: alpha passes through untouched.
#[lpfn_impl_macro::lpfn_impl(f32, "vec4 lpfn_rgb2hsv(vec4 rgb)")]
#[allow(
    clippy::not_unsafe_ptr_arg_deref,
    reason = "builtin C ABI writes to caller-provided out-pointer"
)]
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpfn_rgb2hsv_vec4_f32(result_ptr: *mut f32, x: f32, y: f32, z: f32, w: f32) {
    let hsv = rgb2hsv(x, y, z);
    unsafe {
        *result_ptr = hsv[0];
        *result_ptr.add(1) = hsv[1];
        *result_ptr.add(2) = hsv[2];
        *result_ptr.add(3) = w;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtins::lpfn::color::space::hsv2rgb_f32::__lp_lpfn_hsv2rgb_f32;

    #[test]
    fn value_is_the_max_channel() {
        assert!((rgb2hsv(0.25, 0.5, 0.125)[2] - 0.5).abs() < 1e-6);
        assert!((rgb2hsv(1.0, 0.0, 0.0)[2] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn grey_has_no_saturation() {
        for v in [0.25f32, 0.5, 1.0] {
            let hsv = rgb2hsv(v, v, v);
            assert!(hsv[1] < 1e-3, "saturation {} for grey {v}", hsv[1]);
        }
    }

    #[test]
    fn pure_black_does_not_divide_by_zero() {
        // The epsilon guard is the whole reason this does not produce NaN.
        let hsv = rgb2hsv(0.0, 0.0, 0.0);
        for c in hsv {
            assert!(c.is_finite(), "non-finite channel {c}");
        }
    }

    #[test]
    fn round_trips_through_hsv2rgb() {
        for (r, g, b) in [
            (1.0f32, 0.0f32, 0.0f32),
            (0.0, 1.0, 0.0),
            (0.0, 0.0, 1.0),
            (0.75, 0.25, 0.5),
            (0.1, 0.9, 0.4),
        ] {
            let hsv = rgb2hsv(r, g, b);
            let mut back = [0.0f32; 3];
            __lp_lpfn_hsv2rgb_f32(back.as_mut_ptr(), hsv[0], hsv[1], hsv[2]);
            for (got, want) in back.iter().zip([r, g, b]) {
                assert!((got - want).abs() < 2e-3, "{back:?} vs ({r},{g},{b})");
            }
        }
    }

    /// The pre-workaround form of `hocevar_p`, kept verbatim as the oracle.
    ///
    /// This is the code the Xtensa constant-pool workaround replaced. It stays
    /// here so the rewrite's bit-equivalence is *pinned* rather than argued —
    /// these builtins are shared with rv32 and wasm, where the original form
    /// compiles fine and any drift would be a silent behaviour change.
    fn hocevar_p_literal(g: f32, b: f32) -> [f32; 4] {
        if g < b {
            [b, g, -1.0, 2.0 / 3.0]
        } else {
            [g, b, 0.0, -1.0 / 3.0]
        }
    }

    #[test]
    fn hocevar_p_is_bit_identical_to_the_literal_form() {
        // A grid that crosses g == b in both directions, plus the cases where
        // a sign-of-zero or NaN difference would hide: -0.0/+0.0 must select
        // the same lanes, and `g < b` is false for any NaN operand.
        const SAMPLES: [f32; 11] = [
            -1.0,
            -0.75,
            -0.5,
            -0.25,
            0.0,
            0.25,
            0.5,
            0.75,
            1.0,
            -0.0,
            f32::NAN,
        ];

        for &g in SAMPLES.iter() {
            for &b in SAMPLES.iter() {
                let got = hocevar_p(g, b);
                let want = hocevar_p_literal(g, b);
                for k in 0..4 {
                    assert_eq!(
                        got[k].to_bits(),
                        want[k].to_bits(),
                        "lane {k} for g={g} b={b}: {:#010x} vs {:#010x}",
                        got[k].to_bits(),
                        want[k].to_bits()
                    );
                }
            }
        }
    }

    #[test]
    fn alpha_passes_through_the_vec4_form_untouched() {
        let mut out = [f32::NAN; 4];
        __lp_lpfn_rgb2hsv_vec4_f32(out.as_mut_ptr(), 1.0, 0.0, 0.0, 0.375);
        assert_eq!(out[3], 0.375);
    }
}
