//! Linear sRGB to Oklab (native f32).
//!
//! Transliterated from the canonical GLSL
//! `glsl/lpfn/color/space/rgb2oklab.glsl` (normative); see
//! [`super::oklab2rgb_q32`] for the space.
//!
//! **Tolerance:** `1e-6` absolute against the canonical f32. The canonical
//! spells the signed cube root `sign(x) * pow(abs(x), 1/3)` (GLSL has no
//! `cbrt`); this uses `libm::cbrtf`, which is the same function computed
//! more accurately.

#[inline]
pub(crate) fn rgb2oklab(r: f32, g: f32, b: f32) -> [f32; 3] {
    let l = 0.412_221_46 * r + 0.536_332_55 * g + 0.051_445_995 * b;
    let m = 0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b;
    let s = 0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b;
    let l_ = libm::cbrtf(l);
    let m_ = libm::cbrtf(m);
    let s_ = libm::cbrtf(s);
    [
        0.210_454_26 * l_ + 0.793_617_8 * m_ - 0.004_072_047 * s_,
        1.977_998_5 * l_ - 2.428_592_2 * m_ + 0.450_593_7 * s_,
        0.025_904_037 * l_ + 0.782_771_77 * m_ - 0.808_675_77 * s_,
    ]
}

/// RGB to Oklab (float version).
///
/// # Arguments
/// * `result_ptr` - Pointer to memory where the vec3 result is written
/// * `x` / `y` / `z` - Red / green / blue (linear)
#[lpfn_impl_macro::lpfn_impl(f32, "vec3 lpfn_rgb2oklab(vec3 rgb)")]
#[allow(
    clippy::not_unsafe_ptr_arg_deref,
    reason = "builtin C ABI writes to caller-provided out-pointer"
)]
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpfn_rgb2oklab_f32(result_ptr: *mut f32, x: f32, y: f32, z: f32) {
    let lab = rgb2oklab(x, y, z);
    unsafe {
        *result_ptr = lab[0];
        *result_ptr.add(1) = lab[1];
        *result_ptr.add(2) = lab[2];
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtins::lpfn::color::space::oklab2rgb_f32::oklab2rgb;

    #[test]
    fn round_trips_through_oklab2rgb() {
        for (r, g, b) in [
            (1.0f32, 0.0f32, 0.0f32),
            (0.0, 1.0, 0.0),
            (0.0, 0.0, 1.0),
            (0.75, 0.25, 0.5),
            (1.25, -0.1, 0.5),
        ] {
            let lab = rgb2oklab(r, g, b);
            let back = oklab2rgb(lab[0], lab[1], lab[2]);
            for (got, want) in back.iter().zip([r, g, b]) {
                assert!((got - want).abs() < 1e-4, "{back:?} vs ({r},{g},{b})");
            }
        }
    }
}
