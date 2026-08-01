//! Hue value to RGB color (native f32).
//!
//! Transliterated from the canonical GLSL
//! `glsl/lpfn/color/space/hue2rgb.glsl` (normative — see
//! `docs/adr/2026-07-08-glsl-canonical-builtins.md`).
//!
//! The hue2rgb formula (abs/arithmetic ramp per channel) is standard color
//! space mathematics documented in graphics literature; the LightPlayer port
//! was originally written with reference to LYGIA's hue2rgb.glsl
//! (see docs/reports/2026-03-31-lpfx-license-audit.md).
//!
//! **Tolerance:** exact against the canonical f32 — three multiplies, three
//! `abs`, and a clamp.

use crate::f32_math::{abs, clamp};

/// Rust-facing form; `hsv2rgb` calls this directly.
#[inline]
pub(crate) fn hue2rgb(hue: f32) -> [f32; 3] {
    let h6 = hue * 6.0;
    let r = abs(h6 - 3.0) - 1.0;
    let g = 2.0 - abs(h6 - 2.0);
    let b = 2.0 - abs(h6 - 4.0);
    [clamp(r, 0.0, 1.0), clamp(g, 0.0, 1.0), clamp(b, 0.0, 1.0)]
}

/// Hue to RGB (float version).
///
/// # Arguments
/// * `result_ptr` - Pointer to memory where the vec3 result is written
/// * `hue` - Hue in turns (0..1 wraps the colour wheel)
#[lpfn_impl_macro::lpfn_impl(f32, "vec3 lpfn_hue2rgb(float hue)")]
#[allow(
    clippy::not_unsafe_ptr_arg_deref,
    reason = "builtin C ABI writes to caller-provided out-pointer"
)]
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpfn_hue2rgb_f32(result_ptr: *mut f32, hue: f32) {
    let rgb = hue2rgb(hue);
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
    fn the_primaries_land_where_they_should() {
        assert_eq!(hue2rgb(0.0), [1.0, 0.0, 0.0]); // red
        assert_eq!(hue2rgb(1.0 / 3.0), [0.0, 1.0, 0.0]); // green
        assert_eq!(hue2rgb(2.0 / 3.0), [0.0, 0.0, 1.0]); // blue
    }

    #[test]
    fn every_channel_stays_saturated_to_the_unit_interval() {
        for i in -100..=200 {
            let rgb = hue2rgb(i as f32 * 0.01);
            for c in rgb {
                assert!((0.0..=1.0).contains(&c), "channel {c}");
            }
        }
    }

    #[test]
    fn writes_all_three_out_lanes() {
        let mut out = [f32::NAN; 3];
        __lp_lpfn_hue2rgb_f32(out.as_mut_ptr(), 0.5);
        assert_eq!(out, hue2rgb(0.5));
    }
}
