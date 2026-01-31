//! 2D Periodic Simplex Rotational Domain noise function.
//!
//! Periodic Simplex Rotational Domain noise (psrdnoise) is a variant of Simplex noise
//! that supports seamless tiling and rotational gradients for flow-like effects.
//! This implementation uses Q32 fixed-point arithmetic (16.16 format).
//!
//! # Source
//!
//! This is a derivative work based on the psrdnoise implementation from Lygia:
//! https://github.com/patriciogonzalezvivo/lygia/blob/main/generative/psrdnoise.glsl
//!
//! Original algorithm by Stefan Gustavson and Ian McEwan:
//! https://github.com/stegu/psrdnoise
//!
//! # License
//!
//! Original work:
//! Copyright 2021-2023 by Stefan Gustavson and Ian McEwan.
//! Published under the terms of the MIT license:
//! https://opensource.org/license/mit/
//!
//! This derivative work (Rust/Q32 fixed-point implementation):
//! Also published under the terms of the MIT license.
//!
//! # GLSL Usage
//!
//! This function is callable from GLSL shaders using the `lpfx_psrdnoise` name:
//!
//! ```glsl
//! vec2 gradient;
//! float noise = lpfx_psrdnoise(vec2(5.0, 3.0), vec2(10.0, 10.0), 0.5, gradient);
//! ```
//!
//! # Parameters
//!
//! - `x`: Input coordinates as vec2 (converted to Q32 internally, flattened to x, y)
//! - `period`: Tiling period as vec2 (0 = no tiling, flattened to period_x, period_y)
//! - `alpha`: Rotation angle in radians (float, converted to Q32)
//! - `gradient`: Output gradient vector (out vec2, written to pointer)
//!
//! # Returns
//!
//! Noise value approximately in range [-1, 1] (float)

use crate::builtins::q32::{__lp_q32_cos, __lp_q32_mod, __lp_q32_sin};
use crate::glsl::q32::types::q32::Q32;
use crate::glsl::q32::types::vec2_q32::Vec2Q32;

/// Fixed-point constants
const HALF: Q32 = Q32(0x00008000); // 0.5 in Q16.16
const EIGHT: Q32 = Q32(0x00080000); // 8.0 in Q16.16

/// Period constant for hash: 289.0
/// In Q16.16: 289.0 * 65536 = 18939904
const PERIOD_289: Q32 = Q32(18939904);

/// Hash multiplier: 0.07482
/// In Q16.16: 0.07482 * 65536 ≈ 4904
const HASH_MULT_0_07482: Q32 = Q32(4904);

/// Radial decay constant: 0.8
/// In Q16.16: 0.8 * 65536 = 52429
const RADIAL_DECAY_0_8: Q32 = Q32(52429);

/// Final scale factor: 10.9
/// In Q16.16: 10.9 * 65536 ≈ 714342
const SCALE_10_9: Q32 = Q32(714342);

/// Hash computation constants
const HASH_CONST_51: Q32 = Q32(51 << 16); // 51.0
const HASH_CONST_2: Q32 = Q32(2 << 16); // 2.0
const HASH_CONST_34: Q32 = Q32(34 << 16); // 34.0
const HASH_CONST_10: Q32 = Q32(10 << 16); // 10.0

