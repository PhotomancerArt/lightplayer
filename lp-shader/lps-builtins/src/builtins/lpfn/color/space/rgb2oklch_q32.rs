//! Convert linear sRGB to Oklch (Q16.16).
//!
//! The inverse of [`super::oklch2rgb_q32`]: Oklab, then chroma `C = |ab|` and
//! hue `h = atan2(b, a)` in **turns**, wrapped to `[0, 1)`. A grey has no hue;
//! it reports `h = 0` (and `C` of a few ulp), so read `C` before trusting `h`.

use super::rgb2oklab_q32::lpfn_rgb2oklab_q32;
use crate::builtins::glsl::atan2_q32::__lps_atan2_q32;
use lps_q32::q32::Q32;
use lps_q32::vec3_q32::Vec3Q32;

/// 1/(2π) in Q16.16.
const INV_TWO_PI: Q32 = Q32::from_fixed(10_430);

/// Convert a linear sRGB color to Oklch `(L, C, h turns)`.
#[inline(always)]
pub fn lpfn_rgb2oklch_q32(rgb: Vec3Q32) -> Vec3Q32 {
    let lab = lpfn_rgb2oklab_q32(rgb);
    let chroma = (lab.y * lab.y + lab.z * lab.z).sqrt();
    let angle = Q32::from_fixed(__lps_atan2_q32(lab.z.to_fixed(), lab.y.to_fixed()));
    let hue = (angle * INV_TWO_PI).frac();
    Vec3Q32::new(lab.x, chroma, hue)
}

/// RGB to Oklch
///
/// Convert a linear RGB color to Oklch (L, C, hue in turns).
///
/// # Arguments
/// * `result_ptr` - Pointer to memory where the vec3 result is written
/// * `x` - R component as i32 (Q32 fixed-point)
/// * `y` - G component as i32 (Q32 fixed-point)
/// * `z` - B component as i32 (Q32 fixed-point)
#[lpfn_impl_macro::lpfn_impl(q32, "vec3 lpfn_rgb2oklch(vec3 rgb)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpfn_rgb2oklch_q32(result_ptr: *mut i32, x: i32, y: i32, z: i32) {
    let result = unsafe { &mut *result_ptr.cast::<[i32; 3]>() };
    let rgb = Vec3Q32::new(Q32::from_fixed(x), Q32::from_fixed(y), Q32::from_fixed(z));
    let lch = lpfn_rgb2oklch_q32(rgb);
    result[0] = lch.x.to_fixed();
    result[1] = lch.y.to_fixed();
    result[2] = lch.z.to_fixed();
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use crate::builtins::lpfn::color::space::oklch2rgb_q32::lpfn_oklch2rgb_q32;
    use crate::util::test_helpers::fixed_to_float;

    #[test]
    fn hue_is_in_turns_and_wrapped() {
        for (r, g, b) in [
            (1.0f32, 0.0f32, 0.0f32),
            (0.0, 1.0, 0.0),
            (0.0, 0.0, 1.0),
            (1.0, 0.0, 1.0),
        ] {
            let lch = lpfn_rgb2oklch_q32(Vec3Q32::from_f32(r, g, b));
            let h = fixed_to_float(lch.z.to_fixed());
            assert!((0.0..1.0).contains(&h), "hue {h} for ({r},{g},{b})");
        }
        // Published: sRGB red has Oklch hue 29.23°.
        let red = lpfn_rgb2oklch_q32(Vec3Q32::from_f32(1.0, 0.0, 0.0));
        let h = fixed_to_float(red.z.to_fixed());
        assert!((h - 29.23 / 360.0).abs() < 2e-3, "red hue {h}");
    }

    #[test]
    fn round_trips_through_oklch2rgb() {
        for (r, g, b) in [
            (1.0f32, 0.0f32, 0.0f32),
            (0.0, 1.0, 0.0),
            (0.0, 0.0, 1.0),
            (0.75, 0.25, 0.5),
            (0.1, 0.9, 0.4),
        ] {
            let back = lpfn_oklch2rgb_q32(lpfn_rgb2oklch_q32(Vec3Q32::from_f32(r, g, b)));
            let got = [back.x, back.y, back.z].map(|c| fixed_to_float(c.to_fixed()));
            for (g2, w) in got.iter().zip([r, g, b]) {
                assert!((g2 - w).abs() < 1e-2, "{got:?} vs ({r},{g},{b})");
            }
        }
    }
}
