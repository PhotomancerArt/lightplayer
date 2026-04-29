//! 2D Simplex noise function.
//!
//! Simplex noise is an improved version of Perlin noise with better quality and performance.
//! This implementation uses Q32 fixed-point arithmetic (16.16 format).
//!
//! Reference: noise-rs library and Stefan Gustavson's Simplex noise implementation
//!
//! LICENSE: Stefan Gustavson and Ian McEwan's Simplex noise implementation is
//! MIT licensed: https://github.com/stegu/webgl-noise
//! This Rust/Q32 port was derived from their algorithm, which LYGIA also distributes
//! under MIT license (not Prosperity License).
//!
//! # GLSL Usage
//!
//! This function is callable from GLSL shaders using the `lpfn_snoise` name:
//!
//! ```glsl
//! float noise = lpfn_snoise(vec2(5.0, 3.0), 123u);
//! ```
//!
//! # Parameters
//!
//! - `p`: Input coordinates as vec2 (converted to Q32 internally, flattened to x, y)
//! - `seed`: Seed value for randomization (uint)
//!
//! # Returns
//!
//! Noise value approximately in range [-1, 1] (float)
//!
//! # Internal Implementation
//!
//! The user-facing `lpfn_snoise` function maps to internal `__lp_lpfn_snoise2` which
//! operates on Q32 fixed-point values. Vector arguments are automatically flattened
//! by the compiler (vec2 becomes two i32 parameters).

use crate::builtins::lpfn::hash::lpfn_hash2;
use lps_q32::q32::Q32;
use lps_q32::vec2_q32::Vec2Q32;

/// Fixed-point constants
const TWO: Q32 = Q32(0x00020000); // 2.0 in Q16.16

/// Skew factor for 2D: (sqrt(3) - 1) / 2
/// sqrt(3) ≈ 1.73205080757, so (1.732 - 1) / 2 ≈ 0.36602540378
/// In Q16.16: 0.36602540378 * 65536 ≈ 23967
const SKEW_FACTOR_2D: Q32 = Q32(23967);

/// Unskew factor for 2D: (3 - sqrt(3)) / 6
/// (3 - 1.732) / 6 ≈ 0.21132486541
/// In Q16.16: 0.21132486541 * 65536 ≈ 13853
const UNSKEW_FACTOR_2D: Q32 = Q32(13853);

/// Gradient LUT for 2D simplex noise (8 gradients).
/// Matches original grad2() ordering: 4 axis-aligned + 4 diagonal (normalized).
/// DIAG = 1/sqrt(2) ≈ 0.70710678118 in Q16.16 = 0xB505 = 46341
const GRAD_LUT_2D: [(i32, i32); 8] = [
    (65536, 0),       // (1, 0)           - index 0
    (-65536, 0),      // (-1, 0)          - index 1
    (0, 65536),       // (0, 1)           - index 2
    (0, -65536),      // (0, -1)          - index 3
    (46341, 46341),   // (1/sqrt(2), ...) - index 4
    (-46341, 46341),  // (-1/sqrt(2), ...) - index 5
    (46341, -46341),  // (1/sqrt(2), ...) - index 6
    (-46341, -46341), // (-1/sqrt(2), ...) - index 7
];

/// 2D Simplex noise function.
///
/// # Arguments
/// * `p` - Input coordinates as Vec2Q32
/// * `seed` - Seed value for randomization
///
/// # Returns
/// Noise value in Q32 fixed-point format, approximately in range [-1, 1]
pub fn lpfn_snoise2(p: Vec2Q32, seed: u32) -> Q32 {
    let x = p.x;
    let y = p.y;

    // Skew the input space to determine which simplex cell we're in
    // skew = (x + y) * SKEW_FACTOR
    let sum = x + y;
    let skew = sum * SKEW_FACTOR_2D;
    let skewed_x = x + skew;
    let skewed_y = y + skew;

    // Get cell coordinates (floor)
    let cell_x_int = skewed_x.to_i32();
    let cell_y_int = skewed_y.to_i32();

    // Convert back to fixed-point for calculations
    let cell_x = Q32::from_i32(cell_x_int);
    let cell_y = Q32::from_i32(cell_y_int);

    // Unskew the cell origin back to (x,y) space
    let cell_sum = cell_x + cell_y;
    let unskew = cell_sum * UNSKEW_FACTOR_2D;
    let unskewed_x = cell_x - unskew;
    let unskewed_y = cell_y - unskew;

    // The x,y distances from the cell origin
    let offset1_x = x - unskewed_x;
    let offset1_y = y - unskewed_y;

    // For the 2D case, the simplex shape is an equilateral triangle.
    // Determine which simplex we are in using branchless step.
    // Branchless comparison: offset1_x > offset1_y ? 1 : 0
    // Uses bit arithmetic to avoid branch misprediction penalty.
    let cmp_raw = (((offset1_x.0 - offset1_y.0) >> 31).wrapping_add(1)) & 1;
    let order_x = Q32(cmp_raw << 16); // 0x10000 or 0
    let order_y = Q32((1 - cmp_raw) << 16); // opposite

    // Offsets for middle corner in (x,y) unskewed coords
    let offset2_x = offset1_x - order_x + UNSKEW_FACTOR_2D;
    let offset2_y = offset1_y - order_y + UNSKEW_FACTOR_2D;

    // Offsets for last corner in (x,y) unskewed coords
    let offset3_x = offset1_x - Q32::ONE + (TWO * UNSKEW_FACTOR_2D);
    let offset3_y = offset1_y - Q32::ONE + (TWO * UNSKEW_FACTOR_2D);

    // Calculate gradient indexes for each corner
    let gi0 = lpfn_hash2(cell_x_int as u32, cell_y_int as u32, seed);
    let gi1 = lpfn_hash2(
        (cell_x_int + order_x.to_i32()) as u32,
        (cell_y_int + order_y.to_i32()) as u32,
        seed,
    );
    let gi2 = lpfn_hash2((cell_x_int + 1) as u32, (cell_y_int + 1) as u32, seed);

    // Calculate contribution from each corner
    let corner0 = surflet_2d(gi0 as usize, offset1_x, offset1_y);
    let corner1 = surflet_2d(gi1 as usize, offset2_x, offset2_y);
    let corner2 = surflet_2d(gi2 as usize, offset3_x, offset3_y);

    // Add contributions from each corner
    // Result is already approximately in [-1, 1] range due to algorithm
    corner0 + corner1 + corner2
}

