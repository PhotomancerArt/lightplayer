use core::ops::{Add, Div, Mul, Neg, Sub};

use crate::builtins::q32::__lp_q32_sqrt;
use crate::glsl::q32::fns;
use crate::glsl::q32::types::q32::Q32;
use crate::glsl::q32::types::vec2_q32::Vec2Q32;
use crate::glsl::q32::types::vec3_q32::Vec3Q32;

/// 4D vector for Q32 fixed-point arithmetic (useful for RGBA colors and homogeneous coordinates)
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Vec4Q32 {
    pub x: Q32,
    pub y: Q32,
    pub z: Q32,
    pub w: Q32,
}

impl Vec4Q32 {
    #[inline(always)]
    pub const fn new(x: Q32, y: Q32, z: Q32, w: Q32) -> Self {
        Vec4Q32 { x, y, z, w }
    }

    #[inline(always)]
    pub fn from_f32(x: f32, y: f32, z: f32, w: f32) -> Self {
        Vec4Q32 {
            x: Q32::from_f32(x),
            y: Q32::from_f32(y),
            z: Q32::from_f32(z),
            w: Q32::from_f32(w),
        }
    }

    #[inline(always)]
    pub fn from_i32(x: i32, y: i32, z: i32, w: i32) -> Self {
        Vec4Q32 {
            x: Q32::from_i32(x),
            y: Q32::from_i32(y),
            z: Q32::from_i32(z),
            w: Q32::from_i32(w),
        }
    }

    #[inline(always)]
    pub const fn zero() -> Self {
        Vec4Q32::new(Q32::ZERO, Q32::ZERO, Q32::ZERO, Q32::ZERO)
    }

    #[inline(always)]
    pub const fn one() -> Self {
        Vec4Q32::new(Q32::ONE, Q32::ONE, Q32::ONE, Q32::ONE)
    }

    /// Dot product
    #[inline(always)]
    pub fn dot(self, rhs: Self) -> Q32 {
        (self.x * rhs.x) + (self.y * rhs.y) + (self.z * rhs.z) + (self.w * rhs.w)
    }

    /// Length squared (avoids sqrt)
    #[inline(always)]
    pub fn length_squared(self) -> Q32 {
        self.dot(self)
    }

    /// Length
    #[inline(always)]
    pub fn length(self) -> Q32 {
        let len_sq = self.length_squared();
        Q32::from_fixed(__lp_q32_sqrt(len_sq.to_fixed()))
    }

    /// Distance between two vectors
    #[inline(always)]
    pub fn distance(self, other: Self) -> Q32 {
        (self - other).length()
    }

    /// Normalize (returns zero vector if length is zero)
    #[inline(always)]
    pub fn normalize(self) -> Self {
        let len = self.length();
        if len.to_fixed() == 0 {
            return Vec4Q32::zero();
        }
        self / len
    }

    // Swizzle accessors (GLSL-style) - scalar
    #[inline(always)]
    pub fn x(self) -> Q32 {
        self.x
    }

    #[inline(always)]
    pub fn y(self) -> Q32 {
        self.y
    }

    #[inline(always)]
    pub fn z(self) -> Q32 {
        self.z
    }

    #[inline(always)]
    pub fn w(self) -> Q32 {
        self.w
    }

    #[inline(always)]
    pub fn r(self) -> Q32 {
        self.x
    }

    #[inline(always)]
    pub fn g(self) -> Q32 {
        self.y
    }

    #[inline(always)]
    pub fn b(self) -> Q32 {
        self.z
    }

    #[inline(always)]
    pub fn a(self) -> Q32 {
        self.w
    }

    // 2-component swizzles (most common)
    #[inline(always)]
    pub fn xy(self) -> Vec2Q32 {
        Vec2Q32::new(self.x, self.y)
    }

    #[inline(always)]
    pub fn xz(self) -> Vec2Q32 {
        Vec2Q32::new(self.x, self.z)
    }

    #[inline(always)]
    pub fn xw(self) -> Vec2Q32 {
        Vec2Q32::new(self.x, self.w)
    }

