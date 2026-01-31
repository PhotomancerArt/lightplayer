//! 3D Periodic Simplex Rotational Domain noise function.
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
//! vec3 gradient;
//! float noise = lpfx_psrdnoise(vec3(5.0, 3.0, 1.0), vec3(10.0, 10.0, 10.0), 0.5, gradient);
//! ```
//!
//! # Parameters
//!
//! - `x`: Input coordinates as vec3 (converted to Q32 internally, flattened to x, y, z)
//! - `period`: Tiling period as vec3 (0 = no tiling, flattened to period_x, period_y, period_z)
//! - `alpha`: Rotation angle in radians (float, converted to Q32)
//! - `gradient`: Output gradient vector (out vec3, written to pointer)
//!
//! # Returns
//!
//! Noise value approximately in range [-1, 1] (float)

use crate::builtins::q32::__lp_q32_mod;
use crate::glsl::q32::fns;
use crate::glsl::q32::types::q32::Q32;
use crate::glsl::q32::types::vec3_q32::Vec3Q32;
use crate::glsl::q32::types::vec4_q32::Vec4Q32;

/// Fixed-point constants
const HALF: Q32 = Q32(0x00008000); // 0.5 in Q16.16
const SIX: Q32 = Q32(0x00060000); // 6.0 in Q16.16

/// Period constant for hash: 289.0
/// In Q16.16: 289.0 * 65536 = 18939904
const PERIOD_289: Q32 = Q32(18939904);

/// Radial decay constant: 0.5
/// In Q16.16: 0.5 * 65536 = 32768
const RADIAL_DECAY_0_5: Q32 = Q32(32768);

/// Final scale factor: 39.5
/// In Q16.16: 39.5 * 65536 ≈ 2588672
const SCALE_39_5: Q32 = Q32(2588672);

/// Hash computation constants
const HASH_CONST_34: Q32 = Q32(34 << 16); // 34.0

/// Fibonacci spiral constants
/// 2*pi/golden ratio ≈ 3.883222077
const THETA_MULT: Q32 = Q32(254545); // 3.883222077 * 65536
/// -0.006920415
const SZ_MULT: Q32 = Q32(-454); // -0.006920415 * 65536
/// 0.996539792
const SZ_ADD: Q32 = Q32(65296); // 0.996539792 * 65536
/// 10*pi/289 ≈ 0.108705628
const PSI_MULT: Q32 = Q32(7124); // 0.108705628 * 65536
/// 1/3 ≈ 0.33333333
const ONE_THIRD: Q32 = Q32(21845); // 0.33333333 * 65536
/// 1/6 ≈ 0.16666667
const ONE_SIXTH: Q32 = Q32(10923); // 0.16666667 * 65536

/// Helper: mod289(x) = mod(x, 289.0)
#[inline(always)]
fn mod289_q32(x: i32) -> i32 {
    __lp_q32_mod(x, PERIOD_289.to_fixed())
}

/// Helper: permute(v) = mod289(((v * 34.0) + 1.0) * v)
#[inline(always)]
fn permute_q32(v: i32) -> i32 {
    let v_q32 = Q32::from_fixed(v);
    let temp = v_q32 * HASH_CONST_34 + Q32::ONE;
    mod289_q32((temp * v_q32).to_fixed())
}

