//! Hash functions for noise generation.
//!
//! These functions provide integer hashing optimized for noise generation.
//! Uses the hash algorithm from the noiz library, optimized for noise generation.
//! Algorithm inspired by https://nullprogram.com/blog/2018/07/31/
//!
//! Credit: noiz library (github.com/ElliottjPierce/noiz)
//!
//! # GLSL Usage
//!
//! These functions are callable from GLSL shaders using the `lpfx_hash` name:
//!
//! ```glsl
//! uint h1 = lpfx_hash(42u, 123u);                    // 1D hash
//! uint h2 = lpfx_hash(uvec2(10u, 20u), 123u);      // 2D hash
//! uint h3 = lpfx_hash(uvec3(10u, 20u, 30u), 123u); // 3D hash
//! ```
//!
//! # Internal Implementation
//!
//! The user-facing `lpfx_hash` functions map to internal `__lpfx_hash_*` functions
//! which are registered in the builtin system. The compiler handles the mapping
//! and argument flattening automatically.

/// Large prime number with even bit distribution.
/// Used as a multiplier and XOR value in the hash function.
const KEY: u32 = 249_222_277;

/// Hash function for 1D coordinates.
///
/// # Arguments
/// * `x` - X coordinate
/// * `seed` - Seed value for randomization
///
/// # Returns
/// Hash value as u32
#[inline(always)]
pub fn lpfx_hash(x: u32, seed: u32) -> u32 {
    hash_impl(x, seed)
}

/// Hash function for 2D coordinates.
///
/// # Arguments
/// * `x` - X coordinate
/// * `y` - Y coordinate
/// * `seed` - Seed value for randomization
///
/// # Returns
/// Hash value as u32
#[inline(always)]
pub fn lpfx_hash2(x: u32, y: u32, seed: u32) -> u32 {
    // Combine coordinates non-commutatively (similar to noiz's UVec2::collapse_for_rng)
    let combined = (x ^ 983742189).wrapping_add((y ^ 102983473).rotate_left(8));
    hash_impl(combined, seed)
}

/// Hash function for 3D coordinates.
///
/// # Arguments
/// * `x` - X coordinate
/// * `y` - Y coordinate
/// * `z` - Z coordinate
/// * `seed` - Seed value for randomization
///
/// # Returns
/// Hash value as u32
#[inline(always)]
pub fn lpfx_hash3(x: u32, y: u32, z: u32, seed: u32) -> u32 {
    // Combine coordinates non-commutatively (similar to noiz's UVec3::collapse_for_rng)
    let combined = (x ^ 983742189)
        .wrapping_add((y ^ 102983473).rotate_left(8))
        .wrapping_add((z ^ 189203473).rotate_left(16));
    hash_impl(combined, seed)
}

/// Hash function for 1D coordinates (extern C wrapper for compiler).
///
/// # Arguments
/// * `x` - X coordinate
/// * `seed` - Seed value for randomization
///
/// # Returns
/// Hash value as u32
#[lpfx_impl_macro::lpfx_impl("uint lpfx_hash(uint x, uint seed)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lpfx_hash_1(x: u32, seed: u32) -> u32 {
    lpfx_hash(x, seed)
}

/// Hash function for 2D coordinates (extern C wrapper for compiler).
///
/// # Arguments
/// * `x` - X coordinate (from uvec2)
/// * `y` - Y coordinate (from uvec2)
/// * `seed` - Seed value for randomization
///
/// # Returns
/// Hash value as u32
#[lpfx_impl_macro::lpfx_impl("uint lpfx_hash(uvec2 xy, uint seed)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lpfx_hash_2(x: u32, y: u32, seed: u32) -> u32 {
    lpfx_hash2(x, y, seed)
}

/// Hash function for 3D coordinates (extern C wrapper for compiler).
///
/// # Arguments
/// * `x` - X coordinate (from uvec3)
/// * `y` - Y coordinate (from uvec3)
/// * `z` - Z coordinate (from uvec3)
/// * `seed` - Seed value for randomization
///
/// # Returns
/// Hash value as u32
#[lpfx_impl_macro::lpfx_impl("uint lpfx_hash(uvec3 xyz, uint seed)")]
#[unsafe(no_mangle)]
pub extern "C" fn __lpfx_hash_3(x: u32, y: u32, z: u32, seed: u32) -> u32 {
    lpfx_hash3(x, y, z, seed)
}

