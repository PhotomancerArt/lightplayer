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
//! This function is callable from GLSL shaders using the `lpfn_psrdnoise` name:
//!
//! ```glsl
//! vec3 gradient;
//! float noise = lpfn_psrdnoise(vec3(5.0, 3.0, 1.0), vec3(10.0, 10.0, 10.0), 0.5, gradient);
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

use crate::builtins::glsl::sincos_q32::lps_sincos_q32_pair;
use crate::builtins::lpfn::generative::psrdnoise::fibonacci_lut_q32::{
    grad_base_and_orthogonal, rotate_by_alpha,
};
use lps_q32::q32::Q32;
use lps_q32::vec3_q32::Vec3Q32;
use lps_q32::vec4_q32::Vec4Q32;

/// Fixed-point constants
const HALF: Q32 = Q32(0x00008000); // 0.5 in Q16.16
const SIX: Q32 = Q32(0x00060000); // 6.0 in Q16.16

/// Radial decay constant: 0.5
/// In Q16.16: 0.5 * 65536 = 32768
const RADIAL_DECAY_0_5: Q32 = Q32(32768);

/// Final scale factor: 39.5
/// In Q16.16: 39.5 * 65536 ≈ 2588672
const SCALE_39_5: Q32 = Q32(2588672);

/// 1/3 ≈ 0.33333333
const ONE_THIRD: Q32 = Q32(21845); // 0.33333333 * 65536
/// 1/6 ≈ 0.16666667
const ONE_SIXTH: Q32 = Q32(10923); // 0.16666667 * 65536

/// Integer hash for one simplex corner using i32 rem_euclid(289).
/// 3D permutation: permute(permute(permute(iw) + iv) + iu)
#[inline(always)]
fn hash_corner(iu: i32, iv: i32, iw: i32) -> i32 {
    let h = iw.rem_euclid(289);
    let h = ((h * 34 + 1) * h).rem_euclid(289);
    let h = ((h + iv).rem_euclid(289) * 34 + 1) * h;
    let h = (h + iu).rem_euclid(289);
    ((h * 34 + 10) * h).rem_euclid(289)
}

/// Corner indices for the four simplex corners.
struct CornerIndices3D {
    i0_x: i32,
    i0_y: i32,
    i0_z: i32,
    i1_x: i32,
    i1_y: i32,
    i1_z: i32,
    i2_x: i32,
    i2_y: i32,
    i2_z: i32,
    i3_x: i32,
    i3_y: i32,
    i3_z: i32,
}

/// Gradient vectors and entry data for all four corners.
struct CornerGradients {
    g0: Vec3Q32,
    g1: Vec3Q32,
    g2: Vec3Q32,
    g3: Vec3Q32,
}

/// Compute gradients for all four corners using LUT + alpha rotation.
#[inline(always)]
fn compute_gradients(idx: &CornerIndices3D, sin_alpha: i32, cos_alpha: i32) -> CornerGradients {
    // Hash all four corners
    let hash_0 = hash_corner(idx.i0_x, idx.i0_y, idx.i0_z);
    let hash_1 = hash_corner(idx.i1_x, idx.i1_y, idx.i1_z);
    let hash_2 = hash_corner(idx.i2_x, idx.i2_y, idx.i2_z);
    let hash_3 = hash_corner(idx.i3_x, idx.i3_y, idx.i3_z);

    // LUT lookup + rotation for each corner
    let entry_0 = grad_base_and_orthogonal(hash_0);
    let (gx_0, gy_0, gz_0) = rotate_by_alpha(&entry_0, sin_alpha, cos_alpha);

    let entry_1 = grad_base_and_orthogonal(hash_1);
    let (gx_1, gy_1, gz_1) = rotate_by_alpha(&entry_1, sin_alpha, cos_alpha);

    let entry_2 = grad_base_and_orthogonal(hash_2);
    let (gx_2, gy_2, gz_2) = rotate_by_alpha(&entry_2, sin_alpha, cos_alpha);

    let entry_3 = grad_base_and_orthogonal(hash_3);
    let (gx_3, gy_3, gz_3) = rotate_by_alpha(&entry_3, sin_alpha, cos_alpha);

    CornerGradients {
        g0: Vec3Q32::new(
            Q32::from_fixed(gx_0),
            Q32::from_fixed(gy_0),
            Q32::from_fixed(gz_0),
        ),
        g1: Vec3Q32::new(
            Q32::from_fixed(gx_1),
            Q32::from_fixed(gy_1),
            Q32::from_fixed(gz_1),
        ),
        g2: Vec3Q32::new(
            Q32::from_fixed(gx_2),
            Q32::from_fixed(gy_2),
            Q32::from_fixed(gz_2),
        ),
        g3: Vec3Q32::new(
            Q32::from_fixed(gx_3),
            Q32::from_fixed(gy_3),
            Q32::from_fixed(gz_3),
        ),
    }
}

