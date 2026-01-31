use crate::glsl::q32::types::q32::Q32;
use crate::glsl::q32::types::vec2_q32::Vec2Q32;
use crate::glsl::q32::types::vec3_q32::Vec3Q32;
use crate::glsl::q32::types::vec4_q32::Vec4Q32;

/// Component-wise floor for Vec2Q32
/// Returns a vector with floor applied to each component
#[inline(always)]
pub fn floor_vec2(v: Vec2Q32) -> Vec2Q32 {
    Vec2Q32::new(Q32::from_i32(v.x.to_i32()), Q32::from_i32(v.y.to_i32()))
}

/// Component-wise floor for Vec3Q32
/// Returns a vector with floor applied to each component
#[inline(always)]
pub fn floor_vec3(v: Vec3Q32) -> Vec3Q32 {
    Vec3Q32::new(
        Q32::from_i32(v.x.to_i32()),
        Q32::from_i32(v.y.to_i32()),
        Q32::from_i32(v.z.to_i32()),
    )
}

/// Component-wise floor for Vec4Q32
/// Returns a vector with floor applied to each component
#[inline(always)]
pub fn floor_vec4(v: Vec4Q32) -> Vec4Q32 {
    Vec4Q32::new(
        Q32::from_i32(v.x.to_i32()),
        Q32::from_i32(v.y.to_i32()),
        Q32::from_i32(v.z.to_i32()),
        Q32::from_i32(v.w.to_i32()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_floor_vec2() {
        let v = Vec2Q32::from_f32(1.7, -2.3);
        let result = floor_vec2(v);
        assert_eq!(result.x.to_f32(), 1.0);
        assert_eq!(result.y.to_f32(), -3.0);
    }

    #[test]
    fn test_floor_vec3() {
        let v = Vec3Q32::from_f32(1.7, -2.3, 5.9);
        let result = floor_vec3(v);
        assert_eq!(result.x.to_f32(), 1.0);
        assert_eq!(result.y.to_f32(), -3.0);
        assert_eq!(result.z.to_f32(), 5.0);
    }

    #[test]
    fn test_floor_vec4() {
        let v = Vec4Q32::from_f32(1.7, -2.3, 5.9, -0.1);
        let result = floor_vec4(v);
        assert_eq!(result.x.to_f32(), 1.0);
        assert_eq!(result.y.to_f32(), -3.0);
        assert_eq!(result.z.to_f32(), 5.0);
        assert_eq!(result.w.to_f32(), -1.0);
    }
}
