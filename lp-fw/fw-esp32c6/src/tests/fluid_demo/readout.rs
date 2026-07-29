//! Convert sampled (r, g, b) Q32 values into a packed `[u8; 723]`
//! frame buffer (241 lamps × 3 channels). lp2014 normalize-by-max
//! preserves hue: divide each channel by `max(1.0, r, g, b)` so any
//! channel above 1.0 pulls the others down proportionally.

use lps_q32::Q32;

use crate::tests::fluid_demo::ring_geometry::LAMP_COUNT;
use crate::tests::fluid_demo::sampler::sample_rgb;
use crate::tests::msafluid_solver::MsaFluidSolver;

pub const FRAME_BYTES: usize = LAMP_COUNT * 3;

/// Sample the solver at every lamp position and write the result as
/// gamma-naïve `[u8; FRAME_BYTES]` in RGB triplet order. The display
/// pipeline applies its own gamma / brightness LUTs downstream.
pub fn render_frame(
    solver: &MsaFluidSolver,
    lamp_positions: &[(f32, f32); LAMP_COUNT],
    out: &mut [u8; FRAME_BYTES],
) {
    for (i, &(x, y)) in lamp_positions.iter().enumerate() {
        let (r, g, b) = sample_rgb(solver, x, y);
        let (r8, g8, b8) = q32_rgb_to_u8_normalized(r, g, b);
        let off = i * 3;
        out[off] = r8;
        out[off + 1] = g8;
        out[off + 2] = b8;
    }
}

// ----- helpers (private) ---------------------------------------------

/// lp2014 normalize-by-max → 8-bit. Each channel is divided by
/// `max(1.0, r, g, b)`; channels below the cap pass through unchanged
/// (in normalized form), channels above the cap are scaled down
/// preserving hue. Negative values clamp to zero before normalization.
fn q32_rgb_to_u8_normalized(r: Q32, g: Q32, b: Q32) -> (u8, u8, u8) {
    let zero = Q32::ZERO;
    let one = Q32::from_f32_wrapping(1.0);
    let r = if r > zero { r } else { zero };
    let g = if g > zero { g } else { zero };
    let b = if b > zero { b } else { zero };

    let mx = max3(r, g, b);
    let denom = if mx > one { mx } else { one };

    let to_u8 = |c: Q32| -> u8 {
        let v = c / denom;
        let scaled = v * Q32::from_f32_wrapping(255.0);
        let raw = scaled.to_f32();
        let clamped = raw.clamp(0.0, 255.0);
        clamped as u8
    };

    (to_u8(r), to_u8(g), to_u8(b))
}

fn max3(a: Q32, b: Q32, c: Q32) -> Q32 {
    let ab = if a > b { a } else { b };
    if ab > c { ab } else { c }
}

#[cfg(test)]
mod tests {
    use lps_q32::Q32;

    use super::q32_rgb_to_u8_normalized;

    #[test]
    fn normalize_passthrough_below_cap() {
        let one = Q32::from_f32_wrapping(1.0);
        let half = Q32::from_f32_wrapping(0.5);
        let zero = Q32::ZERO;
        let (r, g, b) = q32_rgb_to_u8_normalized(one, half, zero);
        assert_eq!(r, 255);
        assert!((g as i32 - 127).abs() <= 1);
        assert_eq!(b, 0);
    }

    #[test]
    fn normalize_scales_when_above_cap() {
        let two = Q32::from_f32_wrapping(2.0);
        let one = Q32::from_f32_wrapping(1.0);
        let (r, g, _) = q32_rgb_to_u8_normalized(two, one, Q32::ZERO);
        assert_eq!(r, 255);
        assert!((g as i32 - 127).abs() <= 1);
    }
}