    #[inline(always)]
    pub fn yz(self) -> Vec2Q32 {
        Vec2Q32::new(self.y, self.z)
    }

    #[inline(always)]
    pub fn yw(self) -> Vec2Q32 {
        Vec2Q32::new(self.y, self.w)
    }

    #[inline(always)]
    pub fn zw(self) -> Vec2Q32 {
        Vec2Q32::new(self.z, self.w)
    }

    // 3-component swizzles (most common)
    #[inline(always)]
    pub fn xyz(self) -> Vec3Q32 {
        Vec3Q32::new(self.x, self.y, self.z)
    }

    #[inline(always)]
    pub fn xyw(self) -> Vec3Q32 {
        Vec3Q32::new(self.x, self.y, self.w)
    }

    #[inline(always)]
    pub fn xzw(self) -> Vec3Q32 {
        Vec3Q32::new(self.x, self.z, self.w)
    }

    #[inline(always)]
    pub fn yzw(self) -> Vec3Q32 {
        Vec3Q32::new(self.y, self.z, self.w)
    }

    // 4-component swizzle (identity)
    #[inline(always)]
    pub fn xyzw(self) -> Vec4Q32 {
        self
    }

    // RGBA variants
    #[inline(always)]
    pub fn rg(self) -> Vec2Q32 {
        self.xy()
    }

    #[inline(always)]
    pub fn rb(self) -> Vec2Q32 {
        self.xz()
    }

    #[inline(always)]
    pub fn rgb(self) -> Vec3Q32 {
        self.xyz()
    }

    #[inline(always)]
    pub fn rgba(self) -> Vec4Q32 {
        self
    }

    /// Component-wise multiply
    #[inline(always)]
    pub fn mul_comp(self, rhs: Self) -> Self {
        Vec4Q32::new(
            self.x * rhs.x,
            self.y * rhs.y,
            self.z * rhs.z,
            self.w * rhs.w,
        )
    }

    /// Component-wise divide
    #[inline(always)]
    pub fn div_comp(self, rhs: Self) -> Self {
        Vec4Q32::new(
            self.x / rhs.x,
            self.y / rhs.y,
            self.z / rhs.z,
            self.w / rhs.w,
        )
    }

    /// Clamp components between min and max
    #[inline(always)]
    pub fn clamp(self, min: Q32, max: Q32) -> Self {
        Vec4Q32::new(
            self.x.clamp(min, max),
            self.y.clamp(min, max),
            self.z.clamp(min, max),
            self.w.clamp(min, max),
        )
    }

    /// Component-wise floor
    #[inline(always)]
    pub fn floor(self) -> Self {
        fns::floor_vec4(self)
    }

    /// Component-wise fractional part
    #[inline(always)]
    pub fn fract(self) -> Self {
        fns::fract_vec4(self)
    }

    /// Component-wise step function
    /// Returns 1.0 if edge <= x, else 0.0 for each component
    #[inline(always)]
    pub fn step(self, edge: Self) -> Self {
        fns::step_vec4(edge, self)
    }

    /// Component-wise minimum
    #[inline(always)]
    pub fn min(self, other: Self) -> Self {
        fns::min_vec4(self, other)
    }

    /// Component-wise maximum
    #[inline(always)]
    pub fn max(self, other: Self) -> Self {
        fns::max_vec4(self, other)
    }

    /// Component-wise modulo
    #[inline(always)]
    pub fn modulo(self, other: Self) -> Self {
        fns::mod_vec4(self, other)
    }

    /// Modulo with scalar
    #[inline(always)]
    pub fn modulo_scalar(self, y: Q32) -> Self {
        fns::mod_vec4_scalar(self, y)
    }

    /// Component-wise linear interpolation
    /// Returns a + t * (b - a) for each component
    #[inline(always)]
    pub fn mix(self, other: Self, t: Self) -> Self {
        fns::mix_vec4(self, other, t)
    }

    /// Create Vec4 from Vec3 + scalar
    /// Useful for patterns like vec4(v0.x, v1.x, v2.x, v3.x)
    #[inline(always)]
    pub fn from_vec3_scalar(v: Vec3Q32, w: Q32) -> Self {
        Vec4Q32::new(v.x, v.y, v.z, w)
    }
}

