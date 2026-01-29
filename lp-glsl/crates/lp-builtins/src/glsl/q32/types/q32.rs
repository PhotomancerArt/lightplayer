/// Fixed-point arithmetic (16.16 format)
///
/// Core type and conversion utilities for fixed-point fixed.
use core::cmp::Ord;
use core::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Sub, SubAssign};

/// Fixed-point constants
const SHIFT: i32 = 16;
const ONE: i32 = 1 << SHIFT;
const HALF: i32 = ONE / 2;

/// Fixed-point number (Q16.16 format)
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Q32(pub i32);

impl Q32 {
    // 2π ≈ 6.28318530718 in 16.16
    pub const E: Q32 = Q32(178145);
    pub const HALF: Q32 = Q32(HALF);
    pub const ONE: Q32 = Q32(ONE);
    // e ≈ 2.71828182846 in 16.16
    pub const PHI: Q32 = Q32(106039);
    // Mathematical constants
    pub const PI: Q32 = Q32(205887);
    pub const SHIFT: i32 = SHIFT;
    // π ≈ 3.14159265359 in 16.16
    pub const TAU: Q32 = Q32(411774);
    pub const ZERO: Q32 = Q32(0);

    // φ ≈ 1.61803398875 in 16.16 (golden ratio)

    /// Create a Fixed from a raw fixed-point value
    #[inline(always)]
    pub const fn from_fixed(f: i32) -> Self {
        Q32(f)
    }

    /// Create a Fixed from an f32
    #[inline(always)]
    pub fn from_f32(f: f32) -> Self {
        Q32((f * ONE as f32) as i32)
    }

    /// Create a Fixed from an i32
    #[inline(always)]
    pub const fn from_i32(i: i32) -> Self {
        Q32(i << Self::SHIFT)
    }

    /// Convert to f32
    #[inline(always)]
    pub fn to_f32(self) -> f32 {
        self.0 as f32 / ONE as f32
    }

    /// Get the raw fixed-point value
    #[inline(always)]
    pub const fn to_fixed(self) -> i32 {
        self.0
    }

    /// Clamp value between min and max
    #[inline(always)]
    pub fn clamp(self, min: Q32, max: Q32) -> Q32 {
        Q32(self.0.clamp(min.0, max.0))
    }

    /// Return the maximum of two values
    #[inline(always)]
    pub fn max(self, other: Q32) -> Q32 {
        Q32(self.0.max(other.0))
    }

    /// Return the minimum of two values
    #[inline(always)]
    pub fn min(self, other: Q32) -> Q32 {
        Q32(self.0.min(other.0))
    }

    /// Return the absolute value
    #[inline(always)]
    pub fn abs(self) -> Q32 {
        Q32(self.0.abs())
    }

    /// Check if value is zero
    #[inline(always)]
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    /// Get the fractional part (0..1)
    #[inline(always)]
    pub const fn frac(self) -> Q32 {
        Q32(self.0 & (ONE - 1))
    }

    /// Get the integer part (floor)
    #[inline(always)]
    pub const fn to_i32(self) -> i32 {
        self.0 >> Self::SHIFT
    }

    /// Get the integer part (floor) as u8 clamped to [0, 255]
    ///
    /// Uses efficient bitwise operations:
    /// - Right shift to get integer part
    /// - Sign bit trick to clamp negative to 0: `value & !(value >> 31)`
    /// - Comparison trick to clamp > 255 to 255: `value & !((255 - value) >> 31) | 255 & ((255 - value) >> 31)`
    #[inline]
    pub fn to_u8_clamped(self) -> u8 {
        self.to_i32().clamp(0, 255) as u8
    }

    /// Multiply by an integer (more efficient than converting to Fixed first)
    #[inline]
    pub const fn mul_int(self, i: i32) -> Q32 {
        Q32(self.0 * i)
    }

