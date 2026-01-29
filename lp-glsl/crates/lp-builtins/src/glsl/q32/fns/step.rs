use crate::glsl::q32::types::q32::Q32;
use crate::glsl::q32::types::vec2_q32::Vec2Q32;
use crate::glsl::q32::types::vec3_q32::Vec3Q32;
use crate::glsl::q32::types::vec4_q32::Vec4Q32;

/// Component-wise step function for Vec2Q32
/// Returns 1.0 if edge <= x, else 0.0 for each component
#[inline(always)]
pub fn step_vec2(edge: Vec2Q32, x: Vec2Q32) -> Vec2Q32 {
    Vec2Q32::new(
        if edge.x <= x.x { Q32::ONE } else { Q32::ZERO },
        if edge.y <= x.y { Q32::ONE } else { Q32::ZERO },
    )
}

/// Component-wise step function for Vec3Q32
/// Returns 1.0 if edge <= x, else 0.0 for each component
#[inline(always)]
pub fn step_vec3(edge: Vec3Q32, x: Vec3Q32) -> Vec3Q32 {
    Vec3Q32::new(
        if edge.x <= x.x { Q32::ONE } else { Q32::ZERO },
        if edge.y <= x.y { Q32::ONE } else { Q32::ZERO },
        if edge.z <= x.z { Q32::ONE } else { Q32::ZERO },
    )
}

/// Component-wise step function for Vec4Q32
/// Returns 1.0 if edge <= x, else 0.0 for each component
#[inline(always)]
pub fn step_vec4(edge: Vec4Q32, x: Vec4Q32) -> Vec4Q32 {
    Vec4Q32::new(
        if edge.x <= x.x { Q32::ONE } else { Q32::ZERO },
        if edge.y <= x.y { Q32::ONE } else { Q32::ZERO },
        if edge.z <= x.z { Q32::ONE } else { Q32::ZERO },
        if edge.w <= x.w { Q32::ONE } else { Q32::ZERO },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_step_vec2() {
        let edge = Vec2Q32::from_f32(0.5, 1.0);
        let x = Vec2Q32::from_f32(0.7, 0.3);
        let result = step_vec2(edge, x);
        assert_eq!(result.x.to_f32(), 1.0); // 0.5 <= 0.7
        assert_eq!(result.y.to_f32(), 0.0); // 1.0 > 0.3
    }

    #[test]
    fn test_step_vec3() {
        let edge = Vec3Q32::from_f32(0.5, 1.0, 0.0);
        let x = Vec3Q32::from_f32(0.7, 0.3, 0.1);
        let result = step_vec3(edge, x);
        assert_eq!(result.x.to_f32(), 1.0);
        assert_eq!(result.y.to_f32(), 0.0);
        assert_eq!(result.z.to_f32(), 1.0);
    }

    #[test]
    fn test_step_vec4() {
        let edge = Vec4Q32::from_f32(0.5, 1.0, 0.0, 2.0);
        let x = Vec4Q32::from_f32(0.7, 0.3, 0.1, 1.5);
        let result = step_vec4(edge, x);
        assert_eq!(result.x.to_f32(), 1.0);
        assert_eq!(result.y.to_f32(), 0.0);
        assert_eq!(result.z.to_f32(), 1.0);
        assert_eq!(result.w.to_f32(), 0.0);
    }
}