/// 3D Periodic Simplex Rotational Domain noise function.
///
/// # Arguments
/// * `x` - Input coordinates as Vec3Q32
/// * `period` - Tiling period as Vec3Q32 (zero = no tiling)
/// * `alpha` - Rotation angle in radians as Q32
/// * `seed` - Seed value for randomization (unused in psrdnoise, kept for consistency)
///
/// # Returns
/// Tuple of (noise_value, gradient_x, gradient_y, gradient_z) in Q32 fixed-point format
pub fn lpfx_psrdnoise3(
    x: Vec3Q32,
    period: Vec3Q32,
    alpha: Q32,
    _seed: u32,
) -> (Q32, Q32, Q32, Q32) {
    // Transform to simplex space (tetrahedral grid)
    // Using optimized transformation: uvw = x + dot(x, vec3(1.0/3.0))
    let dot_sum = x.x + x.y + x.z;
    let uvw = x + Vec3Q32::new(
        dot_sum * ONE_THIRD,
        dot_sum * ONE_THIRD,
        dot_sum * ONE_THIRD,
    );

    // Determine which simplex we're in, i0 is the "base corner"
    // i0 = floor(uvw)
    let i0 = uvw.floor();
    let i0_x_int = i0.x.to_i32();
    let i0_y_int = i0.y.to_i32();
    let i0_z_int = i0.z.to_i32();

    // f0 = fract(uvw)
    let f0 = uvw.fract();

    // To determine which simplex corners are closest, rank order the
    // magnitudes of u,v,w, resolving ties in priority order u,v,w
    // g_ = step(f0.xyx, f0.yzz) -> 1.0 if f0.xyx <= f0.yzz, else 0.0
    let g_ = f0.xyx().step(f0.yzz());
    // l_ = 1.0 - g_
    let l_ = Vec3Q32::one() - g_;
    // g = vec3(l_.z, g_.xy)
    let g = Vec3Q32::new(l_.z, g_.x, g_.y);
    // l = vec3(l_.xy, g_.z)
    let l = Vec3Q32::new(l_.x, l_.y, g_.z);
    // o1 = min(g, l), o2 = max(g, l)
    let o1 = g.min(l);
    let o2 = g.max(l);

    // Enumerate the remaining simplex corners
    // i1 = i0 + o1, i2 = i0 + o2, i3 = i0 + vec3(1.0)
    let i1 = i0 + o1;
    let i2 = i0 + o2;
    let i3 = i0 + Vec3Q32::one();
    let i1_x_int = i1.x.to_i32();
    let i1_y_int = i1.y.to_i32();
    let i1_z_int = i1.z.to_i32();
    let i2_x_int = i2.x.to_i32();
    let i2_y_int = i2.y.to_i32();
    let i2_z_int = i2.z.to_i32();
    let i3_x_int = i3.x.to_i32();
    let i3_y_int = i3.y.to_i32();
    let i3_z_int = i3.z.to_i32();

    // Transform the corners back to texture space
    // Using optimized transformation: v = i - dot(i, vec3(1.0/6.0))
    let dot0 = i0.dot(Vec3Q32::one()) * ONE_SIXTH;
    let dot1 = i1.dot(Vec3Q32::one()) * ONE_SIXTH;
    let dot2 = i2.dot(Vec3Q32::one()) * ONE_SIXTH;
    let dot3 = i3.dot(Vec3Q32::one()) * ONE_SIXTH;

    let v0 = i0 - Vec3Q32::new(dot0, dot0, dot0);
    let v1 = i1 - Vec3Q32::new(dot1, dot1, dot1);
    let v2 = i2 - Vec3Q32::new(dot2, dot2, dot2);
    let v3 = i3 - Vec3Q32::new(dot3, dot3, dot3);

    // Compute vectors to each of the simplex corners
    let x0 = x - v0;
    let x1 = x - v1;
    let x2 = x - v2;
    let x3 = x - v3;

    // Wrap to periods, if desired
    let (
        i0_x_final,
        i0_y_final,
        i0_z_final,
        i1_x_final,
        i1_y_final,
        i1_z_final,
        i2_x_final,
        i2_y_final,
        i2_z_final,
        i3_x_final,
        i3_y_final,
        i3_z_final,
    ) = if period.x > Q32::ZERO || period.y > Q32::ZERO || period.z > Q32::ZERO {
        let mut vx = Vec4Q32::new(v0.x, v1.x, v2.x, v3.x);
        let mut vy = Vec4Q32::new(v0.y, v1.y, v2.y, v3.y);
        let mut vz = Vec4Q32::new(v0.z, v1.z, v2.z, v3.z);

        // Wrap to periods where specified
        if period.x > Q32::ZERO {
            vx = vx.modulo_scalar(period.x);
        }
        if period.y > Q32::ZERO {
            vy = vy.modulo_scalar(period.y);
        }
        if period.z > Q32::ZERO {
            vz = vz.modulo_scalar(period.z);
        }

        // Transform wrapped coordinates back to uvw
        // i = v + dot(v, vec3(1.0/3.0))
        let dot_v0 = (vx.x + vy.x + vz.x) * ONE_THIRD;
        let dot_v1 = (vx.y + vy.y + vz.y) * ONE_THIRD;
        let dot_v2 = (vx.z + vy.z + vz.z) * ONE_THIRD;
        let dot_v3 = (vx.w + vy.w + vz.w) * ONE_THIRD;

        let v0_wrapped = Vec3Q32::new(vx.x, vy.x, vz.x);
        let v1_wrapped = Vec3Q32::new(vx.y, vy.y, vz.y);
        let v2_wrapped = Vec3Q32::new(vx.z, vy.z, vz.z);
        let v3_wrapped = Vec3Q32::new(vx.w, vy.w, vz.w);

        let i0_wrapped =
            (v0_wrapped + Vec3Q32::new(dot_v0, dot_v0, dot_v0) + Vec3Q32::new(HALF, HALF, HALF))
                .floor();
        let i1_wrapped =
            (v1_wrapped + Vec3Q32::new(dot_v1, dot_v1, dot_v1) + Vec3Q32::new(HALF, HALF, HALF))
                .floor();
        let i2_wrapped =
            (v2_wrapped + Vec3Q32::new(dot_v2, dot_v2, dot_v2) + Vec3Q32::new(HALF, HALF, HALF))
                .floor();
        let i3_wrapped =
            (v3_wrapped + Vec3Q32::new(dot_v3, dot_v3, dot_v3) + Vec3Q32::new(HALF, HALF, HALF))
                .floor();

        (
            i0_wrapped.x.to_i32(),
            i0_wrapped.y.to_i32(),
            i0_wrapped.z.to_i32(),
            i1_wrapped.x.to_i32(),
            i1_wrapped.y.to_i32(),
            i1_wrapped.z.to_i32(),
            i2_wrapped.x.to_i32(),
            i2_wrapped.y.to_i32(),
            i2_wrapped.z.to_i32(),
            i3_wrapped.x.to_i32(),
            i3_wrapped.y.to_i32(),
            i3_wrapped.z.to_i32(),
        )
    } else {
        (
            i0_x_int, i0_y_int, i0_z_int, i1_x_int, i1_y_int, i1_z_int, i2_x_int, i2_y_int,
            i2_z_int, i3_x_int, i3_y_int, i3_z_int,
        )
    };

    // Avoid truncation effects in permutation
    // i0 = mod289(i0), etc.
    let i0_x_mod = mod289_q32(i0_x_final << 16) >> 16;
    let i0_y_mod = mod289_q32(i0_y_final << 16) >> 16;
    let i0_z_mod = mod289_q32(i0_z_final << 16) >> 16;
    let i1_x_mod = mod289_q32(i1_x_final << 16) >> 16;
    let i1_y_mod = mod289_q32(i1_y_final << 16) >> 16;
    let i1_z_mod = mod289_q32(i1_z_final << 16) >> 16;
    let i2_x_mod = mod289_q32(i2_x_final << 16) >> 16;
    let i2_y_mod = mod289_q32(i2_y_final << 16) >> 16;
    let i2_z_mod = mod289_q32(i2_z_final << 16) >> 16;
    let i3_x_mod = mod289_q32(i3_x_final << 16) >> 16;
    let i3_y_mod = mod289_q32(i3_y_final << 16) >> 16;
    let i3_z_mod = mod289_q32(i3_z_final << 16) >> 16;

    // Compute one pseudo-random hash value for each corner
    // hash = permute(permute(permute(vec4(i0.z, i1.z, i2.z, i3.z)) + vec4(i0.y, i1.y, i2.y, i3.y)) + vec4(i0.x, i1.x, i2.x, i3.x))
    let hash_z0 = permute_q32(i0_z_mod << 16);
    let hash_z1 = permute_q32(i1_z_mod << 16);
    let hash_z2 = permute_q32(i2_z_mod << 16);
    let hash_z3 = permute_q32(i3_z_mod << 16);

    let hash_y0 = permute_q32((hash_z0 >> 16) + (i0_y_mod << 16));
    let hash_y1 = permute_q32((hash_z1 >> 16) + (i1_y_mod << 16));
    let hash_y2 = permute_q32((hash_z2 >> 16) + (i2_y_mod << 16));
    let hash_y3 = permute_q32((hash_z3 >> 16) + (i3_y_mod << 16));

    let hash_x0 = permute_q32((hash_y0 >> 16) + (i0_x_mod << 16));
    let hash_x1 = permute_q32((hash_y1 >> 16) + (i1_x_mod << 16));
    let hash_x2 = permute_q32((hash_y2 >> 16) + (i2_x_mod << 16));
    let hash_x3 = permute_q32((hash_y3 >> 16) + (i3_x_mod << 16));

    // Compute generating gradients from a Fibonacci spiral on the unit sphere
    // theta = hash * 3.883222077 (2*pi/golden ratio)
    let hash = Vec4Q32::new(
        Q32::from_fixed(hash_x0),
        Q32::from_fixed(hash_x1),
        Q32::from_fixed(hash_x2),
        Q32::from_fixed(hash_x3),
    );
    let theta = hash * THETA_MULT;

    // sz = hash * -0.006920415 + 0.996539792 (1-(hash+0.5)*2/289)
    let sz = hash * SZ_MULT + Vec4Q32::new(SZ_ADD, SZ_ADD, SZ_ADD, SZ_ADD);

    // psi = hash * 0.108705628 (10*pi/289)
    let psi = hash * PSI_MULT;

    // Ct = cos(theta), St = sin(theta)
    let ct = fns::cos_vec4(theta);
    let st = fns::sin_vec4(theta);

    // sz_prime = sqrt(1.0 - sz*sz)
    let sz_prime = fns::sqrt_vec4(Vec4Q32::one() - sz.mul_comp(sz));

    // Rotate gradients by angle alpha around a pseudo-random orthogonal axis
    // Using fast rotation algorithm (PSRDNOISE_FAST_ROTATION)
    // qx = St, qy = -Ct, qz = 0.0
    let qx = st;
    let qy = -ct;
    let qz = Vec4Q32::zero();

    // px = sz * qy, py = -sz * qx, pz = sz_prime
    let px = sz.mul_comp(qy);
    let py = -sz.mul_comp(qx);
    let pz = sz_prime;

    // psi += alpha (psi and alpha in the same plane)
    let psi_final = psi + Vec4Q32::new(alpha, alpha, alpha, alpha);

    // Sa = sin(psi), Ca = cos(psi)
    let sa = fns::sin_vec4(psi_final);
    let ca = fns::cos_vec4(psi_final);

    // gx = Ca * px + Sa * qx, gy = Ca * py + Sa * qy, gz = Ca * pz + Sa * qz
    let gx = ca.mul_comp(px) + sa.mul_comp(qx);
    let gy = ca.mul_comp(py) + sa.mul_comp(qy);
    let gz = ca.mul_comp(pz) + sa.mul_comp(qz);

    // Reorganize for dot products below
    let g0 = Vec3Q32::new(gx.x, gy.x, gz.x);
    let g1 = Vec3Q32::new(gx.y, gy.y, gz.y);
    let g2 = Vec3Q32::new(gx.z, gy.z, gz.z);
    let g3 = Vec3Q32::new(gx.w, gy.w, gz.w);

    // Radial decay with distance from each simplex corner
    // w = 0.5 - vec4(dot(x0,x0), dot(x1,x1), dot(x2,x2), dot(x3,x3))
    let dot0 = x0.length_squared();
    let dot1 = x1.length_squared();
    let dot2 = x2.length_squared();
    let dot3 = x3.length_squared();
    let mut w = Vec4Q32::new(
        RADIAL_DECAY_0_5 - dot0,
        RADIAL_DECAY_0_5 - dot1,
        RADIAL_DECAY_0_5 - dot2,
        RADIAL_DECAY_0_5 - dot3,
    );

    // w = max(w, 0.0)
    w = w.max(Vec4Q32::zero());

    // w2 = w * w, w3 = w2 * w
    let w2 = w.mul_comp(w);
    let w3 = w2.mul_comp(w);

    // The value of the linear ramp from each of the corners
    // gdotx = vec4(dot(g0,x0), dot(g1,x1), dot(g2,x2), dot(g3,x3))
    let gdotx = Vec4Q32::new(g0.dot(x0), g1.dot(x1), g2.dot(x2), g3.dot(x3));

    // Multiply by the radial decay and sum up the noise value
    // n = dot(w3, gdotx)
    let n = w3.dot(gdotx);

    // Compute the first order partial derivatives
    // dw = -6.0 * w2 * gdotx
    let dw = w2.mul_comp(gdotx) * -SIX;
    // dn0 = w3.x * g0 + dw.x * x0, etc.
    let dn0 = g0 * w3.x + x0 * dw.x;
    let dn1 = g1 * w3.y + x1 * dw.y;
    let dn2 = g2 * w3.z + x2 * dw.z;
    let dn3 = g3 * w3.w + x3 * dw.w;
    // gradient = 39.5 * (dn0 + dn1 + dn2 + dn3)
    let gradient = (dn0 + dn1 + dn2 + dn3) * SCALE_39_5;
    let gradient_x = gradient.x;
    let gradient_y = gradient.y;
    let gradient_z = gradient.z;

    // Scale the return value to fit nicely into the range [-1,1]
    let noise_value = SCALE_39_5 * n;

    (noise_value, gradient_x, gradient_y, gradient_z)
}

