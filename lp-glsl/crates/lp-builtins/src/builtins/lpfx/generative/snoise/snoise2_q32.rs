//! 2D Simplex noise function.
//!
//! Simplex noise is an improved version of Perlin noise with better quality and performance.
//! This implementation uses Q32 fixed-point arithmetic (16.16 format).
//!
//! Reference: noise-rs library and Stefan Gustavson's Simplex noise implementation
//!
//! # GLSL Usage
//!
//! This function is callable from GLSL shaders using the `lpfx_snoise` name:
//!
//! ```glsl
//! float noise = lpfx_snoise(vec2(5.0, 3.0), 123u);
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
//! The user-facing `lpfx_snoise` function maps to internal `__lpfx_snoise2` which
//! operates on Q32 fixed-point values. Vector arguments are automatically flattened
//! by the compiler (vec2 becomes two i32 parameters).

use crate::builtins::lpfx::hash::lpfx_hash2;
use crate::glsl::q32::types::q32::Q32;
use crate::glsl::q32::types::vec2_q32::Vec2Q32;

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

/// 2D Simplex noise function.
///
/// # Arguments
/// * `p` - Input coordinates as Vec2Q32
/// * `seed` - Seed value for randomization
///
/// # Returns
/// Noise value in Q32 fixed-point format, approximately in range [-1, 1]
pub fn lpfx_snoise2(p: Vec2Q32, seed: u32) -> Q32 {
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
    // Determine which simplex we are in.
    let (order_x, order_y) = if offset1_x > offset1_y {
        // Lower triangle, XY order: (0,0)->(1,0)->(1,1)
        (Q32::ONE, Q32::ZERO)
    } else {
        // Upper triangle, YX order: (0,0)->(0,1)->(1,1)
        (Q32::ZERO, Q32::ONE)
    };

    // Offsets for middle corner in (x,y) unskewed coords
    let offset2_x = offset1_x - order_x + UNSKEW_FACTOR_2D;
    let offset2_y = offset1_y - order_y + UNSKEW_FACTOR_2D;

    // Offsets for last corner in (x,y) unskewed coords
    let offset3_x = offset1_x - Q32::ONE + (TWO * UNSKEW_FACTOR_2D);
    let offset3_y = offset1_y - Q32::ONE + (TWO * UNSKEW_FACTOR_2D);

    // Calculate gradient indexes for each corner
    let gi0 = lpfx_hash2(cell_x_int as u32, cell_y_int as u32, seed);
    let gi1 = lpfx_hash2(
        (cell_x_int + order_x.to_i32()) as u32,
        (cell_y_int + order_y.to_i32()) as u32,
        seed,
    );
    let gi2 = lpfx_hash2((cell_x_int + 1) as u32, (cell_y_int + 1) as u32, seed);

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
#[lpfx_impl_macro::lpfx_impl(q32, "float lpfx_snoise(vec2 p, uint seed)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lpfx_snoise2_q32(x: i32, y: i32, seed: u32) -> i32 {
    let p = Vec2Q32::new(Q32::from_fixed(x), Q32::from_fixed(y));
    lpfx_snoise2(p, seed).to_fixed()
}

/// Compute magnitude squared of a 2D vector
#[inline(always)]
fn magnitude_squared_2d(x: Q32, y: Q32) -> Q32 {
    x * x + y * y
}

/// Compute dot product of two 2D vectors
#[inline(always)]
fn dot_2d(x1: Q32, y1: Q32, x2: Q32, y2: Q32) -> Q32 {
    x1 * x2 + y1 * y2
}

/// Get 2D gradient vector from gradient index
/// Returns (gx, gy) in Q32 fixed-point format
fn grad2(index: usize) -> (Q32, Q32) {
    // Gradients are combinations of -1, 0, and 1, normalized
    // For 2D, we use 8 gradients
    const DIAG: Q32 = Q32(0xB505); // 1/sqrt(2) ≈ 0.70710678118 in Q16.16

    match index % 8 {
        0 => (Q32::ONE, Q32::ZERO),  // (1, 0)
        1 => (-Q32::ONE, Q32::ZERO), // (-1, 0)
        2 => (Q32::ZERO, Q32::ONE),  // (0, 1)
        3 => (Q32::ZERO, -Q32::ONE), // (0, -1)
        4 => (DIAG, DIAG),           // (1/sqrt(2), 1/sqrt(2))
        5 => (-DIAG, DIAG),          // (-1/sqrt(2), 1/sqrt(2))
        6 => (DIAG, -DIAG),          // (1/sqrt(2), -1/sqrt(2))
        7 => (-DIAG, -DIAG),         // (-1/sqrt(2), -1/sqrt(2))
        _ => (Q32::ONE, Q32::ZERO),  // Should never happen
    }
}

/// Compute surflet contribution for a corner
fn surflet_2d(gradient_index: usize, x: Q32, y: Q32) -> Q32 {
    // t = 1.0 - dist^2 * 2.0
    let dist_sq = magnitude_squared_2d(x, y);
    let dist_sq_times_2 = dist_sq * TWO;
    let t = Q32::ONE - dist_sq_times_2;

    if t > Q32::ZERO {
        // Get gradient
        let (gx, gy) = grad2(gradient_index);

        // Compute dot product: gradient · offset
        let dot = dot_2d(gx, gy, x, y);

        // Apply falloff: (2.0 * t^2 + t^4) * dot
        let t2 = t * t;
        let t4 = t2 * t2;
        let falloff = TWO * t2 + t4;

        dot * falloff
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
        let result1 = __lpfx_snoise2_q32(float_to_fixed(1.5), float_to_fixed(2.3), 0);
        let result2 = __lpfx_snoise2_q32(float_to_fixed(3.7), float_to_fixed(2.3), 0);
        let result3 = __lpfx_snoise2_q32(float_to_fixed(1.5), float_to_fixed(2.3), 1);

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
            let result = __lpfx_snoise2_q32(x, y, 0);
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
        let result1 = __lpfx_snoise2_q32(float_to_fixed(42.5), float_to_fixed(37.3), 123);
        let result2 = __lpfx_snoise2_q32(float_to_fixed(42.5), float_to_fixed(37.3), 123);

        // Same input and seed should produce same output
        assert_eq!(result1, result2, "Noise should be deterministic");
    }
}
