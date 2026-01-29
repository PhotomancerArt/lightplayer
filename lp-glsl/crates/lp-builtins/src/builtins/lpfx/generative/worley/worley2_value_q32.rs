//! 2D Worley noise function (value variant).
//!
//! Worley noise (cellular noise) generates cellular patterns based on the distance
//! to the nearest feature point in a grid. This variant returns a hash value based
//! on the nearest cell's coordinates. This implementation uses Q32 fixed-point
//! arithmetic (16.16 format).
//!
//! Reference: noise-rs library (https://github.com/Razaekel/noise-rs)
//!
//! # GLSL Usage
//!
//! This function is callable from GLSL shaders using the `lpfx_worley_value` name:
//!
//! ```glsl
//! float noise = lpfx_worley_value(vec2(5.0, 3.0), 123u);
//! ```
//!
//! # Parameters
//!
//! - `p`: Input coordinates as vec2 (converted to Q32 internally, flattened to x, y)
//! - `seed`: Seed value for randomization (uint)
//!
//! # Returns
//!
//! Hash value of nearest cell, approximately in range [-1, 1] (float)

use crate::builtins::lpfx::hash::lpfx_hash2;
use crate::glsl::q32::types::q32::Q32;
use crate::glsl::q32::types::vec2_q32::Vec2Q32;

/// Fixed-point constants
const HALF: Q32 = Q32(0x00008000); // 0.5 in Q16.16
const TWO: Q32 = Q32(0x00020000); // 2.0 in Q16.16

/// 1/sqrt(2) ≈ 0.70710678118 in Q16.16
const FRAC_1_SQRT_2: Q32 = Q32(0xB505);

/// Maximum hash value for normalization (u32::MAX would be too large, use 255 like reference)
const MAX_HASH: Q32 = Q32(255 << 16); // 255.0 in Q16.16

/// 2D Worley noise function (value variant).
///
/// # Arguments
/// * `p` - Input coordinates as Vec2Q32
/// * `seed` - Seed value for randomization
///
/// # Returns
/// Hash value of nearest cell in Q32 fixed-point format, approximately in range [-1, 1]
pub fn lpfx_worley2_value(p: Vec2Q32, seed: u32) -> Q32 {
    let x = p.x;
    let y = p.y;

    // Get cell coordinates (floor)
    let cell_x_int = x.to_i32();
    let cell_y_int = y.to_i32();

    // Convert back to fixed-point for calculations
    let cell_x = Q32::from_i32(cell_x_int);
    let cell_y = Q32::from_i32(cell_y_int);

    // Calculate fractional coordinates
    let frac_x = x - cell_x;
    let frac_y = y - cell_y;

    // Determine near/far cells based on fractional > 0.5
    let near_x_int = if frac_x > HALF {
        cell_x_int + 1
    } else {
        cell_x_int
    };
    let near_y_int = if frac_y > HALF {
        cell_y_int + 1
    } else {
        cell_y_int
    };
    let far_x_int = if frac_x > HALF {
        cell_x_int
    } else {
        cell_x_int + 1
    };
    let far_y_int = if frac_y > HALF {
        cell_y_int
    } else {
        cell_y_int + 1
    };

    // Generate feature point for near cell using hash
    let seed_index = lpfx_hash2(near_x_int as u32, near_y_int as u32, seed);
    let seed_point = get_point_2d(seed_index as usize, near_x_int, near_y_int);

    // Calculate initial distance (euclidean squared)
    let dx = x - seed_point.0;
    let dy = y - seed_point.1;
    let mut distance = dx * dx + dy * dy;

    // Track which cell contains the nearest point
    let mut seed_cell_x = near_x_int;
    let mut seed_cell_y = near_y_int;

    // Calculate range for optimization: (0.5 - frac)^2
    let range_x = (HALF - frac_x) * (HALF - frac_x);
    let range_y = (HALF - frac_y) * (HALF - frac_y);

    // Check adjacent cells only if within distance range
    if range_x < distance {
        let test_x_int = far_x_int;
        let test_y_int = near_y_int;
        let test_index = lpfx_hash2(test_x_int as u32, test_y_int as u32, seed);
        let test_point = get_point_2d(test_index as usize, test_x_int, test_y_int);
        let test_dx = x - test_point.0;
        let test_dy = y - test_point.1;
        let test_distance = test_dx * test_dx + test_dy * test_dy;
        if test_distance < distance {
            distance = test_distance;
            seed_cell_x = test_x_int;
            seed_cell_y = test_y_int;
        }
    }

    if range_y < distance {
        let test_x_int = near_x_int;
        let test_y_int = far_y_int;
        let test_index = lpfx_hash2(test_x_int as u32, test_y_int as u32, seed);
        let test_point = get_point_2d(test_index as usize, test_x_int, test_y_int);
        let test_dx = x - test_point.0;
        let test_dy = y - test_point.1;
        let test_distance = test_dx * test_dx + test_dy * test_dy;
        if test_distance < distance {
            distance = test_distance;
            seed_cell_x = test_x_int;
            seed_cell_y = test_y_int;
        }
    }

    if range_x < distance && range_y < distance {
        let test_x_int = far_x_int;
        let test_y_int = far_y_int;
        let test_index = lpfx_hash2(test_x_int as u32, test_y_int as u32, seed);
        let test_point = get_point_2d(test_index as usize, test_x_int, test_y_int);
        let test_dx = x - test_point.0;
        let test_dy = y - test_point.1;
        let test_distance = test_dx * test_dx + test_dy * test_dy;
        #[allow(unused_assignments)]
        if test_distance < distance {
            distance = test_distance;
            seed_cell_x = test_x_int;
            seed_cell_y = test_y_int;
        }
    }

    // Hash the seed_cell coordinates
    let hash_value = lpfx_hash2(seed_cell_x as u32, seed_cell_y as u32, seed);

    // Normalize hash to [0, 1] range: hash_value / 255.0
    let normalized = Q32::from_i32((hash_value & 0xFF) as i32) / MAX_HASH;

    // Scale to [-1, 1] range: value * 2.0 - 1.0

    normalized * TWO - Q32::ONE
}