// Vector + Vector
impl Add for Vec4Q32 {
    type Output = Self;

    #[inline(always)]
    fn add(self, rhs: Self) -> Self {
        Vec4Q32::new(
            self.x + rhs.x,
            self.y + rhs.y,
            self.z + rhs.z,
            self.w + rhs.w,
        )
    }
}

// Vector - Vector
impl Sub for Vec4Q32 {
    type Output = Self;

    #[inline(always)]
    fn sub(self, rhs: Self) -> Self {
        Vec4Q32::new(
            self.x - rhs.x,
            self.y - rhs.y,
            self.z - rhs.z,
            self.w - rhs.w,
        )
    }
}

// Vector * Scalar
impl Mul<Q32> for Vec4Q32 {
    type Output = Self;

    #[inline(always)]
    fn mul(self, rhs: Q32) -> Self {
        Vec4Q32::new(self.x * rhs, self.y * rhs, self.z * rhs, self.w * rhs)
    }
}

// Vector / Scalar
impl Div<Q32> for Vec4Q32 {
    type Output = Self;

    #[inline(always)]
    fn div(self, rhs: Q32) -> Self {
        Vec4Q32::new(self.x / rhs, self.y / rhs, self.z / rhs, self.w / rhs)
    }
}

impl Neg for Vec4Q32 {
    type Output = Self;

    #[inline(always)]
    fn neg(self) -> Self {
        Vec4Q32::new(-self.x, -self.y, -self.z, -self.w)
    }
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    extern crate std;
    use super::*;

    #[test]
    fn test_new() {
        let v = Vec4Q32::new(
            Q32::from_i32(1),
            Q32::from_i32(2),
            Q32::from_i32(3),
            Q32::from_i32(4),
        );
        assert_eq!(v.x.to_f32(), 1.0);
        assert_eq!(v.y.to_f32(), 2.0);
        assert_eq!(v.z.to_f32(), 3.0);
        assert_eq!(v.w.to_f32(), 4.0);
    }

    #[test]
    fn test_from_f32() {
        let v = Vec4Q32::from_f32(1.0, 2.0, 3.0, 4.0);
        assert_eq!(v.x.to_f32(), 1.0);
        assert_eq!(v.y.to_f32(), 2.0);
        assert_eq!(v.z.to_f32(), 3.0);
        assert_eq!(v.w.to_f32(), 4.0);
    }

    #[test]
    fn test_from_i32() {
        let v = Vec4Q32::from_i32(5, 10, 15, 20);
        assert_eq!(v.x.to_f32(), 5.0);
        assert_eq!(v.y.to_f32(), 10.0);
        assert_eq!(v.z.to_f32(), 15.0);
        assert_eq!(v.w.to_f32(), 20.0);
    }

    #[test]
    fn test_zero_one() {
        let z = Vec4Q32::zero();
        assert_eq!(z.x.to_f32(), 0.0);
        assert_eq!(z.y.to_f32(), 0.0);
        assert_eq!(z.z.to_f32(), 0.0);
        assert_eq!(z.w.to_f32(), 0.0);

        let o = Vec4Q32::one();
        assert_eq!(o.x.to_f32(), 1.0);
        assert_eq!(o.y.to_f32(), 1.0);
        assert_eq!(o.z.to_f32(), 1.0);
        assert_eq!(o.w.to_f32(), 1.0);
    }

    #[test]
    fn test_add() {
        let a = Vec4Q32::from_f32(1.0, 2.0, 3.0, 4.0);
        let b = Vec4Q32::from_f32(5.0, 6.0, 7.0, 8.0);
        let c = a + b;
        assert_eq!(c.x.to_f32(), 6.0);
        assert_eq!(c.y.to_f32(), 8.0);
        assert_eq!(c.z.to_f32(), 10.0);
        assert_eq!(c.w.to_f32(), 12.0);
    }