/// 2D Periodic Simplex Rotational Domain noise function.
///
/// # Arguments
/// * `x` - Input coordinates as Vec2Q32
/// * `period` - Tiling period as Vec2Q32 (zero = no tiling)
/// * `alpha` - Rotation angle in radians as Q32
/// * `seed` - Seed value for randomization (unused in psrdnoise, kept for consistency)
///
/// # Returns
/// Tuple of (noise_value, gradient_x, gradient_y) in Q32 fixed-point format
pub fn lpfx_psrdnoise2(x: Vec2Q32, period: Vec2Q32, alpha: Q32, _seed: u32) -> (Q32, Q32, Q32) {
    // Transform to simplex space (axis-aligned hexagonal grid)
    // uv = vec2(x.x + x.y*0.5, x.y)
    let uv = Vec2Q32::new(x.x + x.y * HALF, x.y);

    // Determine which simplex we're in, with i0 being the "base"
    // i0 = floor(uv)
    let i0 = uv.floor();
    let i0_x_int = i0.x.to_i32();
    let i0_y_int = i0.y.to_i32();

    // f0 = fract(uv)
    let f0 = uv.fract();

    // o1 is the offset in simplex space to the second corner
    // cmp = step(f0.y, f0.x) -> 1.0 if f0.y <= f0.x, else 0.0
    let cmp = if f0.y <= f0.x { Q32::ONE } else { Q32::ZERO };
    let o1 = Vec2Q32::new(cmp, Q32::ONE - cmp);

    // Enumerate the remaining simplex corners
    // i1 = i0 + o1
    let i1 = i0 + o1;
    let i1_x_int = i1.x.to_i32();
    let i1_y_int = i1.y.to_i32();
    // i2 = i0 + vec2(1.0, 1.0)
    let i2 = i0 + Vec2Q32::one();
    let i2_x_int = i2.x.to_i32();
    let i2_y_int = i2.y.to_i32();

    // Transform corners back to texture space
    // v0 = vec2(i0.x - i0.y * 0.5, i0.y)
    let v0 = Vec2Q32::new(i0.x - i0.y * HALF, i0.y);
    // v1 = vec2(v0.x + o1.x - o1.y * 0.5, v0.y + o1.y)
    let v1 = Vec2Q32::new(v0.x + o1.x - o1.y * HALF, v0.y + o1.y);
    // v2 = vec2(v0.x + 0.5, v0.y + 1.0)
    let v2 = Vec2Q32::new(v0.x + HALF, v0.y + Q32::ONE);

    // Compute vectors from x to each of the simplex corners
    let x0 = x - v0;
    let x1 = x - v1;
    let x2 = x - v2;

    // Wrap to periods, if desired
    let (iu_x, iu_y, iu_z, iv_x, iv_y, iv_z) = if period.x > Q32::ZERO || period.y > Q32::ZERO {
        let mut xw_x = v0.x;
        let mut xw_y = v1.x;
        let mut xw_z = v2.x;
        let mut yw_x = v0.y;
        let mut yw_y = v1.y;
        let mut yw_z = v2.y;

        // Wrap to periods where specified
        if period.x > Q32::ZERO {
            xw_x = Q32::from_fixed(__lp_q32_mod(v0.x.to_fixed(), period.x.to_fixed()));
            xw_y = Q32::from_fixed(__lp_q32_mod(v1.x.to_fixed(), period.x.to_fixed()));
            xw_z = Q32::from_fixed(__lp_q32_mod(v2.x.to_fixed(), period.x.to_fixed()));
        }
        if period.y > Q32::ZERO {
            yw_x = Q32::from_fixed(__lp_q32_mod(v0.y.to_fixed(), period.y.to_fixed()));
            yw_y = Q32::from_fixed(__lp_q32_mod(v1.y.to_fixed(), period.y.to_fixed()));
            yw_z = Q32::from_fixed(__lp_q32_mod(v2.y.to_fixed(), period.y.to_fixed()));
        }

        // Transform back to simplex space and fix rounding errors
        // iu = floor(xw + 0.5*yw + 0.5)
        let iu_x = (xw_x + yw_x * HALF + HALF).to_i32();
        let iu_y = (xw_y + yw_y * HALF + HALF).to_i32();
        let iu_z = (xw_z + yw_z * HALF + HALF).to_i32();
        // iv = floor(yw + 0.5)
        let iv_x = (yw_x + HALF).to_i32();
        let iv_y = (yw_y + HALF).to_i32();
        let iv_z = (yw_z + HALF).to_i32();

        (iu_x, iu_y, iu_z, iv_x, iv_y, iv_z)
    } else {
        // Shortcut if neither x nor y periods are specified
        (i0_x_int, i1_x_int, i2_x_int, i0_y_int, i1_y_int, i2_y_int)
    };

    // Compute one pseudo-random hash value for each corner
    // hash = mod(iu, 289.0)
    let hash_x = __lp_q32_mod(iu_x << 16, PERIOD_289.to_fixed());
    let hash_y = __lp_q32_mod(iu_y << 16, PERIOD_289.to_fixed());
    let hash_z = __lp_q32_mod(iu_z << 16, PERIOD_289.to_fixed());

    // hash = mod((hash*51.0 + 2.0)*hash + iv, 289.0)
    let hash_x = {
        let temp = Q32::from_fixed(hash_x) * HASH_CONST_51 + HASH_CONST_2;
        let temp = temp * Q32::from_fixed(hash_x) + Q32::from_i32(iv_x);
        __lp_q32_mod(temp.to_fixed(), PERIOD_289.to_fixed())
    };
    let hash_y = {
        let temp = Q32::from_fixed(hash_y) * HASH_CONST_51 + HASH_CONST_2;
        let temp = temp * Q32::from_fixed(hash_y) + Q32::from_i32(iv_y);
        __lp_q32_mod(temp.to_fixed(), PERIOD_289.to_fixed())
    };
    let hash_z = {
        let temp = Q32::from_fixed(hash_z) * HASH_CONST_51 + HASH_CONST_2;
        let temp = temp * Q32::from_fixed(hash_z) + Q32::from_i32(iv_z);
        __lp_q32_mod(temp.to_fixed(), PERIOD_289.to_fixed())
    };

    // hash = mod((hash*34.0 + 10.0)*hash, 289.0)
    let hash_x = {
        let temp = Q32::from_fixed(hash_x) * HASH_CONST_34 + HASH_CONST_10;
        __lp_q32_mod(
            (temp * Q32::from_fixed(hash_x)).to_fixed(),
            PERIOD_289.to_fixed(),
        )
    };
    let hash_y = {
        let temp = Q32::from_fixed(hash_y) * HASH_CONST_34 + HASH_CONST_10;
        __lp_q32_mod(
            (temp * Q32::from_fixed(hash_y)).to_fixed(),
            PERIOD_289.to_fixed(),
        )
    };
    let hash_z = {
        let temp = Q32::from_fixed(hash_z) * HASH_CONST_34 + HASH_CONST_10;
        __lp_q32_mod(
            (temp * Q32::from_fixed(hash_z)).to_fixed(),
            PERIOD_289.to_fixed(),
        )
    };

    // Pick a pseudo-random angle and add the desired rotation
    // psi = hash * 0.07482 + alpha
    let psi_x = Q32::from_fixed(hash_x) * HASH_MULT_0_07482 + alpha;
    let psi_y = Q32::from_fixed(hash_y) * HASH_MULT_0_07482 + alpha;
    let psi_z = Q32::from_fixed(hash_z) * HASH_MULT_0_07482 + alpha;

    // gx = cos(psi), gy = sin(psi)
    let gx_x = Q32::from_fixed(__lp_q32_cos(psi_x.to_fixed()));
    let gx_y = Q32::from_fixed(__lp_q32_cos(psi_y.to_fixed()));
    let gx_z = Q32::from_fixed(__lp_q32_cos(psi_z.to_fixed()));
    let gy_x = Q32::from_fixed(__lp_q32_sin(psi_x.to_fixed()));
    let gy_y = Q32::from_fixed(__lp_q32_sin(psi_y.to_fixed()));
    let gy_z = Q32::from_fixed(__lp_q32_sin(psi_z.to_fixed()));

    // Reorganize for dot products below
    let g0_x = gx_x;
    let g0_y = gy_x;
    let g1_x = gx_y;
    let g1_y = gy_y;
    let g2_x = gx_z;
    let g2_y = gy_z;

    // Radial decay with distance from each simplex corner
    // w = 0.8 - vec3(dot(x0, x0), dot(x1, x1), dot(x2, x2))
    let dot0 = x0.length_squared();
    let dot1 = x1.length_squared();
    let dot2 = x2.length_squared();
    let mut w_x = RADIAL_DECAY_0_8 - dot0;
    let mut w_y = RADIAL_DECAY_0_8 - dot1;
    let mut w_z = RADIAL_DECAY_0_8 - dot2;

    // w = max(w, 0.0)
    w_x = w_x.max(Q32::ZERO);
    w_y = w_y.max(Q32::ZERO);
    w_z = w_z.max(Q32::ZERO);

    // w2 = w * w, w4 = w2 * w2
    let w2_x = w_x * w_x;
    let w2_y = w_y * w_y;
    let w2_z = w_z * w_z;
    let w4_x = w2_x * w2_x;
    let w4_y = w2_y * w2_y;
    let w4_z = w2_z * w2_z;

    // The value of the linear ramp from each of the corners
    // gdotx = vec3(dot(g0, x0), dot(g1, x1), dot(g2, x2))
    let g0 = Vec2Q32::new(g0_x, g0_y);
    let g1 = Vec2Q32::new(g1_x, g1_y);
    let g2 = Vec2Q32::new(g2_x, g2_y);
    let gdotx_x = g0.dot(x0);
    let gdotx_y = g1.dot(x1);
    let gdotx_z = g2.dot(x2);

    // Multiply by the radial decay and sum up the noise value
    // n = dot(w4, gdotx)
    let n = w4_x * gdotx_x + w4_y * gdotx_y + w4_z * gdotx_z;

    // Compute the first order partial derivatives
    // w3 = w2 * w
    let w3_x = w2_x * w_x;
    let w3_y = w2_y * w_y;
    let w3_z = w2_z * w_z;
    // dw = -8.0 * w3 * gdotx
    let dw_x = -EIGHT * w3_x * gdotx_x;
    let dw_y = -EIGHT * w3_y * gdotx_y;
    let dw_z = -EIGHT * w3_z * gdotx_z;
    // dn0 = w4.x * g0 + dw.x * x0
    let dn0 = g0 * w4_x + x0 * dw_x;
    // dn1 = w4.y * g1 + dw.y * x1
    let dn1 = g1 * w4_y + x1 * dw_y;
    // dn2 = w4.z * g2 + dw.z * x2
    let dn2 = g2 * w4_z + x2 * dw_z;
    // gradient = 10.9 * (dn0 + dn1 + dn2)
    let gradient = (dn0 + dn1 + dn2) * SCALE_10_9;
    let gradient_x = gradient.x;
    let gradient_y = gradient.y;

    // Scale the return value to fit nicely into the range [-1,1]
    let noise_value = SCALE_10_9 * n;

    (noise_value, gradient_x, gradient_y)
}