    /// Linear interpolation
    /// Returns a + t * (b - a)
    #[inline]
    pub fn mix(self, other: Q32, t: Q32) -> Q32 {
        crate::glsl::q32::fns::mix_q32(self, other, t)
    }
}

impl Add for Q32 {
    type Output = Self;

    #[inline(always)]
    fn add(self, rhs: Self) -> Self {
        Q32(self.0 + rhs.0)
    }
}

impl Sub for Q32 {
    type Output = Self;

    #[inline(always)]
    fn sub(self, rhs: Self) -> Self {
        Q32(self.0 - rhs.0)
    }
}

impl Mul for Q32 {
    type Output = Self;

    #[inline(always)]
    fn mul(self, rhs: Self) -> Self {
        Q32(((self.0 as i64 * rhs.0 as i64) >> Self::SHIFT) as i32)
    }
}

impl Div for Q32 {
    type Output = Self;

    #[inline(always)]
    fn div(self, rhs: Self) -> Self {
        if rhs.0 != 0 {
            Q32(((self.0 as i64 * ONE as i64) / rhs.0 as i64) as i32)
        } else {
            Q32(0)
        }
    }
}

impl core::ops::Rem for Q32 {
    type Output = Self;

    #[inline(always)]
    fn rem(self, rhs: Self) -> Self {
        if rhs.0 != 0 {
            Q32(self.0 % rhs.0)
        } else {
            Q32(0)
        }
    }
}

impl Neg for Q32 {
    type Output = Self;

    #[inline(always)]
    fn neg(self) -> Self {
        Q32(-self.0)
    }
}

impl AddAssign for Q32 {
    #[inline(always)]
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
    }
}

impl SubAssign for Q32 {
    #[inline(always)]
    fn sub_assign(&mut self, rhs: Self) {
        self.0 -= rhs.0;
    }
}

impl MulAssign for Q32 {
    #[inline(always)]
    fn mul_assign(&mut self, rhs: Self) {
        *self = *self * rhs;
    }
}

impl DivAssign for Q32 {
    #[inline(always)]
    fn div_assign(&mut self, rhs: Self) {
        *self = *self / rhs;
    }
}

/// Trait for converting various types to Q32
pub trait ToQ32 {
    /// Convert to Q32 fixed-point value
    fn to_q32(self) -> Q32;
}

impl ToQ32 for i32 {
    #[inline(always)]
    fn to_q32(self) -> Q32 {
        Q32::from_i32(self)
    }
}

impl ToQ32 for i16 {
    #[inline(always)]
    fn to_q32(self) -> Q32 {
        Q32::from_i32(self as i32)
    }
}

impl ToQ32 for i8 {
    #[inline(always)]
    fn to_q32(self) -> Q32 {
        Q32::from_i32(self as i32)
    }
}

impl ToQ32 for u16 {
    #[inline(always)]
    fn to_q32(self) -> Q32 {
        Q32::from_i32(self as i32)
    }
}

impl ToQ32 for u8 {
    #[inline(always)]
    fn to_q32(self) -> Q32 {
        Q32::from_i32(self as i32)
    }
}

/// Extension trait for clamped conversions to Q32
pub trait ToQ32Clamped {
    /// Convert to Q32 with saturating arithmetic (clamps to maximum representable integer if value exceeds Q32 range)
    ///
    /// The maximum representable integer in Q32 format is `i32::MAX >> 16` (32767),
    /// since `from_i32` shifts left by 16 bits and must not overflow.
    fn to_q32_clamped(self) -> Q32;
}