    #[test]
    fn test_sub() {
        let a = Vec4Q32::from_f32(5.0, 7.0, 9.0, 11.0);
        let b = Vec4Q32::from_f32(1.0, 2.0, 3.0, 4.0);
        let c = a - b;
        assert_eq!(c.x.to_f32(), 4.0);
        assert_eq!(c.y.to_f32(), 5.0);
        assert_eq!(c.z.to_f32(), 6.0);
        assert_eq!(c.w.to_f32(), 7.0);
    }

    #[test]
    fn test_mul_scalar() {
        let v = Vec4Q32::from_f32(1.0, 2.0, 3.0, 4.0);
        let s = Q32::from_f32(2.0);
        let result = v * s;
        assert_eq!(result.x.to_f32(), 2.0);
        assert_eq!(result.y.to_f32(), 4.0);
        assert_eq!(result.z.to_f32(), 6.0);
        assert_eq!(result.w.to_f32(), 8.0);
    }

    #[test]
    fn test_div_scalar() {
        let v = Vec4Q32::from_f32(4.0, 6.0, 8.0, 10.0);
        let s = Q32::from_f32(2.0);
        let result = v / s;
        assert_eq!(result.x.to_f32(), 2.0);
        assert_eq!(result.y.to_f32(), 3.0);
        assert_eq!(result.z.to_f32(), 4.0);
        assert_eq!(result.w.to_f32(), 5.0);
    }

    #[test]
    fn test_neg() {
        let v = Vec4Q32::from_f32(5.0, -3.0, 7.0, -9.0);
        let neg = -v;
        assert_eq!(neg.x.to_f32(), -5.0);
        assert_eq!(neg.y.to_f32(), 3.0);
        assert_eq!(neg.z.to_f32(), -7.0);
        assert_eq!(neg.w.to_f32(), 9.0);
    }

    #[test]
    fn test_dot() {
        let a = Vec4Q32::from_f32(1.0, 2.0, 3.0, 4.0);
        let b = Vec4Q32::from_f32(5.0, 6.0, 7.0, 8.0);
        let dot = a.dot(b);
        // 1*5 + 2*6 + 3*7 + 4*8 = 5 + 12 + 21 + 32 = 70
        assert_eq!(dot.to_f32(), 70.0);
    }

    #[test]
    fn test_length_squared() {
        let v = Vec4Q32::from_f32(1.0, 2.0, 2.0, 4.0);
        let len_sq = v.length_squared();
        // 1^2 + 2^2 + 2^2 + 4^2 = 1 + 4 + 4 + 16 = 25
        assert_eq!(len_sq.to_f32(), 25.0);
    }

    #[test]
    fn test_length() {
        let v = Vec4Q32::from_f32(3.0, 0.0, 4.0, 0.0);
        let len = v.length();
        // Length should be 5
        assert!((len.to_f32() - 5.0).abs() < 0.01);
    }

