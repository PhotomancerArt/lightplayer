use core::ops::{Add, Div, Mul, Neg, Sub};

use crate::builtins::q32::__lp_q32_sqrt;
use crate::glsl::q32::fns;
use crate::glsl::q32::types::q32::Q32;

/// 2D vector for Q32 fixed-point arithmetic
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Vec2Q32 {
    pub x: Q32,
    pub y: Q32,
}

impl Vec2Q32 {
    #[inline(always)]
    pub const fn new(x: Q32, y: Q32) -> Self {
        Vec2Q32 { x, y }
    }

    #[inline(always)]
    pub fn from_f32(x: f32, y: f32) -> Self {
        Vec2Q32 {
            x: Q32::from_f32(x),
            y: Q32::from_f32(y),
        }
    }

    #[inline(always)]
    pub fn from_i32(x: i32, y: i32) -> Self {
        Vec2Q32 {
            x: Q32::from_i32(x),
            y: Q32::from_i32(y),
        }
    }

    #[inline(always)]
    pub const fn zero() -> Self {
        Vec2Q32::new(Q32::ZERO, Q32::ZERO)
    }

    #[inline(always)]
    pub const fn one() -> Self {
        Vec2Q32::new(Q32::ONE, Q32::ONE)
    }

    /// Dot product
    #[inline(always)]
    pub fn dot(self, rhs: Self) -> Q32 {
        (self.x * rhs.x) + (self.y * rhs.y)
    }

    /// Cross product (returns scalar in 2D, representing z-component of 3D cross product)
    #[inline(always)]
    pub fn cross(self, rhs: Self) -> Q32 {
        (self.x * rhs.y) - (self.y * rhs.x)
    }

    /// Length squared (avoids sqrt)
    #[inline(always)]
    pub fn length_squared(self) -> Q32 {
        self.dot(self)
    }

    /// Length (magnitude)
    #[inline(always)]
    pub fn length(self) -> Q32 {
        let len_sq = self.length_squared();
        Q32::from_fixed(__lp_q32_sqrt(len_sq.to_fixed()))
    }

    /// Distance to another vector
    #[inline(always)]
    pub fn distance(self, other: Self) -> Q32 {
        (self - other).length()
    }

    /// Normalize to unit vector
    #[inline(always)]
    pub fn normalize(self) -> Self {
        let len = self.length();
        if len.to_fixed() == 0 {
            return Vec2Q32::zero();
        }
        self / len
    }

    // Swizzle accessors (GLSL-style)
    #[inline(always)]
    pub fn x(self) -> Q32 {
        self.x
    }

    #[inline(always)]
    pub fn y(self) -> Q32 {
        self.y
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
    pub fn s(self) -> Q32 {
        self.x
    }

    #[inline(always)]
    pub fn t(self) -> Q32 {
        self.y
    }

    // 2-component swizzles (most common)
    #[inline(always)]
    pub fn xx(self) -> Vec2Q32 {
        Vec2Q32::new(self.x, self.x)
    }

    #[inline(always)]
    pub fn xy(self) -> Vec2Q32 {
        self
    }

    // identity
    #[inline(always)]
    pub fn yx(self) -> Vec2Q32 {
        Vec2Q32::new(self.y, self.x)
    }

    #[inline(always)]
    pub fn yy(self) -> Vec2Q32 {
        Vec2Q32::new(self.y, self.y)
    }

    // RGBA variants
    #[inline(always)]
    pub fn rr(self) -> Vec2Q32 {
        self.xx()
    }

    #[inline(always)]
    pub fn rg(self) -> Vec2Q32 {
        self.xy()
    }

    #[inline(always)]
    pub fn gr(self) -> Vec2Q32 {
        self.yx()
    }

    #[inline(always)]
    pub fn gg(self) -> Vec2Q32 {
        self.yy()
    }

    // STPQ variants
    #[inline(always)]
    pub fn ss(self) -> Vec2Q32 {
        self.xx()
    }

    #[inline(always)]
    pub fn st(self) -> Vec2Q32 {
        self.xy()
    }

    #[inline(always)]
    pub fn ts(self) -> Vec2Q32 {
        self.yx()
    }

    #[inline(always)]
    pub fn tt(self) -> Vec2Q32 {
        self.yy()
    }

    /// Component-wise multiply
    #[inline(always)]
    pub fn mul_comp(self, rhs: Self) -> Self {
        Vec2Q32::new(self.x * rhs.x, self.y * rhs.y)
    }

    /// Component-wise divide
    #[inline(always)]
    pub fn div_comp(self, rhs: Self) -> Self {
        Vec2Q32::new(self.x / rhs.x, self.y / rhs.y)
    }

    /// Component-wise floor
    #[inline(always)]
    pub fn floor(self) -> Self {
        fns::floor_vec2(self)
    }

    /// Component-wise fractional part
    #[inline(always)]
    pub fn fract(self) -> Self {
        fns::fract_vec2(self)
    }

    /// Component-wise step function
    /// Returns 1.0 if edge <= x, else 0.0 for each component
    #[inline(always)]
    pub fn step(self, edge: Self) -> Self {
        fns::step_vec2(edge, self)
    }

    /// Component-wise minimum
    #[inline(always)]
    pub fn min(self, other: Self) -> Self {
        fns::min_vec2(self, other)
    }

    /// Component-wise maximum
    #[inline(always)]
    pub fn max(self, other: Self) -> Self {
        fns::max_vec2(self, other)
    }

    /// Component-wise modulo
    #[inline(always)]
    pub fn modulo(self, other: Self) -> Self {
        fns::mod_vec2(self, other)
    }

    /// Component-wise linear interpolation
    /// Returns a + t * (b - a) for each component
    #[inline(always)]
    pub fn mix(self, other: Self, t: Self) -> Self {
        fns::mix_vec2(self, other, t)
    }
}

impl Add for Vec2Q32 {
    type Output = Self;

