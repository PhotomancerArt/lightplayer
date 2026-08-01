//! HSV to RGB conversion (native f32).
//!
//! Transliterated from the canonical GLSL
//! `glsl/lpfn/color/space/hsv2rgb.glsl` (normative).
//!
//! HSV→RGB conversion is standard mathematical procedure (Foley & van Dam);
//! the LightPlayer port was originally written with reference to LYGIA's
//! hsv2rgb.glsl (see docs/reports/2026-03-31-lpfx-license-audit.md).
//!
//! **Tolerance:** exact against the canonical f32.

use super::hue2rgb_f32::hue2rgb;

#[inline]
fn hsv2rgb(h: f32, s: f32, v: f32) -> [f32; 3] {
    // ((hue2rgb(h) - 1.0) * s + 1.0) * v
    let rgb = hue2rgb(h);
    [
        ((rgb[0] - 1.0) * s + 1.0) * v,
        ((rgb[1] - 1.0) * s + 1.0) * v,
        ((rgb[2] - 1.0) * s + 1.0) * v,
    ]
}

/// HSV to RGB (float version).
///
/// # Arguments
/// * `result_ptr` - Pointer to memory where the vec3 result is written
/// * `x` / `y` / `z` - Hue / saturation / value
#[lpfn_impl_macro::lpfn_impl(f32, "vec3 lpfn_hsv2rgb(vec3 hsv)")]
#[allow(
    clippy::not_unsafe_ptr_arg_deref,
    reason = "builtin C ABI writes to caller-provided out-pointer"
)]
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpfn_hsv2rgb_f32(result_ptr: *mut f32, x: f32, y: f32, z: f32) {
    let rgb = hsv2rgb(x, y, z);
    unsafe {
        *result_ptr = rgb[0];
        *result_ptr.add(1) = rgb[1];
        *result_ptr.add(2) = rgb[2];
    }
}

/// HSV to RGB, vec4 form: alpha passes through untouched.
///
/// # Arguments
/// * `result_ptr` - Pointer to memory where the vec4 result is written
/// * `x` / `y` / `z` - Hue / saturation / value
/// * `w` - Alpha, copied to the result unchanged
#[lpfn_impl_macro::lpfn_impl(f32, "vec4 lpfn_hsv2rgb(vec4 hsv)")]
#[allow(
    clippy::not_unsafe_ptr_arg_deref,
    reason = "builtin C ABI writes to caller-provided out-pointer"
)]
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpfn_hsv2rgb_vec4_f32(result_ptr: *mut f32, x: f32, y: f32, z: f32, w: f32) {
    let rgb = hsv2rgb(x, y, z);
    unsafe {
        *result_ptr = rgb[0];
        *result_ptr.add(1) = rgb[1];
        *result_ptr.add(2) = rgb[2];
        *result_ptr.add(3) = w;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_saturation_is_grey_at_the_value_level() {
        for v in [0.0f32, 0.25, 1.0] {
            assert_eq!(hsv2rgb(0.37, 0.0, v), [v, v, v]);
        }
    }

    #[test]
    fn full_saturation_and_value_gives_the_primaries() {
        assert_eq!(hsv2rgb(0.0, 1.0, 1.0), [1.0, 0.0, 0.0]);
        assert_eq!(hsv2rgb(1.0 / 3.0, 1.0, 1.0), [0.0, 1.0, 0.0]);
        assert_eq!(hsv2rgb(2.0 / 3.0, 1.0, 1.0), [0.0, 0.0, 1.0]);
    }

    #[test]
    fn zero_value_is_black_whatever_the_hue() {
        for i in 0..20 {
            assert_eq!(hsv2rgb(i as f32 * 0.05, 1.0, 0.0), [0.0, 0.0, 0.0]);
        }
    }

    #[test]
    fn alpha_passes_through_the_vec4_form_untouched() {
        let mut out = [f32::NAN; 4];
        __lp_lpfn_hsv2rgb_vec4_f32(out.as_mut_ptr(), 0.0, 1.0, 1.0, 0.375);
        assert_eq!(out, [1.0, 0.0, 0.0, 0.375]);
    }
}