    #[test]
    fn test_normalize() {
        let v = Vec4Q32::from_f32(3.0, 0.0, 4.0, 0.0);
        let n = v.normalize();

        // Check length is approximately 1
        let len = n.length();
        assert!((len.to_f32() - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_normalize_zero() {
        let v = Vec4Q32::zero();
        let n = v.normalize();
        // Should return zero vector, not panic
        assert_eq!(n.x.to_f32(), 0.0);
        assert_eq!(n.y.to_f32(), 0.0);
        assert_eq!(n.z.to_f32(), 0.0);
        assert_eq!(n.w.to_f32(), 0.0);
    }

    #[test]
    fn test_distance() {
        let a = Vec4Q32::from_f32(0.0, 0.0, 0.0, 0.0);
        let b = Vec4Q32::from_f32(1.0, 2.0, 2.0, 4.0);
        let dist = a.distance(b);
        // Distance should be 5
        assert!((dist.to_f32() - 5.0).abs() < 0.01);
    }

    #[test]
    fn test_mul_comp() {
        let a = Vec4Q32::from_f32(2.0, 3.0, 4.0, 5.0);
        let b = Vec4Q32::from_f32(6.0, 7.0, 8.0, 9.0);
        let c = a.mul_comp(b);
        assert_eq!(c.x.to_f32(), 12.0);
        assert_eq!(c.y.to_f32(), 21.0);
        assert_eq!(c.z.to_f32(), 32.0);
        assert_eq!(c.w.to_f32(), 45.0);
    }

    #[test]
    fn test_div_comp() {
        let a = Vec4Q32::from_f32(12.0, 21.0, 32.0, 45.0);
        let b = Vec4Q32::from_f32(2.0, 3.0, 4.0, 5.0);
        let c = a.div_comp(b);
        assert_eq!(c.x.to_f32(), 6.0);
        assert_eq!(c.y.to_f32(), 7.0);
        assert_eq!(c.z.to_f32(), 8.0);
        assert_eq!(c.w.to_f32(), 9.0);
    }

    #[test]
    fn test_clamp() {
        let v = Vec4Q32::from_f32(-1.0, 0.5, 1.5, 2.0);
        let min = Q32::from_f32(0.0);
        let max = Q32::from_f32(1.0);
        let clamped = v.clamp(min, max);
        assert_eq!(clamped.x.to_f32(), 0.0);
        assert_eq!(clamped.y.to_f32(), 0.5);
        assert_eq!(clamped.z.to_f32(), 1.0);
        assert_eq!(clamped.w.to_f32(), 1.0);
    }

    #[test]
    fn test_swizzle_scalar() {
        let v = Vec4Q32::from_f32(1.0, 2.0, 3.0, 4.0);
        assert_eq!(v.x().to_f32(), 1.0);
        assert_eq!(v.y().to_f32(), 2.0);
        assert_eq!(v.z().to_f32(), 3.0);
        assert_eq!(v.w().to_f32(), 4.0);
        assert_eq!(v.r().to_f32(), 1.0);
        assert_eq!(v.g().to_f32(), 2.0);
        assert_eq!(v.b().to_f32(), 3.0);
        assert_eq!(v.a().to_f32(), 4.0);
    }

    #[test]
    fn test_swizzle_xy() {
        let v = Vec4Q32::from_f32(1.0, 2.0, 3.0, 4.0);
        let xy = v.xy();
        assert_eq!(xy.x.to_f32(), 1.0);
        assert_eq!(xy.y.to_f32(), 2.0);
    }

    #[test]
    fn test_swizzle_xyz() {
        let v = Vec4Q32::from_f32(1.0, 2.0, 3.0, 4.0);
        let xyz = v.xyz();
        assert_eq!(xyz.x.to_f32(), 1.0);
        assert_eq!(xyz.y.to_f32(), 2.0);
        assert_eq!(xyz.z.to_f32(), 3.0);
    }

    #[test]
    fn test_swizzle_xyzw() {
        let v = Vec4Q32::from_f32(1.0, 2.0, 3.0, 4.0);
        let xyzw = v.xyzw();
        assert_eq!(xyzw.x.to_f32(), 1.0);
        assert_eq!(xyzw.y.to_f32(), 2.0);
        assert_eq!(xyzw.z.to_f32(), 3.0);
        assert_eq!(xyzw.w.to_f32(), 4.0);
    }

    #[test]
    fn test_swizzle_rgba() {
        let v = Vec4Q32::from_f32(1.0, 2.0, 3.0, 4.0);
        assert_eq!(v.rg().x.to_f32(), 1.0);
        assert_eq!(v.rg().y.to_f32(), 2.0);
        assert_eq!(v.rb().x.to_f32(), 1.0);
        assert_eq!(v.rb().y.to_f32(), 3.0);
        assert_eq!(v.rgb().x.to_f32(), 1.0);
        assert_eq!(v.rgb().y.to_f32(), 2.0);
        assert_eq!(v.rgb().z.to_f32(), 3.0);
        assert_eq!(v.rgba().x.to_f32(), 1.0);
        assert_eq!(v.rgba().y.to_f32(), 2.0);
        assert_eq!(v.rgba().z.to_f32(), 3.0);
        assert_eq!(v.rgba().w.to_f32(), 4.0);
    }
}