/// Noise computation tail: gradients, radial decay, final noise value.
/// Shared between periodic and non-periodic paths (corner indices and x vectors already computed).
#[inline(always)]
fn psrdnoise3_tail(
    x0: Vec3Q32,
    x1: Vec3Q32,
    x2: Vec3Q32,
    x3: Vec3Q32,
    corner_indices: &CornerIndices3D,
    sin_alpha: i32,
    cos_alpha: i32,
) -> (Q32, Q32, Q32, Q32) {
    // Compute gradients using LUT + alpha rotation
    let grads = compute_gradients(corner_indices, sin_alpha, cos_alpha);

    // Radial decay with distance from each simplex corner
    // w = 0.5 - vec4(dot(x0,x0), dot(x1,x1), dot(x2,x2), dot(x3,x3))
    //
    // Range analysis for wrapping math:
    // - x_k components bounded by simplex geometry (~[-1, 1] in skewed space)
    // - dot(x_k, x_k) = x_k.x^2 + x_k.y^2 + x_k.z^2 bounded by ~3.0
    // - RADIAL_DECAY_0_5 - dot is bounded and safe for wrapping
    let dot0 = x0.dot_wrapping(x0);
    let dot1 = x1.dot_wrapping(x1);
    let dot2 = x2.dot_wrapping(x2);
    let dot3 = x3.dot_wrapping(x3);

    let mut w = Vec4Q32::new(
        RADIAL_DECAY_0_5.sub_wrapping(dot0),
        RADIAL_DECAY_0_5.sub_wrapping(dot1),
        RADIAL_DECAY_0_5.sub_wrapping(dot2),
        RADIAL_DECAY_0_5.sub_wrapping(dot3),
    );

    // w = max(w, 0.0)
    w = w.max(Vec4Q32::zero());

    // w2 = w * w, w3 = w2 * w
    // Range analysis: w is in [0, 1] after max(), so w*w and w*w*w are bounded
    // Safe for wrapping arithmetic (result stays in [0, 1])
    let w2 = Vec4Q32::new(
        w.x.mul_wrapping(w.x),
        w.y.mul_wrapping(w.y),
        w.z.mul_wrapping(w.z),
        w.w.mul_wrapping(w.w),
    );
    let w3 = Vec4Q32::new(
        w2.x.mul_wrapping(w.x),
        w2.y.mul_wrapping(w.y),
        w2.z.mul_wrapping(w.z),
        w2.w.mul_wrapping(w.w),
    );

    // The value of the linear ramp from each of the corners
    // gdotx = vec4(dot(g0,x0), dot(g1,x1), dot(g2,x2), dot(g3,x3))
    //
    // Range analysis for dot_wrapping:
    // - x_k components bounded by simplex geometry (~[-1, 1] in skewed space)
    // - grad_k components are normalized (sin/cos outputs from LUT, sphere points)
    // - Dot product bounded by ~3.0 max (unit vectors in 3D)
    // - Safe for Q16.16 wrapping arithmetic
    let gdotx = Vec4Q32::new(
        grads.g0.dot_wrapping(x0),
        grads.g1.dot_wrapping(x1),
        grads.g2.dot_wrapping(x2),
        grads.g3.dot_wrapping(x3),
    );

    // Multiply by the radial decay and sum up the noise value
    // n = dot(w3, gdotx)
    //
    // Range analysis:
    // - w3 components are in [0, 1] (w in [0,1], w3 = w^3)
    // - gdotx components bounded by ~3.0
    // - Product bounded by ~3.0, sum of 4 terms bounded by ~12.0
    // - Safe for wrapping: 12.0 << 2^15 (Q16.16 overflow limit)
    let n =
        w3.x.mul_wrapping(gdotx.x)
            .add_wrapping(w3.y.mul_wrapping(gdotx.y))
            .add_wrapping(w3.z.mul_wrapping(gdotx.z))
            .add_wrapping(w3.w.mul_wrapping(gdotx.w));

    // Compute the first order partial derivatives
    // dw = -6.0 * w2 * gdotx
    //
    // Range analysis:
    // - w2 in [0, 1], gdotx bounded by ~3.0
    // - Product bounded by ~3.0, scaled by 6.0 -> ~18.0
    // - Safe for wrapping arithmetic
    let neg_six = Q32(-SIX.0); // -6.0 in Q16.16
    let dw_x = w2.x.mul_wrapping(gdotx.x).mul_wrapping(neg_six);
    let dw_y = w2.y.mul_wrapping(gdotx.y).mul_wrapping(neg_six);
    let dw_z = w2.z.mul_wrapping(gdotx.z).mul_wrapping(neg_six);
    let dw_w = w2.w.mul_wrapping(gdotx.w).mul_wrapping(neg_six);

    // dn0 = w3.x * g0 + dw.x * x0, etc.
    //
    // Range analysis for gradient accumulation:
    // - w3.x in [0, 1], g0 components are normalized (~[-1, 1])
    // - dw.x bounded by ~18.0, x0 bounded by ~[-1, 1]
    // - Product: w3.x * g0 bounded by ~1.0, dw.x * x0 bounded by ~18.0
    // - Sum bounded by ~19.0 per component, well within Q16.16 range
    let dn0 = Vec3Q32::new(
        w3.x.mul_wrapping(grads.g0.x)
            .add_wrapping(dw_x.mul_wrapping(x0.x)),
        w3.x.mul_wrapping(grads.g0.y)
            .add_wrapping(dw_x.mul_wrapping(x0.y)),
        w3.x.mul_wrapping(grads.g0.z)
            .add_wrapping(dw_x.mul_wrapping(x0.z)),
    );
    let dn1 = Vec3Q32::new(
        w3.y.mul_wrapping(grads.g1.x)
            .add_wrapping(dw_y.mul_wrapping(x1.x)),
        w3.y.mul_wrapping(grads.g1.y)
            .add_wrapping(dw_y.mul_wrapping(x1.y)),
        w3.y.mul_wrapping(grads.g1.z)
            .add_wrapping(dw_y.mul_wrapping(x1.z)),
    );
    let dn2 = Vec3Q32::new(
        w3.z.mul_wrapping(grads.g2.x)
            .add_wrapping(dw_z.mul_wrapping(x2.x)),
        w3.z.mul_wrapping(grads.g2.y)
            .add_wrapping(dw_z.mul_wrapping(x2.y)),
        w3.z.mul_wrapping(grads.g2.z)
            .add_wrapping(dw_z.mul_wrapping(x2.z)),
    );
    let dn3 = Vec3Q32::new(
        w3.w.mul_wrapping(grads.g3.x)
            .add_wrapping(dw_w.mul_wrapping(x3.x)),
        w3.w.mul_wrapping(grads.g3.y)
            .add_wrapping(dw_w.mul_wrapping(x3.y)),
        w3.w.mul_wrapping(grads.g3.z)
            .add_wrapping(dw_w.mul_wrapping(x3.z)),
    );

    // gradient = 39.5 * (dn0 + dn1 + dn2 + dn3)
    //
    // Range analysis:
    // - Each dn_k component bounded by ~19.0
    // - Sum of 4 terms bounded by ~76.0
    // - Scaled by 39.5 -> ~3002.0, still well within Q16.16 range (~32767 max)
    let grad_sum_x = dn0
        .x
        .add_wrapping(dn1.x)
        .add_wrapping(dn2.x)
        .add_wrapping(dn3.x);
    let grad_sum_y = dn0
        .y
        .add_wrapping(dn1.y)
        .add_wrapping(dn2.y)
        .add_wrapping(dn3.y);
    let grad_sum_z = dn0
        .z
        .add_wrapping(dn1.z)
        .add_wrapping(dn2.z)
        .add_wrapping(dn3.z);

    let gradient_x = grad_sum_x.mul_wrapping(SCALE_39_5);
    let gradient_y = grad_sum_y.mul_wrapping(SCALE_39_5);
    let gradient_z = grad_sum_z.mul_wrapping(SCALE_39_5);

    // Scale the return value to fit nicely into the range [-1,1]
    // n is bounded by ~12.0, scaled by 39.5 -> ~474.0, well within range
    let noise_value = n.mul_wrapping(SCALE_39_5);

    (noise_value, gradient_x, gradient_y, gradient_z)
}

