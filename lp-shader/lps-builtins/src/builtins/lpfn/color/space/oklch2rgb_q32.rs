//! Convert Oklch to linear sRGB (Q16.16).
//!
//! Oklch is Oklab in polar form: `(L, C, h)` with `a = C cos h`,
//! `b = C sin h`. Holding `L` and `C` and moving `h` walks the hue circle at
//! one perceived lightness, which is what makes it the space for color waves
//! that do not pulse in brightness.
//!
//! **Hue is in turns (`0..1`), not degrees**, like `lpfn_hsv2rgb`'s hue, so a
//! phasor or `fract(time)` drives it directly. Any value is legal; it wraps.
//! (Palette stops, which speak CSS, use degrees — `lpc-engine`'s
//! `colorspace.rs`. Shaders speak turns.)

use super::oklab2rgb_q32::lpfn_oklab2rgb_q32;
use crate::builtins::glsl::sincos_q32::lps_sincos_q32_pair;
use lps_q32::q32::Q32;
use lps_q32::vec3_q32::Vec3Q32;

/// 2π in Q16.16.
const TWO_PI: Q32 = Q32::from_fixed(411_775);

/// Convert an Oklch color `(L, C, h turns)` to linear sRGB.
#[inline(always)]
pub fn lpfn_oklch2rgb_q32(lch: Vec3Q32) -> Vec3Q32 {
    // `frac` is `x - floor(x)` on the raw bits, so negative hues wrap too,
    // and the angle handed to sin/cos stays in [0, 2π).
    let angle = lch.z.frac() * TWO_PI;
    let (sin, cos) = lps_sincos_q32_pair(angle.to_fixed());
    let a = lch.y * Q32::from_fixed(cos);
    let b = lch.y * Q32::from_fixed(sin);
    lpfn_oklab2rgb_q32(Vec3Q32::new(lch.x, a, b))
}

/// Oklch to RGB
///
/// Convert an Oklch color (L, C, hue in turns) to linear RGB; hold L and C and move the hue for even-brightness color.
///
/// # Arguments
/// * `result_ptr` - Pointer to memory where the vec3 result is written
/// * `x` - L (lightness, 0..1) as i32 (Q32 fixed-point)
/// * `y` - C (chroma, about 0..0.37) as i32 (Q32 fixed-point)
/// * `z` - h (hue in turns, wraps) as i32 (Q32 fixed-point)
#[lpfn_impl_macro::lpfn_impl(q32, "vec3 lpfn_oklch2rgb(vec3 lch)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpfn_oklch2rgb_q32(result_ptr: *mut i32, x: i32, y: i32, z: i32) {
    let result = unsafe { &mut *result_ptr.cast::<[i32; 3]>() };
    let lch = Vec3Q32::new(Q32::from_fixed(x), Q32::from_fixed(y), Q32::from_fixed(z));
    let rgb = lpfn_oklch2rgb_q32(lch);
    result[0] = rgb.x.to_fixed();
    result[1] = rgb.y.to_fixed();
    result[2] = rgb.z.to_fixed();
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use crate::util::test_helpers::fixed_to_float;

    #[test]
    fn zero_chroma_is_grey_at_any_hue() {
        for h in [-0.3f32, 0.0, 0.25, 0.9, 3.5] {
            let rgb = lpfn_oklch2rgb_q32(Vec3Q32::from_f32(0.7, 0.0, h));
            let c = [rgb.x, rgb.y, rgb.z].map(|c| fixed_to_float(c.to_fixed()));
            assert!(
                (c[0] - c[1]).abs() < 1e-3 && (c[1] - c[2]).abs() < 1e-3,
                "{c:?}"
            );
        }
    }

    #[test]
    fn hue_wraps_by_whole_turns() {
        let base = lpfn_oklch2rgb_q32(Vec3Q32::from_f32(0.7, 0.1, 0.25));
        for h in [1.25f32, -0.75, 5.25] {
            let rgb = lpfn_oklch2rgb_q32(Vec3Q32::from_f32(0.7, 0.1, h));
            for (g, w) in [rgb.x, rgb.y, rgb.z].iter().zip([base.x, base.y, base.z]) {
                assert_eq!(g.to_fixed(), w.to_fixed(), "h {h}");
            }
        }
    }
}
