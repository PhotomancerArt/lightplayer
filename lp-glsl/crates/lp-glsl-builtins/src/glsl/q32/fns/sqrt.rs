use crate::builtins::q32::__lp_q32_sqrt;
use crate::glsl::q32::types::q32::Q32;
use crate::glsl::q32::types::vec2_q32::Vec2Q32;
use crate::glsl::q32::types::vec3_q32::Vec3Q32;
use crate::glsl::q32::types::vec4_q32::Vec4Q32;

/// Component-wise square root for Vec2Q32
/// Returns sqrt(x) for each component
#[inline(always)]
pub fn sqrt_vec2(v: Vec2Q32) -> Vec2Q32 {
    Vec2Q32::new(
        Q32::from_fixed(__lp_q32_sqrt(v.x.to_fixed())),
        Q32::from_fixed(__lp_q32_sqrt(v.y.to_fixed())),
    )
}

/// Component-wise square root for Vec3Q32
/// Returns sqrt(x) for each component
#[inline(always)]
pub fn sqrt_vec3(v: Vec3Q32) -> Vec3Q32 {
    Vec3Q32::new(
        Q32::from_fixed(__lp_q32_sqrt(v.x.to_fixed())),
        Q32::from_fixed(__lp_q32_sqrt(v.y.to_fixed())),
        Q32::from_fixed(__lp_q32_sqrt(v.z.to_fixed())),
    )
}

/// Component-wise square root for Vec4Q32
/// Returns sqrt(x) for each component
#[inline(always)]
pub fn sqrt_vec4(v: Vec4Q32) -> Vec4Q32 {
    Vec4Q32::new(
        Q32::from_fixed(__lp_q32_sqrt(v.x.to_fixed())),
        Q32::from_fixed(__lp_q32_sqrt(v.y.to_fixed())),
        Q32::from_fixed(__lp_q32_sqrt(v.z.to_fixed())),
        Q32::from_fixed(__lp_q32_sqrt(v.w.to_fixed())),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sqrt_vec2() {
        let v = Vec2Q32::from_f32(4.0, 9.0);
        let result = sqrt_vec2(v);
        assert!((result.x.to_f32() - 2.0).abs() < 0.01);
        assert!((result.y.to_f32() - 3.0).abs() < 0.01);
    }

    #[test]
    fn test_sqrt_vec3() {
        let v = Vec3Q32::from_f32(4.0, 9.0, 16.0);
        let result = sqrt_vec3(v);
        assert!((result.x.to_f32() - 2.0).abs() < 0.01);
        assert!((result.y.to_f32() - 3.0).abs() < 0.01);
        assert!((result.z.to_f32() - 4.0).abs() < 0.01);
    }

    #[test]
    fn test_sqrt_vec4() {
        let v = Vec4Q32::from_f32(4.0, 9.0, 16.0, 25.0);
        let result = sqrt_vec4(v);
        assert!((result.x.to_f32() - 2.0).abs() < 0.01);
        assert!((result.y.to_f32() - 3.0).abs() < 0.01);
        assert!((result.z.to_f32() - 4.0).abs() < 0.01);
        assert!((result.w.to_f32() - 5.0).abs() < 0.01);
    }
}