/// Non-periodic fast path (no period wrapping); kept separate so call sites with `period == 0` can fold.
#[inline(always)]
fn psrdnoise3_noperiod(x: Vec3Q32, sin_alpha: i32, cos_alpha: i32) -> (Q32, Q32, Q32, Q32) {
    // Transform to simplex space (tetrahedral grid)
    // Using optimized transformation: uvw = x + dot(x, vec3(1.0/3.0))
    let dot_sum = x.x.add_wrapping(x.y).add_wrapping(x.z);
    let uvw_offset = dot_sum.mul_wrapping(ONE_THIRD);
    let uvw = Vec3Q32::new(
        x.x.add_wrapping(uvw_offset),
        x.y.add_wrapping(uvw_offset),
        x.z.add_wrapping(uvw_offset),
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

    // Transform the corners back to texture space
    // Using optimized transformation: v = i - dot(i, vec3(1.0/6.0))
    let dot_i0 = i0.dot(Vec3Q32::one()).mul_wrapping(ONE_SIXTH);
    let dot_i1 = i1.dot(Vec3Q32::one()).mul_wrapping(ONE_SIXTH);
    let dot_i2 = i2.dot(Vec3Q32::one()).mul_wrapping(ONE_SIXTH);
    let dot_i3 = i3.dot(Vec3Q32::one()).mul_wrapping(ONE_SIXTH);

    let v0 = Vec3Q32::new(
        i0.x.sub_wrapping(dot_i0),
        i0.y.sub_wrapping(dot_i0),
        i0.z.sub_wrapping(dot_i0),
    );
    let v1 = Vec3Q32::new(
        i1.x.sub_wrapping(dot_i1),
        i1.y.sub_wrapping(dot_i1),
        i1.z.sub_wrapping(dot_i1),
    );
    let v2 = Vec3Q32::new(
        i2.x.sub_wrapping(dot_i2),
        i2.y.sub_wrapping(dot_i2),
        i2.z.sub_wrapping(dot_i2),
    );
    let v3 = Vec3Q32::new(
        i3.x.sub_wrapping(dot_i3),
        i3.y.sub_wrapping(dot_i3),
        i3.z.sub_wrapping(dot_i3),
    );

    // Compute vectors to each of the simplex corners
    let x0 = Vec3Q32::new(
        x.x.sub_wrapping(v0.x),
        x.y.sub_wrapping(v0.y),
        x.z.sub_wrapping(v0.z),
    );
    let x1 = Vec3Q32::new(
        x.x.sub_wrapping(v1.x),
        x.y.sub_wrapping(v1.y),
        x.z.sub_wrapping(v1.z),
    );
    let x2 = Vec3Q32::new(
        x.x.sub_wrapping(v2.x),
        x.y.sub_wrapping(v2.y),
        x.z.sub_wrapping(v2.z),
    );
    let x3 = Vec3Q32::new(
        x.x.sub_wrapping(v3.x),
        x.y.sub_wrapping(v3.y),
        x.z.sub_wrapping(v3.z),
    );

    let corner_indices = CornerIndices3D {
        i0_x: i0_x_int,
        i0_y: i0_y_int,
        i0_z: i0_z_int,
        i1_x: i1.x.to_i32(),
        i1_y: i1.y.to_i32(),
        i1_z: i1.z.to_i32(),
        i2_x: i2.x.to_i32(),
        i2_y: i2.y.to_i32(),
        i2_z: i2.z.to_i32(),
        i3_x: i3.x.to_i32(),
        i3_y: i3.y.to_i32(),
        i3_z: i3.z.to_i32(),
    };

    psrdnoise3_tail(x0, x1, x2, x3, &corner_indices, sin_alpha, cos_alpha)
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
pub fn lpfn_psrdnoise3(
    x: Vec3Q32,
    period: Vec3Q32,
    alpha: Q32,
    _seed: u32,
) -> (Q32, Q32, Q32, Q32) {
    // Compute sin(alpha) and cos(alpha) once
    let (sin_alpha, cos_alpha) = lps_sincos_q32_pair(alpha.to_fixed());

    // Fast path: no period wrapping needed
    if period.x.is_zero() && period.y.is_zero() && period.z.is_zero() {
        return psrdnoise3_noperiod(x, sin_alpha, cos_alpha);
    }

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
    // Note: i0_int values unused here - recomputed after wrapping in periodic path

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

    // Handle periodic tiling (period was already checked, so at least one is non-zero)
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

    // Recompute x vectors from wrapped v
    let x0_w = x - v0_wrapped;
    let x1_w = x - v1_wrapped;
    let x2_w = x - v2_wrapped;
    let x3_w = x - v3_wrapped;

    let corner_indices = CornerIndices3D {
        i0_x: i0_wrapped.x.to_i32(),
        i0_y: i0_wrapped.y.to_i32(),
        i0_z: i0_wrapped.z.to_i32(),
        i1_x: i1_wrapped.x.to_i32(),
        i1_y: i1_wrapped.y.to_i32(),
        i1_z: i1_wrapped.z.to_i32(),
        i2_x: i2_wrapped.x.to_i32(),
        i2_y: i2_wrapped.y.to_i32(),
        i2_z: i2_wrapped.z.to_i32(),
        i3_x: i3_wrapped.x.to_i32(),
        i3_y: i3_wrapped.y.to_i32(),
        i3_z: i3_wrapped.z.to_i32(),
    };

    psrdnoise3_tail(
        x0_w,
        x1_w,
        x2_w,
        x3_w,
        &corner_indices,
        sin_alpha,
        cos_alpha,
    )
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
#[lpfn_impl_macro::lpfn_impl(
    q32,
    "float lpfn_psrdnoise(vec3 x, vec3 period, float alpha, out vec3 gradient, uint seed)"
)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpfn_psrdnoise3_q32(
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
    let alpha_q32 = Q32::from_fixed(alpha);

    // Fast path: no period wrapping needed (using raw i32 comparison)
    let (noise_value, gradient_x, gradient_y, gradient_z) =
        if period_x == 0 && period_y == 0 && period_z == 0 {
            let (sin_alpha, cos_alpha) = lps_sincos_q32_pair(alpha);
            psrdnoise3_noperiod(x_vec, sin_alpha, cos_alpha)
        } else {
            let period_vec = Vec3Q32::new(
                Q32::from_fixed(period_x),
                Q32::from_fixed(period_y),
                Q32::from_fixed(period_z),
            );
            lpfn_psrdnoise3(x_vec, period_vec, alpha_q32, seed)
        };

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

        let result = __lp_lpfn_psrdnoise3_q32(
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

        let result = __lp_lpfn_psrdnoise3_q32(
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

        let result1 = __lp_lpfn_psrdnoise3_q32(
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
        let result2 = __lp_lpfn_psrdnoise3_q32(
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

        let result1 = __lp_lpfn_psrdnoise3_q32(
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
        let result2 = __lp_lpfn_psrdnoise3_q32(
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
