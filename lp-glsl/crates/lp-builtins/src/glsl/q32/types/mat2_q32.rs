use core::ops::{Add, Div, Mul, Neg, Sub};

use crate::glsl::q32::types::q32::Q32;
use crate::glsl::q32::types::vec2_q32::Vec2Q32;

/// 2x2 matrix for Q32 fixed-point arithmetic (GLSL-compatible, column-major storage)
///
/// Storage layout (column-major):
/// [m00, m10, m01, m11]
/// Where m[row][col] represents the element at row `row` and column `col`
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Mat2Q32 {
    // Column-major storage: [col0, col1] where each column is [x, y]
    // Storage: [m00, m10, m01, m11]
    pub m: [Q32; 4],
}

impl Mat2Q32 {
    /// Create a new matrix from 4 Q32 values (column-major order)
    ///
    /// Parameters are in column-major order:
    /// m00, m10, m01, m11
    #[inline(always)]
    pub const fn new(m00: Q32, m10: Q32, m01: Q32, m11: Q32) -> Self {
        Mat2Q32 {
            m: [m00, m10, m01, m11],
        }
    }

    /// Create a matrix from 4 f32 values (column-major order)
    #[inline(always)]
    pub fn from_f32(m00: f32, m10: f32, m01: f32, m11: f32) -> Self {
        Mat2Q32::new(
            Q32::from_f32(m00),
            Q32::from_f32(m10),
            Q32::from_f32(m01),
            Q32::from_f32(m11),
        )
    }

    /// Create a matrix from 2 Vec2Q32 columns
    #[inline(always)]
    pub fn from_vec2(col0: Vec2Q32, col1: Vec2Q32) -> Self {
        Mat2Q32::new(col0.x, col0.y, col1.x, col1.y)
    }

    /// Create identity matrix
    #[inline(always)]
    pub const fn identity() -> Self {
        Mat2Q32::new(Q32::ONE, Q32::ZERO, Q32::ZERO, Q32::ONE)
    }

    /// Create zero matrix
    #[inline(always)]
    pub const fn zero() -> Self {
        Mat2Q32::new(Q32::ZERO, Q32::ZERO, Q32::ZERO, Q32::ZERO)
    }

    /// Get element at row `row` and column `col`
    #[inline(always)]
    pub fn get(self, row: usize, col: usize) -> Q32 {
        self.m[col * 2 + row]
    }

    /// Set element at row `row` and column `col`
    #[inline(always)]
    pub fn set(&mut self, row: usize, col: usize, value: Q32) {
        self.m[col * 2 + row] = value;
    }

    /// Get column 0 as Vec2Q32
    #[inline(always)]
    pub fn col0(self) -> Vec2Q32 {
        Vec2Q32::new(self.m[0], self.m[1])
    }

    /// Get column 1 as Vec2Q32
    #[inline(always)]
    pub fn col1(self) -> Vec2Q32 {
        Vec2Q32::new(self.m[2], self.m[3])
    }

    /// Matrix-matrix multiplication
    #[allow(clippy::should_implement_trait)]
    #[inline(always)]
    pub fn mul(self, rhs: Self) -> Self {
        let a = self;
        let b = rhs;
        Mat2Q32::new(
            // Row 0
            a.m[0] * b.m[0] + a.m[2] * b.m[1],
            a.m[1] * b.m[0] + a.m[3] * b.m[1],
            // Row 1
            a.m[0] * b.m[2] + a.m[2] * b.m[3],
            a.m[1] * b.m[2] + a.m[3] * b.m[3],
        )
    }

    /// Matrix-vector multiplication (mat2 * vec2)
    #[inline(always)]
    pub fn mul_vec2(self, v: Vec2Q32) -> Vec2Q32 {
        Vec2Q32::new(
            self.m[0] * v.x + self.m[2] * v.y,
            self.m[1] * v.x + self.m[3] * v.y,
        )
    }

    /// Transpose matrix
    #[inline(always)]
    pub fn transpose(self) -> Self {
        Mat2Q32::new(self.m[0], self.m[2], self.m[1], self.m[3])
    }

    /// Calculate determinant
    #[inline(always)]
    pub fn determinant(self) -> Q32 {
        // 2x2 determinant: m00*m11 - m01*m10
        (self.m[0] * self.m[3]) - (self.m[2] * self.m[1])
    }

    /// Calculate inverse matrix
    ///
    /// Returns None if matrix is singular (determinant is zero)
    #[inline(always)]
    pub fn inverse(self) -> Option<Self> {
        let det = self.determinant();
        if det.to_fixed() == 0 {
            return None;
        }

        // For 2x2: inverse = (1/det) * [m11, -m10, -m01, m00]
        let inv_det = Q32::ONE / det;
        Some(Mat2Q32::new(
            self.m[3] * inv_det,
            -self.m[1] * inv_det,
            -self.m[2] * inv_det,
            self.m[0] * inv_det,
        ))
    }
}

