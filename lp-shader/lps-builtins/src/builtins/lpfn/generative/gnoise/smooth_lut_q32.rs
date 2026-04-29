//! Smoothing function LUTs for gradient noise.
//!
//! Precomputed smoothstep functions at 256-entry resolution for fast LUT-based
//! interpolation instead of polynomial evaluation.
//!
//! LUT size: 256 entries × 4 bytes = 1KB per LUT (2KB total)

use lps_q32::q32::Q32;
use lps_q32::vec2_q32::Vec2Q32;
use lps_q32::vec3_q32::Vec3Q32;

/// Cubic smoothstep LUT: 3t² - 2t³ for t in [0, 1]
///
/// 256 entries indexed by top 8 bits of fractional part.
/// Entry i corresponds to t = i/256.
///
/// Generated at compile time using const evaluation.
pub const CUBIC_SMOOTHSTEP_LUT: [i32; 256] = {
    let mut lut = [0i32; 256];
    let mut i = 0;
    while i < 256 {
        // t in Q16.16: i/256 = i << 8 (since 1.0 = 65536 = 256 << 8)
        let t = (i as i64) << 8;
        // t^2 in Q32.32, shift to Q16.16
        let t2 = (t * t) >> 16;
        // t^3 = t^2 * t, shift to Q16.16
        let t3 = (t2 * t) >> 16;
        // 3t^2 - 2t^3 in Q16.16
        let val = 3 * t2 - 2 * t3;
        // val is already in Q16.16 format, just clamp and store
        lut[i] = if val > 65535 { 65535 } else { val as i32 };
        i += 1;
    }
    lut
};

/// Quintic smoothstep LUT: 6t⁵ - 15t⁴ + 10t³ for t in [0, 1]
///
/// 256 entries indexed by top 8 bits of fractional part.
/// Entry i corresponds to t = i/256.
///
/// Generated at compile time using const evaluation.
pub const QUINTIC_SMOOTHSTEP_LUT: [i32; 256] = {
    let mut lut = [0i32; 256];
    let mut i = 0;
    while i < 256 {
        // t in Q16.16: i/256 = i << 8
        let t = (i as i64) << 8;
        // t^2 in Q16.16
        let t2 = (t * t) >> 16;
        // t^3 in Q16.16
        let t3 = (t2 * t) >> 16;
        // t^4 in Q16.16
        let t4 = (t3 * t) >> 16;
        // t^5 in Q16.16
        let t5 = (t4 * t) >> 16;
        // 6t^5 - 15t^4 + 10t^3 in Q16.16 (all terms are Q16.16)
        let val = 6 * t5 - 15 * t4 + 10 * t3;
        // val is already in Q16.16 format, just clamp and store
        lut[i] = if val > 65535 {
            65535
        } else if val < 0 {
            0
        } else {
            val as i32
        };
        i += 1;
    }
    lut
};

/// Cubic smoothstep using LUT lookup.
///
/// Index: top 8 bits of fractional part = (t.0 >> 8) & 0xFF
/// Handles edge case where t >= 1.0 by returning the last LUT entry.
#[inline(always)]
pub fn cubic_smoothstep_lut(t: Q32) -> Q32 {
    // When t >= 1.0 (t.0 >= 65536), we want the last LUT entry
    // Clamp t.0 to [0, 65535] before shifting to get correct indexing
    let clamped = if t.0 >= 65536 {
        65535
    } else if t.0 < 0 {
        0
    } else {
        t.0
    };
    let index = ((clamped >> 8) & 0xFF) as usize;
    Q32::from_fixed(CUBIC_SMOOTHSTEP_LUT[index])
}

