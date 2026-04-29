//! 3D Worley noise function (distance variant).
//!
//! Worley noise (cellular noise) generates cellular patterns based on the distance
//! to the nearest feature point in a grid. This implementation uses Q32 fixed-point
//! arithmetic (16.16 format) and returns the euclidean squared distance.
//!
//! Reference: noise-rs library (https://github.com/Razaekel/noise-rs)
//!
//! # GLSL Usage
//!
//! This function is callable from GLSL shaders using the `lpfn_worley` name:
//!
//! ```glsl
//! float noise = lpfn_worley(vec3(5.0, 3.0, 1.0), 123u);
//! ```
//!
//! # Parameters
//!
//! - `p`: Input coordinates as vec3 (converted to Q32 internally, flattened to x, y, z)
//! - `seed`: Seed value for randomization (uint)
//!
//! # Returns
//!
//! Euclidean squared distance to nearest feature point, approximately in range [-1, 1] (float)
//!
//! # Implementation Notes
//!
//! ## Range Analysis for Wrapping Math
//!
//! The Worley algorithm checks neighboring cells (27 cells in 3D) to find the nearest
//! feature point. Key bounds:
//!
//! - Cell size: 1.0 (Q16.16: 65536)
//! - Query point: within [cell_coord, cell_coord+1]
//! - Feature point: within [cell_coord, cell_coord+1] of its cell
//! - Maximum difference between query and feature point: ~[-2, 2] cells
//!   (current cell boundary + neighbor cell boundary)
//!
//! Therefore:
//! - `dx`, `dy`, `dz` are bounded by approximately [-2, 2]
//! - `dx*dx`, `dy*dy`, `dz*dz` are bounded by approximately 4.0
//! - `dist_sq = dx*dx + dy*dy + dz*dz` is bounded by approximately 12.0
//! - Range calculations `(0.5 - frac)^2` are bounded by 0.25
//!
//! These bounds are safely within Q32's i32 representation range, so wrapping math
//! can be used for distance calculations without overflow concerns.
//!
//! ## Wrapping Math Application
//!
//! Operations using wrapping math (`mul_wrapping`, `add_wrapping`):
//! - Distance squared: `dx.mul_wrapping(dx)` etc. (bounded ~4.0)
//! - Range calculations: `(HALF - frac_x).mul_wrapping(HALF - frac_x)` (bounded ~0.25)
//! - Feature offset: `length.mul_wrapping(FRAC_1_SQRT_2)` (length bounded by 0.5)
//!
//! Operations keeping saturating math (correctness critical):
//! - Min-distance comparisons: `test_distance < distance` (needs correct ordering)
//! - Range comparisons: `range_x < distance` (needs correct ordering)
//! - Final scaling: division and output range mapping

use crate::builtins::lpfn::hash::lpfn_hash3;
use lps_q32::q32::Q32;
use lps_q32::vec3_q32::Vec3Q32;

/// Fixed-point constants
const HALF: Q32 = Q32(0x00008000); // 0.5 in Q16.16
const TWO: Q32 = Q32(0x00020000); // 2.0 in Q16.16

/// 1/sqrt(2) ≈ 0.70710678118 in Q16.16
const FRAC_1_SQRT_2: Q32 = Q32(0xB505);

