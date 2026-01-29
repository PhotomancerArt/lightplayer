use crate::glsl::q32::types::vec2_q32::Vec2Q32;
use crate::glsl::q32::types::vec3_q32::Vec3Q32;
use crate::glsl::q32::types::vec4_q32::Vec4Q32;

/// Component-wise maximum for Vec2Q32
/// Returns the maximum of each component pair
#[inline(always)]
pub fn max_vec2(a: Vec2Q32, b: Vec2Q32) -> Vec2Q32 {
    Vec2Q32::new(a.x.max(b.x), a.y.max(b.y))
}

/// Component-wise maximum for Vec3Q32
/// Returns the maximum of each component pair
#[inline(always)]
pub fn max_vec3(a: Vec3Q32, b: Vec3Q32) -> Vec3Q32 {
    Vec3Q32::new(a.x.max(b.x), a.y.max(b.y), a.z.max(b.z))
}

/// Component-wise maximum for Vec4Q32
/// Returns the maximum of each component pair
#[inline(always)]
pub fn max_vec4(a: Vec4Q32, b: Vec4Q32) -> Vec4Q32 {
    Vec4Q32::new(a.x.max(b.x), a.y.max(b.y), a.z.max(b.z), a.w.max(b.w))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_max_vec2() {
        let a = Vec2Q32::from_f32(1.0, 3.0);
        let b = Vec2Q32::from_f32(2.0, 1.0);
        let result = max_vec2(a, b);
        assert_eq!(result.x.to_f32(), 2.0);
        assert_eq!(result.y.to_f32(), 3.0);
    }

    #[test]
    fn test_max_vec3() {
        let a = Vec3Q32::from_f32(1.0, 3.0, 5.0);
        let b = Vec3Q32::from_f32(2.0, 1.0, 4.0);
        let result = max_vec3(a, b);
        assert_eq!(result.x.to_f32(), 2.0);
        assert_eq!(result.y.to_f32(), 3.0);
        assert_eq!(result.z.to_f32(), 5.0);
    }

    #[test]
    fn test_max_vec4() {
        let a = Vec4Q32::from_f32(1.0, 3.0, 5.0, 7.0);
        let b = Vec4Q32::from_f32(2.0, 1.0, 4.0, 8.0);
        let result = max_vec4(a, b);
        assert_eq!(result.x.to_f32(), 2.0);
        assert_eq!(result.y.to_f32(), 3.0);
        assert_eq!(result.z.to_f32(), 5.0);
        assert_eq!(result.w.to_f32(), 8.0);
    }
}