    #[inline(always)]
    fn add(self, rhs: Self) -> Self {
        Vec2Q32::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl Sub for Vec2Q32 {
    type Output = Self;

    #[inline(always)]
    fn sub(self, rhs: Self) -> Self {
        Vec2Q32::new(self.x - rhs.x, self.y - rhs.y)
    }
}

// Vector * Scalar
impl Mul<Q32> for Vec2Q32 {
    type Output = Self;

    #[inline(always)]
    fn mul(self, rhs: Q32) -> Self {
        Vec2Q32::new(self.x * rhs, self.y * rhs)
    }
}

// Vector / Scalar
impl Div<Q32> for Vec2Q32 {
    type Output = Self;

    #[inline(always)]
    fn div(self, rhs: Q32) -> Self {
        Vec2Q32::new(self.x / rhs, self.y / rhs)
    }
}

impl Neg for Vec2Q32 {
    type Output = Self;

    #[inline(always)]
    fn neg(self) -> Self {
        Vec2Q32::new(-self.x, -self.y)
    }
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    extern crate std;
    use super::*;

    #[test]
    fn test_new() {
        let v = Vec2Q32::new(Q32::from_i32(1), Q32::from_i32(2));
        assert_eq!(v.x.to_f32(), 1.0);
        assert_eq!(v.y.to_f32(), 2.0);
    }

    #[test]
    fn test_from_f32() {
        let v = Vec2Q32::from_f32(1.0, 2.0);
        assert_eq!(v.x.to_f32(), 1.0);
        assert_eq!(v.y.to_f32(), 2.0);
    }

    #[test]
    fn test_from_i32() {
        let v = Vec2Q32::from_i32(5, 10);
        assert_eq!(v.x.to_f32(), 5.0);
        assert_eq!(v.y.to_f32(), 10.0);
    }

    #[test]
    fn test_zero_one() {
        let z = Vec2Q32::zero();
        assert_eq!(z.x.to_f32(), 0.0);
        assert_eq!(z.y.to_f32(), 0.0);

        let o = Vec2Q32::one();
        assert_eq!(o.x.to_f32(), 1.0);
        assert_eq!(o.y.to_f32(), 1.0);
    }

    #[test]
    fn test_add() {
        let a = Vec2Q32::from_f32(1.0, 2.0);
        let b = Vec2Q32::from_f32(3.0, 4.0);
        let c = a + b;
        assert_eq!(c.x.to_f32(), 4.0);
        assert_eq!(c.y.to_f32(), 6.0);
    }

    #[test]
    fn test_sub() {
        let a = Vec2Q32::from_f32(5.0, 7.0);
        let b = Vec2Q32::from_f32(1.0, 2.0);
        let c = a - b;
        assert_eq!(c.x.to_f32(), 4.0);
        assert_eq!(c.y.to_f32(), 5.0);
    }

    #[test]
    fn test_mul_scalar() {
        let v = Vec2Q32::from_f32(1.0, 2.0);
        let s = Q32::from_f32(2.0);
        let result = v * s;
        assert_eq!(result.x.to_f32(), 2.0);
        assert_eq!(result.y.to_f32(), 4.0);
    }

    #[test]
    fn test_div_scalar() {
        let v = Vec2Q32::from_f32(4.0, 6.0);
        let s = Q32::from_f32(2.0);
        let result = v / s;
        assert_eq!(result.x.to_f32(), 2.0);
        assert_eq!(result.y.to_f32(), 3.0);
    }

    #[test]
    fn test_neg() {
        let v = Vec2Q32::from_f32(5.0, -3.0);
        let neg = -v;
        assert_eq!(neg.x.to_f32(), -5.0);
        assert_eq!(neg.y.to_f32(), 3.0);
    }

    #[test]
    fn test_dot() {
        let a = Vec2Q32::from_f32(1.0, 2.0);
        let b = Vec2Q32::from_f32(3.0, 4.0);
        let dot = a.dot(b);
        // 1*3 + 2*4 = 3 + 8 = 11
        assert_eq!(dot.to_f32(), 11.0);
    }

    #[test]
    fn test_cross() {
        let a = Vec2Q32::from_f32(1.0, 0.0);
        let b = Vec2Q32::from_f32(0.0, 1.0);
        let cross = a.cross(b);
        // (1,0) × (0,1) = 1*1 - 0*0 = 1
        assert_eq!(cross.to_f32(), 1.0);
    }

    #[test]
    fn test_length_squared() {
        let v = Vec2Q32::from_f32(3.0, 4.0);
        let len_sq = v.length_squared();
        // 3^2 + 4^2 = 9 + 16 = 25
        assert_eq!(len_sq.to_f32(), 25.0);
    }

    #[test]
    fn test_length() {
        let v = Vec2Q32::from_f32(3.0, 4.0);
        let len = v.length();
        // Length should be 5
        assert!((len.to_f32() - 5.0).abs() < 0.01);
    }

    #[test]
    fn test_normalize() {
        let v = Vec2Q32::from_f32(3.0, 4.0);
        let n = v.normalize();

        // Check length is approximately 1
        let len = n.length();
        assert!((len.to_f32() - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_normalize_zero() {
        let v = Vec2Q32::zero();
        let n = v.normalize();
        // Should return zero vector, not panic
        assert_eq!(n.x.to_f32(), 0.0);
        assert_eq!(n.y.to_f32(), 0.0);
    }

    #[test]
    fn test_distance() {
        let a = Vec2Q32::from_f32(0.0, 0.0);
        let b = Vec2Q32::from_f32(3.0, 4.0);
        let dist = a.distance(b);
        // Distance should be 5
        assert!((dist.to_f32() - 5.0).abs() < 0.01);
    }

    #[test]
    fn test_mul_comp() {
        let a = Vec2Q32::from_f32(2.0, 3.0);
        let b = Vec2Q32::from_f32(5.0, 6.0);
        let c = a.mul_comp(b);
        assert_eq!(c.x.to_f32(), 10.0);
        assert_eq!(c.y.to_f32(), 18.0);
    }

    #[test]
    fn test_div_comp() {
        let a = Vec2Q32::from_f32(10.0, 18.0);
        let b = Vec2Q32::from_f32(2.0, 3.0);
        let c = a.div_comp(b);
        assert_eq!(c.x.to_f32(), 5.0);
        assert_eq!(c.y.to_f32(), 6.0);
    }

    #[test]
    fn test_swizzle_x() {
        let v = Vec2Q32::from_f32(1.0, 2.0);
        assert_eq!(v.x().to_f32(), 1.0);
        assert_eq!(v.y().to_f32(), 2.0);
        assert_eq!(v.r().to_f32(), 1.0);
        assert_eq!(v.g().to_f32(), 2.0);
        assert_eq!(v.s().to_f32(), 1.0);
        assert_eq!(v.t().to_f32(), 2.0);
    }

    #[test]
    fn test_swizzle_xx() {
        let v = Vec2Q32::from_f32(1.0, 2.0);
        let xx = v.xx();
        assert_eq!(xx.x.to_f32(), 1.0);
        assert_eq!(xx.y.to_f32(), 1.0);
    }

    #[test]
    fn test_swizzle_xy() {
        let v = Vec2Q32::from_f32(1.0, 2.0);
        let xy = v.xy();
        assert_eq!(xy.x.to_f32(), 1.0);
        assert_eq!(xy.y.to_f32(), 2.0);
    }

    #[test]
    fn test_swizzle_yx() {
        let v = Vec2Q32::from_f32(1.0, 2.0);
        let yx = v.yx();
        assert_eq!(yx.x.to_f32(), 2.0);
        assert_eq!(yx.y.to_f32(), 1.0);
    }

    #[test]
    fn test_swizzle_rgba() {
        let v = Vec2Q32::from_f32(1.0, 2.0);
        assert_eq!(v.rr().x.to_f32(), 1.0);
        assert_eq!(v.rg().x.to_f32(), 1.0);
        assert_eq!(v.rg().y.to_f32(), 2.0);
        assert_eq!(v.gr().x.to_f32(), 2.0);
        assert_eq!(v.gr().y.to_f32(), 1.0);
        assert_eq!(v.gg().x.to_f32(), 2.0);
        assert_eq!(v.gg().y.to_f32(), 2.0);
    }

    #[test]
    fn test_swizzle_stpq() {
        let v = Vec2Q32::from_f32(1.0, 2.0);
        assert_eq!(v.ss().x.to_f32(), 1.0);
        assert_eq!(v.st().x.to_f32(), 1.0);
        assert_eq!(v.st().y.to_f32(), 2.0);
        assert_eq!(v.ts().x.to_f32(), 2.0);
        assert_eq!(v.ts().y.to_f32(), 1.0);
        assert_eq!(v.tt().x.to_f32(), 2.0);
        assert_eq!(v.tt().y.to_f32(), 2.0);
    }
}