/// 3D Worley noise function (distance variant).
///
/// # Arguments
/// * `p` - Input coordinates as Vec3Q32
/// * `seed` - Seed value for randomization
///
/// # Returns
/// Euclidean squared distance to nearest feature point in Q32 fixed-point format,
/// approximately in range [-1, 1]
pub fn lpfn_worley3(p: Vec3Q32, seed: u32) -> Q32 {
    let x = p.x;
    let y = p.y;
    let z = p.z;

    // Get cell coordinates (floor)
    let cell_x_int = x.to_i32();
    let cell_y_int = y.to_i32();
    let cell_z_int = z.to_i32();

    // Convert back to fixed-point for calculations
    let cell_x = Q32::from_i32(cell_x_int);
    let cell_y = Q32::from_i32(cell_y_int);
    let cell_z = Q32::from_i32(cell_z_int);

    // Calculate fractional coordinates
    let frac_x = x - cell_x;
    let frac_y = y - cell_y;
    let frac_z = z - cell_z;

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
    let near_z_int = if frac_z > HALF {
        cell_z_int + 1
    } else {
        cell_z_int
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
    let far_z_int = if frac_z > HALF {
        cell_z_int
    } else {
        cell_z_int + 1
    };

    // Generate feature point for near cell using hash
    let seed_index = lpfn_hash3(
        near_x_int as u32,
        near_y_int as u32,
        near_z_int as u32,
        seed,
    );
    let seed_point = get_point_3d(seed_index as usize, near_x_int, near_y_int, near_z_int);

    // Calculate initial distance (euclidean squared)
    // Range analysis: dx, dy, dz bounded by ~[-2, 2], so squared terms bounded by ~4.0
    // Using wrapping math as bounds are provably within safe range
    let dx = x - seed_point.0;
    let dy = y - seed_point.1;
    let dz = z - seed_point.2;
    let mut distance = dx
        .mul_wrapping(dx)
        .add_wrapping(dy.mul_wrapping(dy))
        .add_wrapping(dz.mul_wrapping(dz));

    // Calculate range for optimization: (0.5 - frac)^2
    // Range analysis: (HALF - frac) bounded by ~[-0.5, 0.5], squared bounded by ~0.25
    // Using wrapping math as bounds are provably within safe range
    let range_x = (HALF - frac_x).mul_wrapping(HALF - frac_x);
    let range_y = (HALF - frac_y).mul_wrapping(HALF - frac_y);
    let range_z = (HALF - frac_z).mul_wrapping(HALF - frac_z);

    // Check adjacent cells only if within distance range
    // Single-axis checks
    // Comparison uses saturating math - correctness critical for min tracking
    if range_x < distance {
        let test_x_int = far_x_int;
        let test_y_int = near_y_int;
        let test_z_int = near_z_int;
        let test_index = lpfn_hash3(
            test_x_int as u32,
            test_y_int as u32,
            test_z_int as u32,
            seed,
        );
        let test_point = get_point_3d(test_index as usize, test_x_int, test_y_int, test_z_int);
        // Range analysis: test_dx, test_dy, test_dz bounded by ~[-2, 2] (same as primary distance)
        let test_dx = x - test_point.0;
        let test_dy = y - test_point.1;
        let test_dz = z - test_point.2;
        let test_distance = test_dx
            .mul_wrapping(test_dx)
            .add_wrapping(test_dy.mul_wrapping(test_dy))
            .add_wrapping(test_dz.mul_wrapping(test_dz));
        // Comparison uses saturating math - correctness critical for min tracking
        if test_distance < distance {
            distance = test_distance;
        }
    }

    if range_y < distance {
        let test_x_int = near_x_int;
        let test_y_int = far_y_int;
        let test_z_int = near_z_int;
        let test_index = lpfn_hash3(
            test_x_int as u32,
            test_y_int as u32,
            test_z_int as u32,
            seed,
        );
        let test_point = get_point_3d(test_index as usize, test_x_int, test_y_int, test_z_int);
        // Range analysis: test_dx, test_dy, test_dz bounded by ~[-2, 2] (same as primary distance)
        let test_dx = x - test_point.0;
        let test_dy = y - test_point.1;
        let test_dz = z - test_point.2;
        let test_distance = test_dx
            .mul_wrapping(test_dx)
            .add_wrapping(test_dy.mul_wrapping(test_dy))
            .add_wrapping(test_dz.mul_wrapping(test_dz));
        // Comparison uses saturating math - correctness critical for min tracking
        if test_distance < distance {
            distance = test_distance;
        }
    }

    if range_z < distance {
        let test_x_int = near_x_int;
        let test_y_int = near_y_int;
        let test_z_int = far_z_int;
        let test_index = lpfn_hash3(
            test_x_int as u32,
            test_y_int as u32,
            test_z_int as u32,
            seed,
        );
        let test_point = get_point_3d(test_index as usize, test_x_int, test_y_int, test_z_int);
        // Range analysis: test_dx, test_dy, test_dz bounded by ~[-2, 2] (same as primary distance)
        let test_dx = x - test_point.0;
        let test_dy = y - test_point.1;
        let test_dz = z - test_point.2;
        let test_distance = test_dx
            .mul_wrapping(test_dx)
            .add_wrapping(test_dy.mul_wrapping(test_dy))
            .add_wrapping(test_dz.mul_wrapping(test_dz));
        // Comparison uses saturating math - correctness critical for min tracking
        if test_distance < distance {
            distance = test_distance;
        }
    }

    // Two-axis checks
    if range_x < distance && range_y < distance {
        let test_x_int = far_x_int;
        let test_y_int = far_y_int;
        let test_z_int = near_z_int;
        let test_index = lpfn_hash3(
            test_x_int as u32,
            test_y_int as u32,
            test_z_int as u32,
            seed,
        );
        let test_point = get_point_3d(test_index as usize, test_x_int, test_y_int, test_z_int);
        // Range analysis: test_dx, test_dy, test_dz bounded by ~[-2, 2] (same as primary distance)
        let test_dx = x - test_point.0;
        let test_dy = y - test_point.1;
        let test_dz = z - test_point.2;
        let test_distance = test_dx
            .mul_wrapping(test_dx)
            .add_wrapping(test_dy.mul_wrapping(test_dy))
            .add_wrapping(test_dz.mul_wrapping(test_dz));
        // Comparison uses saturating math - correctness critical for min tracking
        if test_distance < distance {
            distance = test_distance;
        }
    }

    if range_x < distance && range_z < distance {
        let test_x_int = far_x_int;
        let test_y_int = near_y_int;
        let test_z_int = far_z_int;
        let test_index = lpfn_hash3(
            test_x_int as u32,
            test_y_int as u32,
            test_z_int as u32,
            seed,
        );
        let test_point = get_point_3d(test_index as usize, test_x_int, test_y_int, test_z_int);
        // Range analysis: test_dx, test_dy, test_dz bounded by ~[-2, 2] (same as primary distance)
        let test_dx = x - test_point.0;
        let test_dy = y - test_point.1;
        let test_dz = z - test_point.2;
        let test_distance = test_dx
            .mul_wrapping(test_dx)
            .add_wrapping(test_dy.mul_wrapping(test_dy))
            .add_wrapping(test_dz.mul_wrapping(test_dz));
        // Comparison uses saturating math - correctness critical for min tracking
        if test_distance < distance {
            distance = test_distance;
        }
    }

    if range_y < distance && range_z < distance {
        let test_x_int = near_x_int;
        let test_y_int = far_y_int;
        let test_z_int = far_z_int;
        let test_index = lpfn_hash3(
            test_x_int as u32,
            test_y_int as u32,
            test_z_int as u32,
            seed,
        );
        let test_point = get_point_3d(test_index as usize, test_x_int, test_y_int, test_z_int);
        // Range analysis: test_dx, test_dy, test_dz bounded by ~[-2, 2] (same as primary distance)
        let test_dx = x - test_point.0;
        let test_dy = y - test_point.1;
        let test_dz = z - test_point.2;
        let test_distance = test_dx
            .mul_wrapping(test_dx)
            .add_wrapping(test_dy.mul_wrapping(test_dy))
            .add_wrapping(test_dz.mul_wrapping(test_dz));
        // Comparison uses saturating math - correctness critical for min tracking
        if test_distance < distance {
            distance = test_distance;
        }
    }

    // Three-axis check
    if range_x < distance && range_y < distance && range_z < distance {
        let test_x_int = far_x_int;
        let test_y_int = far_y_int;
        let test_z_int = far_z_int;
        let test_index = lpfn_hash3(
            test_x_int as u32,
            test_y_int as u32,
            test_z_int as u32,
            seed,
        );
        let test_point = get_point_3d(test_index as usize, test_x_int, test_y_int, test_z_int);
        // Range analysis: test_dx, test_dy, test_dz bounded by ~[-2, 2] (same as primary distance)
        let test_dx = x - test_point.0;
        let test_dy = y - test_point.1;
        let test_dz = z - test_point.2;
        let test_distance = test_dx
            .mul_wrapping(test_dx)
            .add_wrapping(test_dy.mul_wrapping(test_dy))
            .add_wrapping(test_dz.mul_wrapping(test_dz));
        // Comparison uses saturating math - correctness critical for min tracking
        if test_distance < distance {
            distance = test_distance;
        }
    }

    // Scale distance to [-1, 1] range
    // The maximum distance in a unit cell is sqrt(3) ≈ 1.732, so squared is 3.0
    // We scale by dividing by 3.0 and then mapping [0, 3] to [-1, 1]
    // distance / 3.0 gives [0, 1], then * 2.0 - 1.0 gives [-1, 1]
    const THREE: Q32 = Q32(0x00030000); // 3.0 in Q16.16

    (distance / THREE) * TWO - Q32::ONE
}

/// 3D Worley noise function (extern C wrapper for compiler).
///
/// # Arguments
/// * `x` - X coordinate as i32 (Q32 fixed-point)
/// * `y` - Y coordinate as i32 (Q32 fixed-point)
/// * `z` - Z coordinate as i32 (Q32 fixed-point)
/// * `seed` - Seed value for randomization
///
/// # Returns
/// Euclidean squared distance as i32 (Q32 fixed-point format), approximately in range [-1, 1]
#[lpfn_impl_macro::lpfn_impl(q32, "float lpfn_worley(vec3 p, uint seed)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpfn_worley3_q32(x: i32, y: i32, z: i32, seed: u32) -> i32 {
    let p = Vec3Q32::new(Q32::from_fixed(x), Q32::from_fixed(y), Q32::from_fixed(z));
    lpfn_worley3(p, seed).to_fixed()
}