/// 2D Simplex noise function (extern C wrapper for compiler).
///
/// # Arguments
/// * `x` - X coordinate as i32 (Q32 fixed-point)
/// * `y` - Y coordinate as i32 (Q32 fixed-point)
/// * `seed` - Seed value for randomization
///
/// # Returns
/// Noise value as i32 (Q32 fixed-point format), approximately in range [-1, 1]
#[lpfn_impl_macro::lpfn_impl(q32, "float lpfn_snoise(vec2 p, uint seed)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpfn_snoise2_q32(x: i32, y: i32, seed: u32) -> i32 {
    let p = Vec2Q32::new(Q32::from_fixed(x), Q32::from_fixed(y));
    lpfn_snoise2(p, seed).to_fixed()
}

/// Get 2D gradient vector from gradient index using const LUT.
/// Returns (gx, gy) in Q32 fixed-point format.
///
/// Range: gradients are in [-1, 1] for each component.
#[inline(always)]
fn grad2(index: usize) -> (Q32, Q32) {
    let (gx, gy) = GRAD_LUT_2D[index & 7]; // 8 gradients in 2D simplex
    (Q32::from_fixed(gx), Q32::from_fixed(gy))
}

/// Compute surflet contribution for a corner using wrapping math where safe.
///
/// # Range Analysis for Wrapping Operations
///
/// - x, y are offset distances from simplex corners, bounded by simplex geometry (~[-1, 1]).
/// - x*x, y*y are bounded by ~1.0, so mul_wrapping is safe (result < 1.0).
/// - dist^2 = x^2 + y^2 is bounded by ~2.0, dist^2 * 2 is bounded by ~4.0.
/// - t = 1.0 - dist^2 * 2 is bounded but subtraction uses saturating for safety.
/// - t^2, t^4: t is bounded, so mul_wrapping is safe.
/// - Gradient components are in [-1, 1], dot product is bounded.
fn surflet_2d(gradient_index: usize, x: Q32, y: Q32) -> Q32 {
    // t = 1.0 - dist^2 * 2.0
    // x^2 and y^2 are bounded (~[-1,1] squared), so mul_wrapping is safe.
    let x2 = x.mul_wrapping(x);
    let y2 = y.mul_wrapping(y);
    let dist_sq = x2.add_wrapping(y2);
    let dist_sq_times_2 = dist_sq.mul_wrapping(TWO);
    let t = Q32::ONE - dist_sq_times_2; // saturating for the 1.0 - x operation

    if t > Q32::ZERO {
        // Get gradient from LUT
        let (gx, gy) = grad2(gradient_index);

        // Compute dot product: gradient · offset
        // Both gradient and offset are bounded, so mul_wrapping is safe.
        let dot = gx.mul_wrapping(x).add_wrapping(gy.mul_wrapping(y));

        // Apply falloff: (2.0 * t^2 + t^4) * dot
        // t is bounded, so mul_wrapping is safe.
        let t2 = t.mul_wrapping(t);
        let t4 = t2.mul_wrapping(t2);
        let falloff = t2.add_wrapping(t2).add_wrapping(t4); // 2*t^2 + t^4 using wrapping

        dot.mul_wrapping(falloff)
    } else {
        Q32::ZERO
    }
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    extern crate std;
    use super::*;
    use crate::util::test_helpers::{fixed_to_float, float_to_fixed};

    #[test]
    fn test_simplex2_basic() {
        let result1 = __lp_lpfn_snoise2_q32(float_to_fixed(1.5), float_to_fixed(2.3), 0);
        let result2 = __lp_lpfn_snoise2_q32(float_to_fixed(3.7), float_to_fixed(2.3), 0);
        let result3 = __lp_lpfn_snoise2_q32(float_to_fixed(1.5), float_to_fixed(2.3), 1);

        // Different inputs should produce different outputs
        assert_ne!(
            result1, result2,
            "Noise should differ for different x values"
        );
        assert_ne!(result1, result3, "Noise should differ for different seeds");
    }

    #[test]
    fn test_simplex2_range() {
        // Test that output is approximately in [-1, 1] range
        for i in 0..50 {
            let x = float_to_fixed(i as f32 * 0.1);
            let y = float_to_fixed(i as f32 * 0.15);
            let result = __lp_lpfn_snoise2_q32(x, y, 0);
            let result_float = fixed_to_float(result);

            assert!(
                result_float >= -2.0 && result_float <= 2.0,
                "Noise value {} should be in approximate range [-1, 1], got {}",
                i,
                result_float
            );
        }
    }

    #[test]
    fn test_simplex2_deterministic() {
        let result1 = __lp_lpfn_snoise2_q32(float_to_fixed(42.5), float_to_fixed(37.3), 123);
        let result2 = __lp_lpfn_snoise2_q32(float_to_fixed(42.5), float_to_fixed(37.3), 123);

        // Same input and seed should produce same output
        assert_eq!(result1, result2, "Noise should be deterministic");
    }
}