/// 2D Worley noise function value variant (extern C wrapper for compiler).
///
/// # Arguments
/// * `x` - X coordinate as i32 (Q32 fixed-point)
/// * `y` - Y coordinate as i32 (Q32 fixed-point)
/// * `seed` - Seed value for randomization
///
/// # Returns
/// Hash value as i32 (Q32 fixed-point format), approximately in range [-1, 1]
#[lpfx_impl_macro::lpfx_impl(q32, "float lpfx_worley_value(vec2 p, uint seed)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lpfx_worley2_value_q32(x: i32, y: i32, seed: u32) -> i32 {
    let p = Vec2Q32::new(Q32::from_fixed(x), Q32::from_fixed(y));
    lpfx_worley2_value(p, seed).to_fixed()
}

/// Get feature point offset from hash index and cell coordinates
/// Returns (offset_x, offset_y) in Q32 fixed-point format
fn get_point_2d(index: usize, cell_x: i32, cell_y: i32) -> (Q32, Q32) {
    // Length ranges from 0 to 0.5, based on upper 5 bits of index
    // length = ((index & 0xF8) >> 3) * 0.5 / 31.0
    let length_bits = (index & 0xF8) >> 3;
    let length = Q32::from_i32(length_bits as i32) * HALF / Q32::from_i32(31);

    // Diagonal length
    let diag = length * FRAC_1_SQRT_2;

    // Cell origin in Q32
    let cell_x_q32 = Q32::from_i32(cell_x);
    let cell_y_q32 = Q32::from_i32(cell_y);

    // Get direction from lower 3 bits
    let (offset_x, offset_y) = match index & 0x07 {
        0 => (diag, diag),
        1 => (diag, -diag),
        2 => (-diag, diag),
        3 => (-diag, -diag),
        4 => (length, Q32::ZERO),
        5 => (-length, Q32::ZERO),
        6 => (Q32::ZERO, length),
        7 => (Q32::ZERO, -length),
        _ => unreachable!(),
    };

    // Return feature point = cell origin + offset
    (cell_x_q32 + offset_x, cell_y_q32 + offset_y)
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    extern crate std;
    use super::*;
    use crate::util::test_helpers::{fixed_to_float, float_to_fixed};

    #[test]
    fn test_worley2_value_basic() {
        let result1 = __lpfx_worley2_value_q32(float_to_fixed(1.5), float_to_fixed(2.3), 0);
        let result2 = __lpfx_worley2_value_q32(float_to_fixed(3.7), float_to_fixed(2.3), 0);
        let result3 = __lpfx_worley2_value_q32(float_to_fixed(1.5), float_to_fixed(2.3), 1);

        // Different inputs should produce different outputs
        assert_ne!(
            result1, result2,
            "Noise should differ for different x values"
        );
        assert_ne!(result1, result3, "Noise should differ for different seeds");
    }

    #[test]
    fn test_worley2_value_range() {
        // Test that output is approximately in [-1, 1] range
        for i in 0..50 {
            let x = float_to_fixed(i as f32 * 0.1);
            let y = float_to_fixed(i as f32 * 0.15);
            let result = __lpfx_worley2_value_q32(x, y, 0);
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
    fn test_worley2_value_deterministic() {
        let result1 = __lpfx_worley2_value_q32(float_to_fixed(42.5), float_to_fixed(37.3), 123);
        let result2 = __lpfx_worley2_value_q32(float_to_fixed(42.5), float_to_fixed(37.3), 123);

        // Same input and seed should produce same output
        assert_eq!(result1, result2, "Noise should be deterministic");
    }

    #[test]
    fn test_worley2_value_different_from_distance() {
        use crate::builtins::lpfx::generative::worley::worley2_q32::__lpfx_worley2_q32;

        let x = float_to_fixed(5.5);
        let y = float_to_fixed(3.3);
        let seed = 42;

        let distance_result = __lpfx_worley2_q32(x, y, seed);
        let value_result = __lpfx_worley2_value_q32(x, y, seed);

        // Value and distance should produce different outputs
        assert_ne!(
            distance_result, value_result,
            "Distance and value variants should produce different outputs"
        );
    }
}
