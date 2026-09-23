//! Oklch to linear sRGB (native f32).
//!
//! Transliterated from the canonical GLSL
//! `glsl/lpfn/color/space/oklch2rgb.glsl` (normative). Hue is in **turns**;
//! see [`super::oklch2rgb_q32`].
//!
//! **Tolerance:** `1e-6` absolute against the canonical f32 (`libm` sin/cos
//! against the interpreter's).

use super::oklab2rgb_f32::oklab2rgb;

#[inline]
fn oklch2rgb(l: f32, c: f32, h: f32) -> [f32; 3] {
    let angle = crate::f32_math::fract(h) * core::f32::consts::TAU;
    oklab2rgb(l, c * libm::cosf(angle), c * libm::sinf(angle))
}

/// Oklch to RGB (float version).
///
/// # Arguments
/// * `result_ptr` - Pointer to memory where the vec3 result is written
/// * `x` / `y` / `z` - L / C / hue in turns
#[lpfn_impl_macro::lpfn_impl(f32, "vec3 lpfn_oklch2rgb(vec3 lch)")]
#[allow(
    clippy::not_unsafe_ptr_arg_deref,
    reason = "builtin C ABI writes to caller-provided out-pointer"
)]
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpfn_oklch2rgb_f32(result_ptr: *mut f32, x: f32, y: f32, z: f32) {
    let rgb = oklch2rgb(x, y, z);
    unsafe {
        *result_ptr = rgb[0];
        *result_ptr.add(1) = rgb[1];
        *result_ptr.add(2) = rgb[2];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_chroma_is_grey() {
        let rgb = oklch2rgb(0.7, 0.0, 0.37);
        assert!((rgb[0] - rgb[1]).abs() < 1e-5 && (rgb[1] - rgb[2]).abs() < 1e-5);
    }
}
