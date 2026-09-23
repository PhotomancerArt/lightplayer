//! Convert Oklab to linear sRGB (Q16.16).
//!
//! Oklab (Björn Ottosson, 2020, <https://bottosson.github.io/posts/oklab/>)
//! is a perceptual space whose `L` tracks perceived lightness, so holding `L`
//! and moving `a`/`b` changes color without changing brightness. The matrices
//! are the published definition; `lpc-engine`'s `colorspace.rs` uses the same
//! numbers to bake palettes, so a color built here matches one baked there.
//!
//! "rgb" is **linear** sRGB, the canonical LightPlayer color space
//! (`docs/design/color.md` §2) and what a shader writes. The result is not
//! clamped: an out-of-gamut Oklab color gives channels outside `[0, 1]`, and
//! the texture-write boundary is the one place that gives up range.

use lps_q32::q32::Q32;
use lps_q32::vec3_q32::Vec3Q32;

/// Round a coefficient to the nearest Q16.16 at compile time.
pub(crate) const fn q(x: f64) -> Q32 {
    let scaled = x * 65536.0;
    let rounded = if scaled >= 0.0 {
        scaled + 0.5
    } else {
        scaled - 0.5
    };
    Q32::from_fixed(rounded as i32)
}

// Oklab → nonlinear LMS (`M2` inverse).
const LAB_TO_LMS: [[Q32; 3]; 3] = [
    [q(1.0), q(0.396_337_777_4), q(0.215_803_757_3)],
    [q(1.0), q(-0.105_561_345_8), q(-0.063_854_172_8)],
    [q(1.0), q(-0.089_484_177_5), q(-1.291_485_548_0)],
];

// Linear LMS → linear sRGB (`M1` inverse).
const LMS_TO_RGB: [[Q32; 3]; 3] = [
    [q(4.076_741_662_1), q(-3.307_711_591_3), q(0.230_969_929_2)],
    [q(-1.268_438_004_6), q(2.609_757_401_1), q(-0.341_319_396_5)],
    [q(-0.004_196_086_3), q(-0.703_418_614_7), q(1.707_614_701_0)],
];

/// Convert an Oklab color `(L, a, b)` to linear sRGB.
#[inline(always)]
pub fn lpfn_oklab2rgb_q32(lab: Vec3Q32) -> Vec3Q32 {
    let lms_ = mul3(&LAB_TO_LMS, lab);
    let lms = Vec3Q32::new(
        lms_.x * lms_.x * lms_.x,
        lms_.y * lms_.y * lms_.y,
        lms_.z * lms_.z * lms_.z,
    );
    mul3(&LMS_TO_RGB, lms)
}

/// `m * v` for a row-major 3×3.
#[inline(always)]
pub(crate) fn mul3(m: &[[Q32; 3]; 3], v: Vec3Q32) -> Vec3Q32 {
    Vec3Q32::new(
        m[0][0] * v.x + m[0][1] * v.y + m[0][2] * v.z,
        m[1][0] * v.x + m[1][1] * v.y + m[1][2] * v.z,
        m[2][0] * v.x + m[2][1] * v.y + m[2][2] * v.z,
    )
}

/// Oklab to RGB
///
/// Convert an Oklab color (L, a, b) to linear RGB; L tracks perceived lightness.
///
/// # Arguments
/// * `result_ptr` - Pointer to memory where the vec3 result is written
/// * `x` - L (lightness, 0..1) as i32 (Q32 fixed-point)
/// * `y` - a (green–red, about -0.4..0.4) as i32 (Q32 fixed-point)
/// * `z` - b (blue–yellow, about -0.4..0.4) as i32 (Q32 fixed-point)
#[lpfn_impl_macro::lpfn_impl(q32, "vec3 lpfn_oklab2rgb(vec3 lab)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpfn_oklab2rgb_q32(result_ptr: *mut i32, x: i32, y: i32, z: i32) {
    let result = unsafe { &mut *result_ptr.cast::<[i32; 3]>() };
    let lab = Vec3Q32::new(Q32::from_fixed(x), Q32::from_fixed(y), Q32::from_fixed(z));
    let rgb = lpfn_oklab2rgb_q32(lab);
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
    fn white_and_black() {
        let white = lpfn_oklab2rgb_q32(Vec3Q32::from_f32(1.0, 0.0, 0.0));
        for c in [white.x, white.y, white.z] {
            assert!((fixed_to_float(c.to_fixed()) - 1.0).abs() < 2e-3, "{c:?}");
        }
        let black = lpfn_oklab2rgb_q32(Vec3Q32::zero());
        assert_eq!(black, Vec3Q32::zero());
    }

    #[test]
    fn matches_the_published_red() {
        // Linear sRGB red is Oklab (0.627955, 0.224863, 0.125846).
        let rgb = lpfn_oklab2rgb_q32(Vec3Q32::from_f32(0.627_955, 0.224_863, 0.125_846));
        let got = [rgb.x, rgb.y, rgb.z].map(|c| fixed_to_float(c.to_fixed()));
        for (g, w) in got.iter().zip([1.0f32, 0.0, 0.0]) {
            assert!((g - w).abs() < 3e-3, "{got:?}");
        }
    }
}
