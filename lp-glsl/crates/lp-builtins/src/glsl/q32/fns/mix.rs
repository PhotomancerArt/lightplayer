use crate::glsl::q32::types::q32::Q32;
use crate::glsl::q32::types::vec2_q32::Vec2Q32;
use crate::glsl::q32::types::vec3_q32::Vec3Q32;
use crate::glsl::q32::types::vec4_q32::Vec4Q32;

/// Linear interpolation for Q32
/// Returns a + t * (b - a)
#[inline(always)]
pub fn mix_q32(a: Q32, b: Q32, t: Q32) -> Q32 {
    a + t * (b - a)
}

/// Component-wise linear interpolation for Vec2Q32
/// Returns a + t * (b - a) for each component
#[inline(always)]
pub fn mix_vec2(a: Vec2Q32, b: Vec2Q32, t: Vec2Q32) -> Vec2Q32 {
    Vec2Q32::new(mix_q32(a.x, b.x, t.x), mix_q32(a.y, b.y, t.y))
}

/// Component-wise linear interpolation for Vec3Q32
/// Returns a + t * (b - a) for each component
#[inline(always)]
pub fn mix_vec3(a: Vec3Q32, b: Vec3Q32, t: Vec3Q32) -> Vec3Q32 {
    Vec3Q32::new(
        mix_q32(a.x, b.x, t.x),
        mix_q32(a.y, b.y, t.y),
        mix_q32(a.z, b.z, t.z),
    )
}

/// Component-wise linear interpolation for Vec4Q32
/// Returns a + t * (b - a) for each component
#[inline(always)]
pub fn mix_vec4(a: Vec4Q32, b: Vec4Q32, t: Vec4Q32) -> Vec4Q32 {
    Vec4Q32::new(
        mix_q32(a.x, b.x, t.x),
        mix_q32(a.y, b.y, t.y),
        mix_q32(a.z, b.z, t.z),
        mix_q32(a.w, b.w, t.w),
    )
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    extern crate std;
    use super::*;

    #[test]
    fn test_mix_q32() {
        let a = Q32::from_f32(0.0);
        let b = Q32::from_f32(1.0);
        let t = Q32::from_f32(0.5);
        let result = mix_q32(a, b, t);
        assert!((result.to_f32() - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_mix_vec2() {
        let a = Vec2Q32::from_f32(0.0, 0.0);
        let b = Vec2Q32::from_f32(1.0, 2.0);
        let t = Vec2Q32::from_f32(0.5, 0.25);
        let result = mix_vec2(a, b, t);
        assert!((result.x.to_f32() - 0.5).abs() < 0.01);
        assert!((result.y.to_f32() - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_mix_vec3() {
        let a = Vec3Q32::from_f32(0.0, 0.0, 0.0);
        let b = Vec3Q32::from_f32(1.0, 2.0, 3.0);
        let t = Vec3Q32::from_f32(0.5, 0.25, 0.75);
        let result = mix_vec3(a, b, t);
        assert!((result.x.to_f32() - 0.5).abs() < 0.01);
        assert!((result.y.to_f32() - 0.5).abs() < 0.01);
        assert!((result.z.to_f32() - 2.25).abs() < 0.01);
    }

    #[test]
    fn test_mix_vec4() {
        let a = Vec4Q32::from_f32(0.0, 0.0, 0.0, 0.0);
        let b = Vec4Q32::from_f32(1.0, 2.0, 3.0, 4.0);
        let t = Vec4Q32::from_f32(0.5, 0.25, 0.75, 1.0);
        let result = mix_vec4(a, b, t);
        assert!((result.x.to_f32() - 0.5).abs() < 0.01);
        assert!((result.y.to_f32() - 0.5).abs() < 0.01);
        assert!((result.z.to_f32() - 2.25).abs() < 0.01);
        assert!((result.w.to_f32() - 4.0).abs() < 0.01);
    }
}
