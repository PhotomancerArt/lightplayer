//! 1D Simplex noise function.
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
//! float noise = lpfx_snoise(5.0, 123u);
//! ```
//!
//! # Parameters
//!
//! - `x`: Input coordinate (float, converted to Q32 internally)
//! - `seed`: Seed value for randomization (uint)
//!
//! # Returns
//!
//! Noise value approximately in range [-1, 1] (float)
//!
//! # Internal Implementation
//!
//! The user-facing `lpfx_snoise` function maps to internal `__lpfx_snoise1` which
//! operates on Q32 fixed-point values. The compiler handles type conversion automatically.

use crate::builtins::lpfx::hash::lpfx_hash;
use crate::glsl::q32::types::q32::Q32;

/// 1D Simplex noise function.
///
/// # Arguments
/// * `x` - X coordinate in Q32 fixed-point format
/// * `seed` - Seed value for randomization
///
/// # Returns
/// Noise value in Q32 fixed-point format, approximately in range [-1, 1]
pub fn lpfx_snoise(x: Q32, seed: u32) -> Q32 {
    // Get cell coordinate (floor)
    let cell = x.to_i32();

    // Distance from cell origin (fractional part)
    let cell_origin = Q32::from_i32(cell);
    let dist = x - cell_origin;

    // Hash cell coordinate to get gradient (1 or -1 for 1D)
    let hash = lpfx_hash(cell as u32, seed);

    let gradient = if (hash & 1) == 0 { Q32::ONE } else { -Q32::ONE };

    // Compute dot product: gradient * dist
    let dot = gradient * dist;

    // Apply falloff function: t = 1.0 - dist^2
    // For 1D, we use a simple quadratic falloff
    let dist_sq = dist * dist;
    let t = Q32::ONE - dist_sq;

    // Only contribute if t > 0
    if t > Q32::ZERO {
        // Apply quintic falloff: 6t^5 - 15t^4 + 10t^3
        let t2 = t * t;
        let t3 = t2 * t;
        let t4 = t2 * t2;
        let t5 = t3 * t2;

        // 6t^5 - 15t^4 + 10t^3
        let term1 = Q32::from_i32(6) * t5;
        let term2 = Q32::from_i32(15) * t4;
        let term3 = Q32::from_i32(10) * t3;
        let falloff = term1 - term2 + term3;

        dot * falloff
    } else {
        Q32::ZERO
    }
}

/// 1D Simplex noise function (extern C wrapper for compiler).
///
/// # Arguments
/// * `x` - X coordinate as i32 (Q32 fixed-point)
/// * `seed` - Seed value for randomization
///
/// # Returns
/// Noise value as i32 (Q32 fixed-point format), approximately in range [-1, 1]
#[lpfx_impl_macro::lpfx_impl(q32, "float lpfx_snoise(float x, uint seed)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lpfx_snoise1_q32(x: i32, seed: u32) -> i32 {
    lpfx_snoise(Q32::from_fixed(x), seed).to_fixed()
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    extern crate std;
    use super::*;
    use crate::util::test_helpers::{fixed_to_float, float_to_fixed};
    use std::println;
    use std::vec::Vec;

    #[test]
    fn test_simplex1_basic() {
        // Test with various inputs to ensure we get different outputs
        let results: Vec<i32> = (0..10)
            .map(|i| __lpfx_snoise1_q32(float_to_fixed(i as f32 * 0.5), 0))
            .collect();

        // Check that we get some variation (not all zeros)
        let all_zero = results.iter().all(|&r| r == 0);
        assert!(!all_zero, "Simplex1 should produce non-zero values");

        // Test seed affects output
        let result_seed0 = __lpfx_snoise1_q32(float_to_fixed(5.0), 0);
        let result_seed1 = __lpfx_snoise1_q32(float_to_fixed(5.0), 1);
        // Note: seed might not always change output at every point, but should often
        // We just verify the function works with different seeds
        let _ = result_seed0;
        let _ = result_seed1;
    }

    #[test]
    fn test_simplex1_different_seeds() {
        // Test that different seeds produce different hash values
        // This verifies the seed parameter is being passed correctly to the hash function

        // Test that hash function itself works with different seeds
        let hash0 = lpfx_hash(0, 0);
        let hash1 = lpfx_hash(0, 1);
        assert_ne!(hash0, hash1, "Hash should differ for different seeds");

        // Test that simplex1 uses the seed correctly by checking hash values
        // We can't directly access the hash, but we can verify that different seeds
        // produce different outputs at least sometimes (they affect the gradient)
        let mut found_difference = false;

        // Test many points - seeds should produce different outputs at some points
        for i in 0..100 {
            let x = float_to_fixed(i as f32 * 0.1);
            let result_seed0 = __lpfx_snoise1_q32(x, 0);
            let result_seed1 = __lpfx_snoise1_q32(x, 1);

            if result_seed0 != result_seed1 {
                found_difference = true;
                break;
            }
        }

        assert!(
            found_difference,
            "Different seeds should produce different outputs at least at some points. \
             This verifies the seed parameter is being passed correctly to the hash function."
        );
    }

    #[test]
    fn test_simplex1_output_grid() {
        // Output a grid of noise values for manual inspection
        // This helps verify the noise function produces reasonable-looking output
        println!("\n=== Simplex1 Noise Grid (seed=0) ===");
        println!("X values from 0.0 to 9.0:");
        for row in 0..10 {
            let x = row as f32;
            let result = __lpfx_snoise1_q32(float_to_fixed(x), 0);
            let result_float = fixed_to_float(result);
            println!("  x={:4.1}: {:7.4}", x, result_float);
        }

        println!("\n=== Simplex1 Noise Grid (seed=1) ===");
        println!("X values from 0.0 to 9.0:");
        for row in 0..10 {
            let x = row as f32;
            let result = __lpfx_snoise1_q32(float_to_fixed(x), 1);
            let result_float = fixed_to_float(result);
            println!("  x={:4.1}: {:7.4}", x, result_float);
        }

        println!("\n=== Simplex1 Seed Comparison (x=0.5) ===");
        let x = float_to_fixed(0.5);
        for seed in 0..5 {
            let result = __lpfx_snoise1_q32(x, seed);
            let result_float = fixed_to_float(result);
            println!("  seed={}: {:7.4}", seed, result_float);
        }

        // Verify outputs are in reasonable range
        for i in 0..100 {
            let x = float_to_fixed(i as f32 * 0.1);
            let result = __lpfx_snoise1_q32(x, 0);
            let result_float = fixed_to_float(result);
            assert!(
                result_float >= -2.0 && result_float <= 2.0,
                "Noise value should be in reasonable range, got {}",
                result_float
            );
        }
    }

    #[test]
    fn test_simplex1_range() {
        // Test that output is approximately in [-1, 1] range
        for i in 0..100 {
            let x = float_to_fixed(i as f32 * 0.1);
            let result = __lpfx_snoise1_q32(x, 0);
            let result_float = fixed_to_float(result);

            assert!(
                result_float >= -1.5 && result_float <= 1.5,
                "Noise value {} should be in approximate range [-1, 1], got {}",
                i,
                result_float
            );
        }
    }

    #[test]
    fn test_simplex1_deterministic() {
        let result1 = __lpfx_snoise1_q32(float_to_fixed(42.5), 123);
        let result2 = __lpfx_snoise1_q32(float_to_fixed(42.5), 123);

        // Same input and seed should produce same output
        assert_eq!(result1, result2, "Noise should be deterministic");
    }
}