/// Get feature point offset from hash index and cell coordinates
/// Returns (offset_x, offset_y, offset_z) in Q32 fixed-point format
fn get_point_3d(index: usize, cell_x: i32, cell_y: i32, cell_z: i32) -> (Q32, Q32, Q32) {
    // Length ranges from 0 to 0.5, based on upper 3 bits of index
    // length = ((index & 0xE0) >> 5) * 0.5 / 7.0
    let length_bits = (index & 0xE0) >> 5;
    let length = Q32::from_i32(length_bits as i32) * HALF / Q32::from_i32(7);

    // Diagonal length
    // Range analysis: length bounded by [0, 0.5], FRAC_1_SQRT_2 ≈ 0.707
    // Product bounded by ~0.353, safely within range for wrapping math
    let diag = length.mul_wrapping(FRAC_1_SQRT_2);

    // Cell origin in Q32
    let cell_x_q32 = Q32::from_i32(cell_x);
    let cell_y_q32 = Q32::from_i32(cell_y);
    let cell_z_q32 = Q32::from_i32(cell_z);

    // Get direction from lower bits (index % 18)
    let (offset_x, offset_y, offset_z) = match index % 18 {
        0 => (diag, diag, Q32::ZERO),
        1 => (diag, -diag, Q32::ZERO),
        2 => (-diag, diag, Q32::ZERO),
        3 => (-diag, -diag, Q32::ZERO),
        4 => (diag, Q32::ZERO, diag),
        5 => (diag, Q32::ZERO, -diag),
        6 => (-diag, Q32::ZERO, diag),
        7 => (-diag, Q32::ZERO, -diag),
        8 => (Q32::ZERO, diag, diag),
        9 => (Q32::ZERO, diag, -diag),
        10 => (Q32::ZERO, -diag, diag),
        11 => (Q32::ZERO, -diag, -diag),
        12 => (length, Q32::ZERO, Q32::ZERO),
        13 => (Q32::ZERO, length, Q32::ZERO),
        14 => (Q32::ZERO, Q32::ZERO, length),
        15 => (-length, Q32::ZERO, Q32::ZERO),
        16 => (Q32::ZERO, -length, Q32::ZERO),
        17 => (Q32::ZERO, Q32::ZERO, -length),
        _ => unreachable!(),
    };

    // Return feature point = cell origin + offset
    (
        cell_x_q32 + offset_x,
        cell_y_q32 + offset_y,
        cell_z_q32 + offset_z,
    )
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    extern crate std;
    use super::*;
    use crate::util::test_helpers::{fixed_to_float, float_to_fixed};

    #[test]
    fn test_worley3_basic() {
        let result1 = __lp_lpfn_worley3_q32(
            float_to_fixed(1.5),
            float_to_fixed(2.3),
            float_to_fixed(0.7),
            0,
        );
        let result2 = __lp_lpfn_worley3_q32(
            float_to_fixed(3.7),
            float_to_fixed(2.3),
            float_to_fixed(0.7),
            0,
        );
        let result3 = __lp_lpfn_worley3_q32(
            float_to_fixed(1.5),
            float_to_fixed(2.3),
            float_to_fixed(0.7),
            1,
        );

        // Different inputs should produce different outputs
        assert_ne!(
            result1, result2,
            "Noise should differ for different x values"
        );
        assert_ne!(result1, result3, "Noise should differ for different seeds");
    }

    #[test]
    fn test_worley3_range() {
        // Test that output is approximately in [-1, 1] range
        for i in 0..50 {
            let x = float_to_fixed(i as f32 * 0.1);
            let y = float_to_fixed(i as f32 * 0.15);
            let z = float_to_fixed(i as f32 * 0.2);
            let result = __lp_lpfn_worley3_q32(x, y, z, 0);
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
    fn test_worley3_deterministic() {
        let result1 = __lp_lpfn_worley3_q32(
            float_to_fixed(42.5),
            float_to_fixed(37.3),
            float_to_fixed(15.7),
            123,
        );
        let result2 = __lp_lpfn_worley3_q32(
            float_to_fixed(42.5),
            float_to_fixed(37.3),
            float_to_fixed(15.7),
            123,
        );

        // Same input and seed should produce same output
        assert_eq!(result1, result2, "Noise should be deterministic");
    }
}