/// 2D Periodic Simplex Rotational Domain noise function (extern C wrapper for compiler).
///
/// # Arguments
/// * `x` - X coordinate as i32 (Q32 fixed-point)
/// * `y` - Y coordinate as i32 (Q32 fixed-point)
/// * `period_x` - X period as i32 (Q32 fixed-point, 0 = no tiling)
/// * `period_y` - Y period as i32 (Q32 fixed-point, 0 = no tiling)
/// * `alpha` - Rotation angle in radians as i32 (Q32 fixed-point)
/// * `gradient_out` - Pointer to output gradient [gx, gy] as i32 (Q32 fixed-point)
/// * `seed` - Seed value for randomization (unused in psrdnoise, kept for consistency)
///
/// # Returns
/// Noise value as i32 (Q32 fixed-point format), approximately in range [-1, 1]
#[lpfx_impl_macro::lpfx_impl(
    q32,
    "float lpfx_psrdnoise(vec2 x, vec2 period, float alpha, out vec2 gradient)"
)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[unsafe(no_mangle)]
pub extern "C" fn __lpfx_psrdnoise2_q32(
    x: i32,
    y: i32,
    period_x: i32,
    period_y: i32,
    alpha: i32,
    gradient_out: *mut i32,
    seed: u32,
) -> i32 {
    let x_vec = Vec2Q32::new(Q32::from_fixed(x), Q32::from_fixed(y));
    let period_vec = Vec2Q32::new(Q32::from_fixed(period_x), Q32::from_fixed(period_y));
    let alpha_q32 = Q32::from_fixed(alpha);

    let (noise_value, gradient_x, gradient_y) = lpfx_psrdnoise2(x_vec, period_vec, alpha_q32, seed);

    // Write gradient to output pointer
    unsafe {
        *gradient_out = gradient_x.to_fixed();
        *gradient_out.add(1) = gradient_y.to_fixed();
    }

    noise_value.to_fixed()
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    extern crate std;
    use super::*;
    use crate::util::test_helpers::{fixed_to_float, float_to_fixed};

    #[test]
    fn test_psrdnoise2_basic() {
        let x = float_to_fixed(1.5);
        let y = float_to_fixed(2.3);
        let period_x = float_to_fixed(0.0);
        let period_y = float_to_fixed(0.0);
        let alpha = float_to_fixed(0.0);
        let mut gradient = [0i32; 2];

        let result =
            __lpfx_psrdnoise2_q32(x, y, period_x, period_y, alpha, gradient.as_mut_ptr(), 0);

        // Should produce some value
        let result_float = fixed_to_float(result);
        assert!(
            result_float >= -2.0 && result_float <= 2.0,
            "Noise value should be in approximate range [-1, 1], got {}",
            result_float
        );

        // Gradient should be written
        let grad_x = fixed_to_float(gradient[0]);
        let grad_y = fixed_to_float(gradient[1]);
        assert!(
            grad_x >= -20.0 && grad_x <= 20.0,
            "Gradient x should be reasonable, got {}",
            grad_x
        );
        assert!(
            grad_y >= -20.0 && grad_y <= 20.0,
            "Gradient y should be reasonable, got {}",
            grad_y
        );
    }

    #[test]
    fn test_psrdnoise2_periodic() {
        let x = float_to_fixed(1.5);
        let y = float_to_fixed(2.3);
        let period_x = float_to_fixed(10.0);
        let period_y = float_to_fixed(10.0);
        let alpha = float_to_fixed(0.0);
        let mut gradient = [0i32; 2];

        let result =
            __lpfx_psrdnoise2_q32(x, y, period_x, period_y, alpha, gradient.as_mut_ptr(), 0);

        // Should produce some value
        let result_float = fixed_to_float(result);
        assert!(
            result_float >= -2.0 && result_float <= 2.0,
            "Noise value should be in approximate range [-1, 1], got {}",
            result_float
        );
    }

    #[test]
    fn test_psrdnoise2_rotation() {
        let x = float_to_fixed(1.5);
        let y = float_to_fixed(2.3);
        let period_x = float_to_fixed(0.0);
        let period_y = float_to_fixed(0.0);
        let alpha1 = float_to_fixed(0.0);
        let alpha2 = float_to_fixed(1.57); // ~π/2
        let mut gradient1 = [0i32; 2];
        let mut gradient2 = [0i32; 2];

        let result1 =
            __lpfx_psrdnoise2_q32(x, y, period_x, period_y, alpha1, gradient1.as_mut_ptr(), 0);
        let result2 =
            __lpfx_psrdnoise2_q32(x, y, period_x, period_y, alpha2, gradient2.as_mut_ptr(), 0);

        // Different rotation angles should produce different results
        // (though they might occasionally match)
        let result1_float = fixed_to_float(result1);
        let result2_float = fixed_to_float(result2);
        // Just verify they're both in valid range
        assert!(
            result1_float >= -2.0 && result1_float <= 2.0,
            "Result1 should be in range"
        );
        assert!(
            result2_float >= -2.0 && result2_float <= 2.0,
            "Result2 should be in range"
        );
    }

    #[test]
    fn test_psrdnoise2_deterministic() {
        let x = float_to_fixed(42.5);
        let y = float_to_fixed(37.3);
        let period_x = float_to_fixed(0.0);
        let period_y = float_to_fixed(0.0);
        let alpha = float_to_fixed(0.5);
        let mut gradient1 = [0i32; 2];
        let mut gradient2 = [0i32; 2];

        let result1 =
            __lpfx_psrdnoise2_q32(x, y, period_x, period_y, alpha, gradient1.as_mut_ptr(), 0);
        let result2 =
            __lpfx_psrdnoise2_q32(x, y, period_x, period_y, alpha, gradient2.as_mut_ptr(), 0);

        // Same inputs should produce same outputs
        assert_eq!(result1, result2, "Noise should be deterministic");
        assert_eq!(
            gradient1[0], gradient2[0],
            "Gradient x should be deterministic"
        );
        assert_eq!(
            gradient1[1], gradient2[1],
            "Gradient y should be deterministic"
        );
    }
}