// Matrix + Matrix
impl Add for Mat2Q32 {
    type Output = Self;

    #[inline(always)]
    fn add(self, rhs: Self) -> Self {
        Mat2Q32::new(
            self.m[0] + rhs.m[0],
            self.m[1] + rhs.m[1],
            self.m[2] + rhs.m[2],
            self.m[3] + rhs.m[3],
        )
    }
}

// Matrix - Matrix
impl Sub for Mat2Q32 {
    type Output = Self;

    #[inline(always)]
    fn sub(self, rhs: Self) -> Self {
        Mat2Q32::new(
            self.m[0] - rhs.m[0],
            self.m[1] - rhs.m[1],
            self.m[2] - rhs.m[2],
            self.m[3] - rhs.m[3],
        )
    }
}

// Matrix * Matrix (matrix multiplication)
impl Mul for Mat2Q32 {
    type Output = Self;

    #[inline(always)]
    fn mul(self, rhs: Self) -> Self {
        self.mul(rhs)
    }
}

// Matrix * Vec2 (matrix-vector multiplication)
impl Mul<Vec2Q32> for Mat2Q32 {
    type Output = Vec2Q32;

    #[inline(always)]
    fn mul(self, rhs: Vec2Q32) -> Vec2Q32 {
        self.mul_vec2(rhs)
    }
}

// Matrix * Scalar
impl Mul<Q32> for Mat2Q32 {
    type Output = Self;

    #[inline(always)]
    fn mul(self, rhs: Q32) -> Self {
        Mat2Q32::new(
            self.m[0] * rhs,
            self.m[1] * rhs,
            self.m[2] * rhs,
            self.m[3] * rhs,
        )
    }
}

// Matrix / Scalar
impl Div<Q32> for Mat2Q32 {
    type Output = Self;

    #[inline(always)]
    fn div(self, rhs: Q32) -> Self {
        Mat2Q32::new(
            self.m[0] / rhs,
            self.m[1] / rhs,
            self.m[2] / rhs,
            self.m[3] / rhs,
        )
    }
}

// -Matrix
impl Neg for Mat2Q32 {
    type Output = Self;

    #[inline(always)]
    fn neg(self) -> Self {
        Mat2Q32::new(-self.m[0], -self.m[1], -self.m[2], -self.m[3])
    }
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    extern crate std;
    use super::*;

    #[test]
    fn test_identity() {
        let m = Mat2Q32::identity();
        assert_eq!(m.get(0, 0).to_f32(), 1.0);
        assert_eq!(m.get(1, 1).to_f32(), 1.0);
        assert_eq!(m.get(0, 1).to_f32(), 0.0);
        assert_eq!(m.get(1, 0).to_f32(), 0.0);
    }

    #[test]
    fn test_zero() {
        let m = Mat2Q32::zero();
        for i in 0..4 {
            assert_eq!(m.m[i].to_f32(), 0.0);
        }
    }

    #[test]
    fn test_new() {
        let m = Mat2Q32::new(
            Q32::from_i32(1),
            Q32::from_i32(2),
            Q32::from_i32(3),
            Q32::from_i32(4),
        );
        assert_eq!(m.get(0, 0).to_f32(), 1.0);
        assert_eq!(m.get(1, 0).to_f32(), 2.0);
        assert_eq!(m.get(0, 1).to_f32(), 3.0);
        assert_eq!(m.get(1, 1).to_f32(), 4.0);
    }

    #[test]
    fn test_from_f32() {
        let m = Mat2Q32::from_f32(1.0, 2.0, 3.0, 4.0);
        assert_eq!(m.get(0, 0).to_f32(), 1.0);
        assert_eq!(m.get(1, 0).to_f32(), 2.0);
        assert_eq!(m.get(0, 1).to_f32(), 3.0);
        assert_eq!(m.get(1, 1).to_f32(), 4.0);
    }

    #[test]
    fn test_from_vec2() {
        let col0 = Vec2Q32::from_f32(1.0, 2.0);
        let col1 = Vec2Q32::from_f32(3.0, 4.0);
        let m = Mat2Q32::from_vec2(col0, col1);
        assert_eq!(m.col0(), col0);
        assert_eq!(m.col1(), col1);
    }

    #[test]
    fn test_get_set() {
        let mut m = Mat2Q32::zero();
        m.set(0, 0, Q32::from_f32(5.0));
        assert_eq!(m.get(0, 0).to_f32(), 5.0);
    }

