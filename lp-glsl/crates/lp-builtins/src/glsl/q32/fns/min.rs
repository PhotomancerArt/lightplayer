use crate::glsl::q32::types::vec2_q32::Vec2Q32;
use crate::glsl::q32::types::vec3_q32::Vec3Q32;
use crate::glsl::q32::types::vec4_q32::Vec4Q32;

/// Component-wise minimum for Vec2Q32
/// Returns the minimum of each component pair
#[inline(always)]
pub fn min_vec2(a: Vec2Q32, b: Vec2Q32) -> Vec2Q32 {
    Vec2Q32::new(a.x.min(b.x), a.y.min(b.y))
}

/// Component-wise minimum for Vec3Q32
/// Returns the minimum of each component pair
#[inline(always)]
pub fn min_vec3(a: Vec3Q32, b: Vec3Q32) -> Vec3Q32 {
    Vec3Q32::new(a.x.min(b.x), a.y.min(b.y), a.z.min(b.z))
}

/// Component-wise minimum for Vec4Q32
/// Returns the minimum of each component pair
#[inline(always)]
pub fn min_vec4(a: Vec4Q32, b: Vec4Q32) -> Vec4Q32 {
    Vec4Q32::new(a.x.min(b.x), a.y.min(b.y), a.z.min(b.z), a.w.min(b.w))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_min_vec2() {
        let a = Vec2Q32::from_f32(1.0, 3.0);
        let b = Vec2Q32::from_f32(2.0, 1.0);
        let result = min_vec2(a, b);
        assert_eq!(result.x.to_f32(), 1.0);
        assert_eq!(result.y.to_f32(), 1.0);
    }

    #[test]
    fn test_min_vec3() {
        let a = Vec3Q32::from_f32(1.0, 3.0, 5.0);
        let b = Vec3Q32::from_f32(2.0, 1.0, 4.0);
        let result = min_vec3(a, b);
        assert_eq!(result.x.to_f32(), 1.0);
        assert_eq!(result.y.to_f32(), 1.0);
        assert_eq!(result.z.to_f32(), 4.0);
    }

    #[test]
    fn test_min_vec4() {
        let a = Vec4Q32::from_f32(1.0, 3.0, 5.0, 7.0);
        let b = Vec4Q32::from_f32(2.0, 1.0, 4.0, 8.0);
        let result = min_vec4(a, b);
        assert_eq!(result.x.to_f32(), 1.0);
        assert_eq!(result.y.to_f32(), 1.0);
        assert_eq!(result.z.to_f32(), 4.0);
        assert_eq!(result.w.to_f32(), 7.0);
    }
}