/// 3D Periodic Simplex Rotational Domain noise function (extern C wrapper for compiler).
///
/// # Arguments
/// * `x` - X coordinate as i32 (Q32 fixed-point)
/// * `y` - Y coordinate as i32 (Q32 fixed-point)
/// * `z` - Z coordinate as i32 (Q32 fixed-point)
/// * `period_x` - X period as i32 (Q32 fixed-point, 0 = no tiling)
/// * `period_y` - Y period as i32 (Q32 fixed-point, 0 = no tiling)
/// * `period_z` - Z period as i32 (Q32 fixed-point, 0 = no tiling)
/// * `alpha` - Rotation angle in radians as i32 (Q32 fixed-point)
/// * `gradient_out` - Pointer to output gradient [gx, gy, gz] as i32 (Q32 fixed-point)
/// * `seed` - Seed value for randomization (unused in psrdnoise, kept for consistency)
///
/// # Returns
/// Noise value as i32 (Q32 fixed-point format), approximately in range [-1, 1]
#[lpfx_impl_macro::lpfx_impl(
    q32,
    "float lpfx_psrdnoise(vec3 x, vec3 period, float alpha, out vec3 gradient)"
)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[unsafe(no_mangle)]
pub extern "C" fn __lpfx_psrdnoise3_q32(
    x: i32,
    y: i32,
    z: i32,
    period_x: i32,
    period_y: i32,
    period_z: i32,
    alpha: i32,
    gradient_out: *mut i32,
    seed: u32,
) -> i32 {
    let x_vec = Vec3Q32::new(Q32::from_fixed(x), Q32::from_fixed(y), Q32::from_fixed(z));
    let period_vec = Vec3Q32::new(
        Q32::from_fixed(period_x),
        Q32::from_fixed(period_y),
        Q32::from_fixed(period_z),
    );
    let alpha_q32 = Q32::from_fixed(alpha);

    let (noise_value, gradient_x, gradient_y, gradient_z) =
        lpfx_psrdnoise3(x_vec, period_vec, alpha_q32, seed);

    // Write gradient to output pointer
    unsafe {
        *gradient_out = gradient_x.to_fixed();
        *gradient_out.add(1) = gradient_y.to_fixed();
        *gradient_out.add(2) = gradient_z.to_fixed();
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
    fn test_psrdnoise3_basic() {
        let x = float_to_fixed(1.5);
        let y = float_to_fixed(2.3);
        let z = float_to_fixed(0.7);
        let period_x = float_to_fixed(0.0);
        let period_y = float_to_fixed(0.0);
        let period_z = float_to_fixed(0.0);
        let alpha = float_to_fixed(0.0);
        let mut gradient = [0i32; 3];

        let result = __lpfx_psrdnoise3_q32(
            x,
            y,
            z,
            period_x,
            period_y,
            period_z,
            alpha,
            gradient.as_mut_ptr(),
            0,
        );

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
        let grad_z = fixed_to_float(gradient[2]);
        assert!(
            grad_x >= -50.0 && grad_x <= 50.0,
            "Gradient x should be reasonable, got {}",
            grad_x
        );
        assert!(
            grad_y >= -50.0 && grad_y <= 50.0,
            "Gradient y should be reasonable, got {}",
            grad_y
        );
        assert!(
            grad_z >= -50.0 && grad_z <= 50.0,
            "Gradient z should be reasonable, got {}",
            grad_z
        );
    }

    #[test]
    fn test_psrdnoise3_periodic() {
        let x = float_to_fixed(1.5);
        let y = float_to_fixed(2.3);
        let z = float_to_fixed(0.7);
        let period_x = float_to_fixed(10.0);
        let period_y = float_to_fixed(10.0);
        let period_z = float_to_fixed(10.0);
        let alpha = float_to_fixed(0.0);
        let mut gradient = [0i32; 3];

        let result = __lpfx_psrdnoise3_q32(
            x,
            y,
            z,
            period_x,
            period_y,
            period_z,
            alpha,
            gradient.as_mut_ptr(),
            0,
        );

        // Should produce some value
        let result_float = fixed_to_float(result);
        assert!(
            result_float >= -2.0 && result_float <= 2.0,
            "Noise value should be in approximate range [-1, 1], got {}",
            result_float
        );
    }

    #[test]
    fn test_psrdnoise3_rotation() {
        let x = float_to_fixed(1.5);
        let y = float_to_fixed(2.3);
        let z = float_to_fixed(0.7);
        let period_x = float_to_fixed(0.0);
        let period_y = float_to_fixed(0.0);
        let period_z = float_to_fixed(0.0);
        let alpha1 = float_to_fixed(0.0);
        let alpha2 = float_to_fixed(1.57); // ~π/2
        let mut gradient1 = [0i32; 3];
        let mut gradient2 = [0i32; 3];

        let result1 = __lpfx_psrdnoise3_q32(
            x,
            y,
            z,
            period_x,
            period_y,
            period_z,
            alpha1,
            gradient1.as_mut_ptr(),
            0,
        );
        let result2 = __lpfx_psrdnoise3_q32(
            x,
            y,
            z,
            period_x,
            period_y,
            period_z,
            alpha2,
            gradient2.as_mut_ptr(),
            0,
        );

        // Different rotation angles should produce different results
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
    fn test_psrdnoise3_deterministic() {
        let x = float_to_fixed(42.5);
        let y = float_to_fixed(37.3);
        let z = float_to_fixed(25.1);
        let period_x = float_to_fixed(0.0);
        let period_y = float_to_fixed(0.0);
        let period_z = float_to_fixed(0.0);
        let alpha = float_to_fixed(0.5);
        let mut gradient1 = [0i32; 3];
        let mut gradient2 = [0i32; 3];

        let result1 = __lpfx_psrdnoise3_q32(
            x,
            y,
            z,
            period_x,
            period_y,
            period_z,
            alpha,
            gradient1.as_mut_ptr(),
            0,
        );
        let result2 = __lpfx_psrdnoise3_q32(
            x,
            y,
            z,
            period_x,
            period_y,
            period_z,
            alpha,
            gradient2.as_mut_ptr(),
            0,
        );

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
        assert_eq!(
            gradient1[2], gradient2[2],
            "Gradient z should be deterministic"
        );
    }
}
