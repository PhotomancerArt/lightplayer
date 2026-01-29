use crate::glsl::q32::types::vec2_q32::Vec2Q32;
use crate::glsl::q32::types::vec3_q32::Vec3Q32;
use crate::glsl::q32::types::vec4_q32::Vec4Q32;

/// Component-wise fractional part for Vec2Q32
/// Returns x - floor(x) for each component
#[inline(always)]
pub fn fract_vec2(v: Vec2Q32) -> Vec2Q32 {
    Vec2Q32::new(v.x.frac(), v.y.frac())
}

/// Component-wise fractional part for Vec3Q32
/// Returns x - floor(x) for each component
#[inline(always)]
pub fn fract_vec3(v: Vec3Q32) -> Vec3Q32 {
    Vec3Q32::new(v.x.frac(), v.y.frac(), v.z.frac())
}

/// Component-wise fractional part for Vec4Q32
/// Returns x - floor(x) for each component
#[inline(always)]
pub fn fract_vec4(v: Vec4Q32) -> Vec4Q32 {
    Vec4Q32::new(v.x.frac(), v.y.frac(), v.z.frac(), v.w.frac())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fract_vec2() {
        let v = Vec2Q32::from_f32(1.7, -2.3);
        let result = fract_vec2(v);
        assert!((result.x.to_f32() - 0.7).abs() < 0.01);
        assert!((result.y.to_f32() - 0.7).abs() < 0.01); // fract(-2.3) = 0.7
    }

    #[test]
    fn test_fract_vec3() {
        let v = Vec3Q32::from_f32(1.7, -2.3, 5.9);
        let result = fract_vec3(v);
        assert!((result.x.to_f32() - 0.7).abs() < 0.01);
        assert!((result.y.to_f32() - 0.7).abs() < 0.01);
        assert!((result.z.to_f32() - 0.9).abs() < 0.01);
    }

    #[test]
    fn test_fract_vec4() {
        let v = Vec4Q32::from_f32(1.7, -2.3, 5.9, -0.1);
        let result = fract_vec4(v);
        assert!((result.x.to_f32() - 0.7).abs() < 0.01);
        assert!((result.y.to_f32() - 0.7).abs() < 0.01);
        assert!((result.z.to_f32() - 0.9).abs() < 0.01);
        assert!((result.w.to_f32() - 0.9).abs() < 0.01); // fract(-0.1) = 0.9
    }
}
