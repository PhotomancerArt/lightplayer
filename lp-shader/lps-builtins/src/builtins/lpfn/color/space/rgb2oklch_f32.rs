//! Linear sRGB to Oklch (native f32).
//!
//! Transliterated from the canonical GLSL
//! `glsl/lpfn/color/space/rgb2oklch.glsl` (normative). Hue is in **turns**,
//! wrapped to `[0, 1)`; see [`super::rgb2oklch_q32`].
//!
//! **Tolerance:** `1e-6` absolute against the canonical f32, except that a
//! grey's hue is meaningless (atan2 of two roundoff values) and is not
//! asserted.

use super::rgb2oklab_f32::rgb2oklab;

#[inline]
fn rgb2oklch(r: f32, g: f32, b: f32) -> [f32; 3] {
    let lab = rgb2oklab(r, g, b);
    let c = crate::f32_math::sqrt(lab[1] * lab[1] + lab[2] * lab[2]);
    let h = crate::f32_math::fract(libm::atan2f(lab[2], lab[1]) / core::f32::consts::TAU);
    [lab[0], c, h]
}

/// RGB to Oklch (float version).
///
/// # Arguments
/// * `result_ptr` - Pointer to memory where the vec3 result is written
/// * `x` / `y` / `z` - Red / green / blue (linear)
#[lpfn_impl_macro::lpfn_impl(f32, "vec3 lpfn_rgb2oklch(vec3 rgb)")]
#[allow(
    clippy::not_unsafe_ptr_arg_deref,
    reason = "builtin C ABI writes to caller-provided out-pointer"
)]
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpfn_rgb2oklch_f32(result_ptr: *mut f32, x: f32, y: f32, z: f32) {
    let lch = rgb2oklch(x, y, z);
    unsafe {
        *result_ptr = lch[0];
        *result_ptr.add(1) = lch[1];
        *result_ptr.add(2) = lch[2];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn red_hue_matches_the_published_value() {
        let lch = rgb2oklch(1.0, 0.0, 0.0);
        assert!((lch[2] - 29.233_885 / 360.0).abs() < 1e-4, "{lch:?}");
        assert!((lch[1] - 0.257_683).abs() < 1e-4, "{lch:?}");
    }
}