/// Quintic smoothstep using LUT lookup.
///
/// Index: top 8 bits of fractional part = (t.0 >> 8) & 0xFF
/// Handles edge case where t >= 1.0 by returning the last LUT entry.
#[inline(always)]
pub fn quintic_smoothstep_lut(t: Q32) -> Q32 {
    // When t >= 1.0 (t.0 >= 65536), we want the last LUT entry
    // Clamp t.0 to [0, 65535] before shifting to get correct indexing
    let clamped = if t.0 >= 65536 {
        65535
    } else if t.0 < 0 {
        0
    } else {
        t.0
    };
    let index = ((clamped >> 8) & 0xFF) as usize;
    Q32::from_fixed(QUINTIC_SMOOTHSTEP_LUT[index])
}

/// Component-wise cubic smoothing using LUT for Vec2Q32
#[inline(always)]
pub fn cubic_vec2_lut(v: Vec2Q32) -> Vec2Q32 {
    Vec2Q32::new(cubic_smoothstep_lut(v.x), cubic_smoothstep_lut(v.y))
}

/// Component-wise quintic smoothing using LUT for Vec3Q32
#[inline(always)]
pub fn quintic_vec3_lut(v: Vec3Q32) -> Vec3Q32 {
    Vec3Q32::new(
        quintic_smoothstep_lut(v.x),
        quintic_smoothstep_lut(v.y),
        quintic_smoothstep_lut(v.z),
    )
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    extern crate std;
    use super::*;

    #[test]
    fn test_cubic_lut_bounds() {
        // Test endpoints
        let t0 = Q32::from_f32_wrapping(0.0);
        let t1 = Q32::from_f32_wrapping(1.0);

        assert!((cubic_smoothstep_lut(t0).to_f32() - 0.0).abs() < 0.01);
        assert!((cubic_smoothstep_lut(t1).to_f32() - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_cubic_lut_midpoint() {
        // cubic(0.5) = 3*(0.5)^2 - 2*(0.5)^3 = 0.75 - 0.25 = 0.5
        let t = Q32::from_f32_wrapping(0.5);
        let result = cubic_smoothstep_lut(t).to_f32();
        // Allow for LUT quantization error (~0.004)
        assert!(
            (result - 0.5).abs() < 0.01,
            "cubic(0.5) should be ~0.5, got {}",
            result
        );
    }

    #[test]
    fn test_quintic_lut_bounds() {
        let t0 = Q32::from_f32_wrapping(0.0);
        let t1 = Q32::from_f32_wrapping(1.0);

        assert!((quintic_smoothstep_lut(t0).to_f32() - 0.0).abs() < 0.01);
        assert!((quintic_smoothstep_lut(t1).to_f32() - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_quintic_lut_midpoint() {
        // quintic(0.5) = 6*(0.5)^5 - 15*(0.5)^4 + 10*(0.5)^3
        //              = 6/32 - 15/16 + 10/8
        //              = 0.1875 - 0.9375 + 1.25 = 0.5
        let t = Q32::from_f32_wrapping(0.5);
        let result = quintic_smoothstep_lut(t).to_f32();
        // Allow for LUT quantization error
        assert!(
            (result - 0.5).abs() < 0.01,
            "quintic(0.5) should be ~0.5, got {}",
            result
        );
    }

    #[test]
    fn test_cubic_vec2_lut() {
        let v = Vec2Q32::from_f32(0.5, 0.25);
        let result = cubic_vec2_lut(v);
        assert!(result.x.to_f32() >= 0.0 && result.x.to_f32() <= 1.0);
        assert!(result.y.to_f32() >= 0.0 && result.y.to_f32() <= 1.0);
    }

    #[test]
    fn test_quintic_vec3_lut() {
        let v = Vec3Q32::from_f32(0.5, 0.25, 0.75);
        let result = quintic_vec3_lut(v);
        assert!(result.x.to_f32() >= 0.0 && result.x.to_f32() <= 1.0);
        assert!(result.y.to_f32() >= 0.0 && result.y.to_f32() <= 1.0);
        assert!(result.z.to_f32() >= 0.0 && result.z.to_f32() <= 1.0);
    }
}