/// Core hash implementation using the noiz algorithm.
///
/// Algorithm:
/// 1. Rotate right by 17 bits and XOR
/// 2. Multiply by KEY
/// 3. Rotate right by 11 bits, XOR with seed
/// 4. Multiply by KEY (using KEY instead of !KEY to preserve LSB)
///
/// This produces a visually pleasing hash suitable for noise generation.
#[inline(always)]
fn hash_impl(input: u32, seed: u32) -> u32 {
    // Inspired by https://nullprogram.com/blog/2018/07/31/
    // Credit: noiz library (github.com/ElliottjPierce/noiz)
    let mut x = input;
    x ^= x.rotate_right(17);
    x = x.wrapping_mul(KEY);
    x ^= x.rotate_right(11) ^ seed;
    x = x.wrapping_mul(KEY);
    x
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    extern crate std;
    use super::*;

    #[test]
    fn test_hash_1_basic() {
        let result1 = __lpfx_hash_1(0, 0);
        let result2 = __lpfx_hash_1(1, 0);
        let result3 = __lpfx_hash_1(0, 1);

        // Different inputs should produce different outputs
        assert_ne!(
            result1, result2,
            "Hash should differ for different x values"
        );
        assert_ne!(result1, result3, "Hash should differ for different seeds");
    }

    #[test]
    fn test_hash_1_deterministic() {
        let result1 = __lpfx_hash_1(42, 123);
        let result2 = __lpfx_hash_1(42, 123);

        // Same input and seed should produce same output
        assert_eq!(result1, result2, "Hash should be deterministic");
    }

    #[test]
    fn test_hash_2_basic() {
        let result1 = __lpfx_hash_2(0, 0, 0);
        let result2 = __lpfx_hash_2(1, 0, 0);
        let result3 = __lpfx_hash_2(0, 1, 0);
        let result4 = __lpfx_hash_2(0, 0, 1);

        // Different inputs should produce different outputs
        assert_ne!(
            result1, result2,
            "Hash should differ for different x values"
        );
        assert_ne!(
            result1, result3,
            "Hash should differ for different y values"
        );
        assert_ne!(result1, result4, "Hash should differ for different seeds");
    }

    #[test]
    fn test_hash_2_deterministic() {
        let result1 = __lpfx_hash_2(10, 20, 30);
        let result2 = __lpfx_hash_2(10, 20, 30);

        // Same inputs and seed should produce same output
        assert_eq!(result1, result2, "Hash should be deterministic");
    }

    #[test]
    fn test_hash_3_basic() {
        let result1 = __lpfx_hash_3(0, 0, 0, 0);
        let result2 = __lpfx_hash_3(1, 0, 0, 0);
        let result3 = __lpfx_hash_3(0, 1, 0, 0);
        let result4 = __lpfx_hash_3(0, 0, 1, 0);
        let result5 = __lpfx_hash_3(0, 0, 0, 1);

        // Different inputs should produce different outputs
        assert_ne!(
            result1, result2,
            "Hash should differ for different x values"
        );
        assert_ne!(
            result1, result3,
            "Hash should differ for different y values"
        );
        assert_ne!(
            result1, result4,
            "Hash should differ for different z values"
        );
        assert_ne!(result1, result5, "Hash should differ for different seeds");
    }

    #[test]
    fn test_hash_3_deterministic() {
        let result1 = __lpfx_hash_3(100, 200, 300, 400);
        let result2 = __lpfx_hash_3(100, 200, 300, 400);

        // Same inputs and seed should produce same output
        assert_eq!(result1, result2, "Hash should be deterministic");
    }

    #[test]
    fn test_hash_coordinate_combination() {
        // Test that coordinate combination is non-commutative
        let result1 = __lpfx_hash_2(10, 20, 0);
        let result2 = __lpfx_hash_2(20, 10, 0);

        // Swapped coordinates should produce different hash
        assert_ne!(result1, result2, "Hash should be non-commutative");
    }
}