impl ToQ32Clamped for u32 {
    #[inline(always)]
    fn to_q32_clamped(self) -> Q32 {
        const MAX_REPRESENTABLE: u32 = (i32::MAX >> Q32::SHIFT) as u32;
        if self <= MAX_REPRESENTABLE {
            Q32::from_i32(self as i32)
        } else {
            Q32::from_i32(MAX_REPRESENTABLE as i32)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constants() {
        assert_eq!(Q32::ZERO.to_f32(), 0.0);
        assert_eq!(Q32::ONE.to_f32(), 1.0);
        assert_eq!(Q32::HALF.to_f32(), 0.5);
    }

    #[test]
    fn test_from_i32() {
        assert_eq!(Q32::from_i32(5).to_f32(), 5.0);
        assert_eq!(Q32::from_i32(-3).to_f32(), -3.0);
        assert_eq!(Q32::from_i32(0).to_f32(), 0.0);
    }

    #[test]
    fn test_from_f32() {
        let f = Q32::from_f32(1.5);
        assert!((f.to_f32() - 1.5).abs() < 0.001);

        let f2 = Q32::from_f32(-2.75);
        assert!((f2.to_f32() - (-2.75)).abs() < 0.001);
    }

    #[test]
    fn test_add() {
        let a = Q32::from_i32(2);
        let b = Q32::from_i32(3);
        assert_eq!((a + b).to_f32(), 5.0);
    }

    #[test]
    fn test_sub() {
        let a = Q32::from_i32(5);
        let b = Q32::from_i32(3);
        assert_eq!((a - b).to_f32(), 2.0);
    }

    #[test]
    fn test_mul() {
        let a = Q32::from_i32(2);
        let b = Q32::from_i32(3);
        assert_eq!((a * b).to_f32(), 6.0);

        let c = Q32::from_f32(1.5);
        let d = Q32::from_f32(2.0);
        assert!((c * d).to_f32() - 3.0 < 0.01);
    }

    #[test]
    fn test_div() {
        let a = Q32::from_i32(6);
        let b = Q32::from_i32(2);
        assert_eq!((a / b).to_f32(), 3.0);

        let c = Q32::from_i32(3);
        let d = Q32::from_i32(2);
        assert!((c / d).to_f32() - 1.5 < 0.01);
    }

    #[test]
    fn test_neg() {
        let a = Q32::from_i32(5);
        assert_eq!((-a).to_f32(), -5.0);

        let b = Q32::from_i32(-3);
        assert_eq!((-b).to_f32(), 3.0);
    }

    #[test]
    fn test_clamp() {
        let val = Q32::from_i32(5);
        let min = Q32::from_i32(0);
        let max = Q32::from_i32(10);
        assert_eq!(val.clamp(min, max).to_f32(), 5.0);

        let val2 = Q32::from_i32(-5);
        assert_eq!(val2.clamp(min, max).to_f32(), 0.0);

        let val3 = Q32::from_i32(15);
        assert_eq!(val3.clamp(min, max).to_f32(), 10.0);
    }

    #[test]
    fn test_min_max() {
        let a = Q32::from_i32(5);
        let b = Q32::from_i32(10);
        assert_eq!(a.min(b).to_f32(), 5.0);
        assert_eq!(a.max(b).to_f32(), 10.0);
    }

    #[test]
    fn test_to_q32_i32() {
        assert_eq!(5i32.to_q32().to_f32(), 5.0);
        assert_eq!((-3i32).to_q32().to_f32(), -3.0);
        assert_eq!(0i32.to_q32().to_f32(), 0.0);
    }

    #[test]
    fn test_to_q32_i16() {
        assert_eq!(5i16.to_q32().to_f32(), 5.0);
        assert_eq!((-3i16).to_q32().to_f32(), -3.0);
    }

    #[test]
    fn test_to_q32_i8() {
        assert_eq!(5i8.to_q32().to_f32(), 5.0);
        assert_eq!((-3i8).to_q32().to_f32(), -3.0);
    }

    #[test]
    fn test_saturating_to_q32_u32() {
        const MAX_REPRESENTABLE: u32 = (i32::MAX >> Q32::SHIFT) as u32;
        const MAX_REPRESENTABLE_F32: f32 = MAX_REPRESENTABLE as f32;

        assert_eq!(5u32.to_q32_clamped().to_f32(), 5.0);
        assert_eq!(0u32.to_q32_clamped().to_f32(), 0.0);
        // Test that values at the maximum are preserved
        assert_eq!(
            MAX_REPRESENTABLE.to_q32_clamped().to_f32(),
            MAX_REPRESENTABLE_F32
        );
        // Test that values exceeding the maximum are clamped
        assert_eq!(
            (MAX_REPRESENTABLE + 1).to_q32_clamped().to_f32(),
            MAX_REPRESENTABLE_F32
        );
        assert_eq!(u32::MAX.to_q32_clamped().to_f32(), MAX_REPRESENTABLE_F32);
    }

    #[test]
    fn test_to_q32_u16() {
        assert_eq!(5u16.to_q32().to_f32(), 5.0);
        assert_eq!(0u16.to_q32().to_f32(), 0.0);
    }

    #[test]
    fn test_to_q32_u8() {
        assert_eq!(5u8.to_q32().to_f32(), 5.0);
        assert_eq!(0u8.to_q32().to_f32(), 0.0);
    }

    #[test]
    fn test_to_u8_clamping() {
        // Test values in range [0, 255]
        assert_eq!(Q32::from_i32(0).to_u8_clamped(), 0);
        assert_eq!(Q32::from_i32(100).to_u8_clamped(), 100);
        assert_eq!(Q32::from_i32(255).to_u8_clamped(), 255);
        assert_eq!(Q32::from_f32(128.5).to_u8_clamped(), 128);

        // Test values > 255 (should clamp to 255)
        assert_eq!(Q32::from_i32(256).to_u8_clamped(), 255);
        assert_eq!(Q32::from_i32(300).to_u8_clamped(), 255);
        assert_eq!(Q32::from_i32(1000).to_u8_clamped(), 255);
        assert_eq!(Q32::from_f32(300.7).to_u8_clamped(), 255);

        // Test negative values (should clamp to 0)
        assert_eq!(Q32::from_i32(-1).to_u8_clamped(), 0);
        assert_eq!(Q32::from_i32(-100).to_u8_clamped(), 0);
        assert_eq!(Q32::from_f32(-5.5).to_u8_clamped(), 0);

        // Test fractional values
        assert_eq!(Q32::from_f32(0.5).to_u8_clamped(), 0);
        assert_eq!(Q32::from_f32(0.9).to_u8_clamped(), 0);
        assert_eq!(Q32::from_f32(254.9).to_u8_clamped(), 254);
    }

    #[test]
    fn test_add_assign() {
        let mut a = Q32::from_i32(5);
        a += Q32::from_i32(3);
        assert_eq!(a.to_f32(), 8.0);

        let mut b = Q32::from_f32(1.5);
        b += Q32::from_f32(2.5);
        assert!((b.to_f32() - 4.0).abs() < 0.01);
    }

    #[test]
    fn test_sub_assign() {
        let mut a = Q32::from_i32(5);
        a -= Q32::from_i32(3);
        assert_eq!(a.to_f32(), 2.0);

        let mut b = Q32::from_f32(5.5);
        b -= Q32::from_f32(2.5);
        assert!((b.to_f32() - 3.0).abs() < 0.01);
    }

    #[test]
    fn test_mul_assign() {
        let mut a = Q32::from_i32(5);
        a *= Q32::from_i32(3);
        assert_eq!(a.to_f32(), 15.0);

        let mut b = Q32::from_f32(2.0);
        b *= Q32::from_f32(1.5);
        assert!((b.to_f32() - 3.0).abs() < 0.01);
    }

    #[test]
    fn test_div_assign() {
        let mut a = Q32::from_i32(15);
        a /= Q32::from_i32(3);
        assert_eq!(a.to_f32(), 5.0);

        let mut b = Q32::from_f32(6.0);
        b /= Q32::from_f32(2.0);
        assert!((b.to_f32() - 3.0).abs() < 0.01);
    }
}
