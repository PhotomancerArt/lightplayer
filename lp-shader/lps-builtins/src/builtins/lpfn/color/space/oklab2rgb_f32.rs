//! Oklab to linear sRGB (native f32).
//!
//! Transliterated from the canonical GLSL
//! `glsl/lpfn/color/space/oklab2rgb.glsl` (normative); the space and the
//! matrices are described in [`super::oklab2rgb_q32`].
//!
//! **Tolerance:** exact against the canonical f32 (same operations, same
//! order).

#[inline]
pub(crate) fn oklab2rgb(l: f32, a: f32, b: f32) -> [f32; 3] {
    let l_ = l + 0.396_337_78 * a + 0.215_803_76 * b;
    let m_ = l - 0.105_561_346 * a - 0.063_854_17 * b;
    let s_ = l - 0.089_484_18 * a - 1.291_485_5 * b;
    let l3 = l_ * l_ * l_;
    let m3 = m_ * m_ * m_;
    let s3 = s_ * s_ * s_;
    [
        4.076_741_7 * l3 - 3.307_711_6 * m3 + 0.230_969_94 * s3,
        -1.268_438 * l3 + 2.609_757_4 * m3 - 0.341_319_38 * s3,
        -0.004_196_086_3 * l3 - 0.703_418_6 * m3 + 1.707_614_7 * s3,
    ]
}

/// Oklab to RGB (float version).
///
/// # Arguments
/// * `result_ptr` - Pointer to memory where the vec3 result is written
/// * `x` / `y` / `z` - L / a / b
#[lpfn_impl_macro::lpfn_impl(f32, "vec3 lpfn_oklab2rgb(vec3 lab)")]
#[allow(
    clippy::not_unsafe_ptr_arg_deref,
    reason = "builtin C ABI writes to caller-provided out-pointer"
)]
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpfn_oklab2rgb_f32(result_ptr: *mut f32, x: f32, y: f32, z: f32) {
    let rgb = oklab2rgb(x, y, z);
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
    fn white_is_one_and_red_matches_the_published_value() {
        for c in oklab2rgb(1.0, 0.0, 0.0) {
            assert!((c - 1.0).abs() < 1e-4, "{c}");
        }
        let red = oklab2rgb(0.627_955, 0.224_863, 0.125_846);
        for (g, w) in red.iter().zip([1.0f32, 0.0, 0.0]) {
            assert!((g - w).abs() < 1e-4, "{red:?}");
        }
    }
}
