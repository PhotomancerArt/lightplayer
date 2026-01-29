use crate::builtins::q32::__lp_q32_cos;
use crate::glsl::q32::types::q32::Q32;
use crate::glsl::q32::types::vec2_q32::Vec2Q32;
use crate::glsl::q32::types::vec3_q32::Vec3Q32;
use crate::glsl::q32::types::vec4_q32::Vec4Q32;

/// Component-wise cosine for Vec2Q32
/// Returns cos(x) for each component
#[inline(always)]
pub fn cos_vec2(v: Vec2Q32) -> Vec2Q32 {
    Vec2Q32::new(
        Q32::from_fixed(__lp_q32_cos(v.x.to_fixed())),
        Q32::from_fixed(__lp_q32_cos(v.y.to_fixed())),
    )
}

/// Component-wise cosine for Vec3Q32
/// Returns cos(x) for each component
#[inline(always)]
pub fn cos_vec3(v: Vec3Q32) -> Vec3Q32 {
    Vec3Q32::new(
        Q32::from_fixed(__lp_q32_cos(v.x.to_fixed())),
        Q32::from_fixed(__lp_q32_cos(v.y.to_fixed())),
        Q32::from_fixed(__lp_q32_cos(v.z.to_fixed())),
    )
}

/// Component-wise cosine for Vec4Q32
/// Returns cos(x) for each component
#[inline(always)]
pub fn cos_vec4(v: Vec4Q32) -> Vec4Q32 {
    Vec4Q32::new(
        Q32::from_fixed(__lp_q32_cos(v.x.to_fixed())),
        Q32::from_fixed(__lp_q32_cos(v.y.to_fixed())),
        Q32::from_fixed(__lp_q32_cos(v.z.to_fixed())),
        Q32::from_fixed(__lp_q32_cos(v.w.to_fixed())),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cos_vec2() {
        let v = Vec2Q32::from_f32(0.0, 1.5708); // 0, π/2
        let result = cos_vec2(v);
        assert!((result.x.to_f32() - 1.0).abs() < 0.01);
        assert!((result.y.to_f32() - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_cos_vec3() {
        let v = Vec3Q32::from_f32(0.0, 1.5708, 3.14159); // 0, π/2, π
        let result = cos_vec3(v);
        assert!((result.x.to_f32() - 1.0).abs() < 0.01);
        assert!((result.y.to_f32() - 0.0).abs() < 0.01);
        assert!((result.z.to_f32() - (-1.0)).abs() < 0.01);
    }

    #[test]
    fn test_cos_vec4() {
        let v = Vec4Q32::from_f32(0.0, 1.5708, 3.14159, 0.0);
        let result = cos_vec4(v);
        assert!((result.x.to_f32() - 1.0).abs() < 0.01);
        assert!((result.y.to_f32() - 0.0).abs() < 0.01);
        assert!((result.z.to_f32() - (-1.0)).abs() < 0.01);
        assert!((result.w.to_f32() - 1.0).abs() < 0.01);
    }
}