    #[test]
    fn test_col0_col1() {
        let m = Mat2Q32::from_f32(1.0, 2.0, 3.0, 4.0);
        let col0 = m.col0();
        assert_eq!(col0.x.to_f32(), 1.0);
        assert_eq!(col0.y.to_f32(), 2.0);
        let col1 = m.col1();
        assert_eq!(col1.x.to_f32(), 3.0);
        assert_eq!(col1.y.to_f32(), 4.0);
    }

    #[test]
    fn test_add() {
        let a = Mat2Q32::from_f32(1.0, 2.0, 3.0, 4.0);
        let b = Mat2Q32::from_f32(1.0, 1.0, 1.0, 1.0);
        let c = a + b;
        assert_eq!(c.get(0, 0).to_f32(), 2.0);
        assert_eq!(c.get(1, 1).to_f32(), 5.0);
    }

    #[test]
    fn test_sub() {
        let a = Mat2Q32::from_f32(5.0, 5.0, 5.0, 5.0);
        let b = Mat2Q32::from_f32(1.0, 1.0, 1.0, 1.0);
        let c = a - b;
        assert_eq!(c.get(0, 0).to_f32(), 4.0);
    }

    #[test]
    fn test_mul_matrix() {
        let a = Mat2Q32::identity();
        let b = Mat2Q32::identity();
        let c = a * b;
        assert_eq!(c, Mat2Q32::identity());
    }

    #[test]
    fn test_mul_vec2() {
        let m = Mat2Q32::identity();
        let v = Vec2Q32::from_f32(1.0, 2.0);
        let result = m * v;
        assert_eq!(result.x.to_f32(), 1.0);
        assert_eq!(result.y.to_f32(), 2.0);
    }

    #[test]
    fn test_mul_scalar() {
        let m = Mat2Q32::from_f32(1.0, 2.0, 3.0, 4.0);
        let s = Q32::from_f32(2.0);
        let result = m * s;
        assert_eq!(result.get(0, 0).to_f32(), 2.0);
        assert_eq!(result.get(1, 1).to_f32(), 8.0);
    }

    #[test]
    fn test_div_scalar() {
        let m = Mat2Q32::from_f32(4.0, 6.0, 8.0, 10.0);
        let s = Q32::from_f32(2.0);
        let result = m / s;
        assert_eq!(result.get(0, 0).to_f32(), 2.0);
        assert_eq!(result.get(1, 1).to_f32(), 5.0);
    }

    #[test]
    fn test_neg() {
        let m = Mat2Q32::from_f32(1.0, 2.0, 3.0, 4.0);
        let neg = -m;
        assert_eq!(neg.get(0, 0).to_f32(), -1.0);
        assert_eq!(neg.get(1, 1).to_f32(), -4.0);
    }

    #[test]
    fn test_transpose() {
        let m = Mat2Q32::from_f32(1.0, 2.0, 3.0, 4.0);
        let t = m.transpose();
        assert_eq!(t.get(0, 1).to_f32(), m.get(1, 0).to_f32());
        assert_eq!(t.get(1, 0).to_f32(), m.get(0, 1).to_f32());
    }

    #[test]
    fn test_determinant() {
        let m = Mat2Q32::identity();
        let det = m.determinant();
        assert!((det.to_f32() - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_determinant_specific() {
        let m = Mat2Q32::from_f32(1.0, 2.0, 3.0, 4.0);
        let det = m.determinant();
        // det = 1*4 - 3*2 = 4 - 6 = -2
        assert!((det.to_f32() - (-2.0)).abs() < 0.001);
    }

    #[test]
    fn test_inverse() {
        let m = Mat2Q32::identity();
        let inv = m.inverse().unwrap();
        assert_eq!(inv, Mat2Q32::identity());
    }

    #[test]
    fn test_inverse_singular() {
        let m = Mat2Q32::zero();
        assert_eq!(m.inverse(), None);
    }

    #[test]
    fn test_inverse_specific() {
        // Test with a specific matrix: [1, 2; 3, 4]
        // Inverse should be: (1/-2) * [4, -2; -3, 1] = [-2, 1; 1.5, -0.5]
        let m = Mat2Q32::from_f32(1.0, 2.0, 3.0, 4.0);
        let inv = m.inverse().unwrap();
        let product = m * inv;
        // Should be approximately identity
        assert!((product.get(0, 0).to_f32() - 1.0).abs() < 0.01);
        assert!((product.get(1, 1).to_f32() - 1.0).abs() < 0.01);
        assert!((product.get(0, 1).to_f32() - 0.0).abs() < 0.01);
        assert!((product.get(1, 0).to_f32() - 0.0).abs() < 0.01);
    }
}
