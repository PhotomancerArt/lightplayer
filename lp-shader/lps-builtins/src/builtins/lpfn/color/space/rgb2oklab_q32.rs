//! Convert linear sRGB to Oklab (Q16.16).
//!
//! The inverse of [`super::oklab2rgb_q32`]; see there for the space, the
//! source of the matrices and why "rgb" means linear sRGB. Inputs outside
//! `[0, 1]` are legal: the cube root keeps the sign, so an out-of-gamut color
//! round-trips instead of folding.

use super::oklab2rgb_q32::{mul3, q};
use lps_q32::q32::Q32;
use lps_q32::vec3_q32::Vec3Q32;

// Linear sRGB → linear LMS (`M1`).
const RGB_TO_LMS: [[Q32; 3]; 3] = [
    [q(0.412_221_470_8), q(0.536_332_536_3), q(0.051_445_992_9)],
    [q(0.211_903_498_2), q(0.680_699_545_1), q(0.107_396_956_6)],
    [q(0.088_302_461_9), q(0.281_718_837_6), q(0.629_978_700_5)],
];

// Nonlinear LMS → Oklab (`M2`).
const LMS_TO_LAB: [[Q32; 3]; 3] = [
    [q(0.210_454_255_3), q(0.793_617_785_0), q(-0.004_072_046_8)],
    [q(1.977_998_495_1), q(-2.428_592_205_0), q(0.450_593_709_9)],
    [q(0.025_904_037_1), q(0.782_771_766_2), q(-0.808_675_766_0)],
];

/// Convert a linear sRGB color to Oklab `(L, a, b)`.
#[inline(always)]
pub fn lpfn_rgb2oklab_q32(rgb: Vec3Q32) -> Vec3Q32 {
    let lms = mul3(&RGB_TO_LMS, rgb);
    let lms_ = Vec3Q32::new(cbrt_q32(lms.x), cbrt_q32(lms.y), cbrt_q32(lms.z));
    mul3(&LMS_TO_LAB, lms_)
}

/// Signed cube root in Q16.16, exact to the last bit (floor of the true root).
///
/// `cbrt(X / 2^16) * 2^16 = cbrt(X * 2^32)`, so the fixed-point root is the
/// integer cube root of the raw value shifted up 32. Done digit by digit
/// (Hacker's Delight `icbrt`, widened to 64 bits): 22 steps, no division, no
/// float, and no approximation to carry a tolerance.
pub(crate) fn cbrt_q32(x: Q32) -> Q32 {
    let raw = x.to_fixed();
    let mut rem = (u64::from(raw.unsigned_abs())) << 32;
    let mut root: u64 = 0;
    let mut shift: i32 = 63;
    while shift >= 0 {
        root <<= 1;
        let b = 3 * root * (root + 1) + 1;
        if (rem >> shift) >= b {
            rem -= b << shift;
            root += 1;
        }
        shift -= 3;
    }
    // |X| < 2^31 → root < 2^21: always fits.
    let root = root as i32;
    Q32::from_fixed(if raw < 0 { -root } else { root })
}

/// RGB to Oklab
///
/// Convert a linear RGB color to Oklab (L, a, b); L tracks perceived lightness.
///
/// # Arguments
/// * `result_ptr` - Pointer to memory where the vec3 result is written
/// * `x` - R component as i32 (Q32 fixed-point)
/// * `y` - G component as i32 (Q32 fixed-point)
/// * `z` - B component as i32 (Q32 fixed-point)
#[lpfn_impl_macro::lpfn_impl(q32, "vec3 lpfn_rgb2oklab(vec3 rgb)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpfn_rgb2oklab_q32(result_ptr: *mut i32, x: i32, y: i32, z: i32) {
    let result = unsafe { &mut *result_ptr.cast::<[i32; 3]>() };
    let rgb = Vec3Q32::new(Q32::from_fixed(x), Q32::from_fixed(y), Q32::from_fixed(z));
    let lab = lpfn_rgb2oklab_q32(rgb);
    result[0] = lab.x.to_fixed();
    result[1] = lab.y.to_fixed();
    result[2] = lab.z.to_fixed();
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use crate::builtins::lpfn::color::space::oklab2rgb_q32::lpfn_oklab2rgb_q32;
    use crate::util::test_helpers::fixed_to_float;

    #[test]
    fn cbrt_is_exact_on_cubes_and_keeps_sign() {
        for (x, want) in [
            (0.0f32, 0.0f32),
            (1.0, 1.0),
            (8.0, 2.0),
            (0.125, 0.5),
            (-0.125, -0.5),
        ] {
            let got = fixed_to_float(cbrt_q32(Q32::from_f32_wrapping(x)).to_fixed());
            assert_eq!(got, want, "cbrt({x})");
        }
    }

    #[test]
    fn cbrt_is_the_floor_root_everywhere() {
        // For every sampled raw value, root^3 <= X*2^32 < (root+1)^3.
        for raw in (0..i32::MAX).step_by(9_973) {
            let r = i128::from(cbrt_q32(Q32::from_fixed(raw)).to_fixed());
            let x = i128::from(raw) << 32;
            assert!(
                r * r * r <= x && (r + 1) * (r + 1) * (r + 1) > x,
                "raw {raw}"
            );
        }
    }

    #[test]
    fn white_is_l1_and_grey_has_no_chroma() {
        let lab = lpfn_rgb2oklab_q32(Vec3Q32::one());
        assert!((fixed_to_float(lab.x.to_fixed()) - 1.0).abs() < 1e-3);
        for v in [0.1f32, 0.5, 0.9] {
            let lab = lpfn_rgb2oklab_q32(Vec3Q32::from_f32(v, v, v));
            assert!(
                fixed_to_float(lab.y.to_fixed()).abs() < 1e-3,
                "a for grey {v}"
            );
            assert!(
                fixed_to_float(lab.z.to_fixed()).abs() < 1e-3,
                "b for grey {v}"
            );
        }
    }

    #[test]
    fn round_trips_through_oklab2rgb() {
        for (r, g, b) in [
            (1.0f32, 0.0f32, 0.0f32),
            (0.0, 1.0, 0.0),
            (0.0, 0.0, 1.0),
            (0.75, 0.25, 0.5),
            (0.1, 0.9, 0.4),
            (0.02, 0.03, 0.01),
        ] {
            let back = lpfn_oklab2rgb_q32(lpfn_rgb2oklab_q32(Vec3Q32::from_f32(r, g, b)));
            let got = [back.x, back.y, back.z].map(|c| fixed_to_float(c.to_fixed()));
            for (g2, w) in got.iter().zip([r, g, b]) {
                assert!((g2 - w).abs() < 3e-3, "{got:?} vs ({r},{g},{b})");
            }
        }
    }
}
