use crate::glsl::q32::types::q32::Q32;
use crate::glsl::q32::types::vec2_q32::Vec2Q32;
use crate::glsl::q32::types::vec3_q32::Vec3Q32;
use crate::glsl::q32::types::vec4_q32::Vec4Q32;

/// Cubic polynomial smoothing for Q32
/// Returns v * v * (3.0 - 2.0 * v)
#[inline(always)]
pub fn cubic_q32(v: Q32) -> Q32 {
    let v2 = v * v;
    let three = Q32::from_f32(3.0);
    let two = Q32::from_f32(2.0);
    v2 * (three - two * v)
}

/// Component-wise cubic polynomial smoothing for Vec2Q32
#[inline(always)]
pub fn cubic_vec2(v: Vec2Q32) -> Vec2Q32 {
    Vec2Q32::new(cubic_q32(v.x), cubic_q32(v.y))
}

/// Component-wise cubic polynomial smoothing for Vec3Q32
#[inline(always)]
pub fn cubic_vec3(v: Vec3Q32) -> Vec3Q32 {
    Vec3Q32::new(cubic_q32(v.x), cubic_q32(v.y), cubic_q32(v.z))
}

/// Component-wise cubic polynomial smoothing for Vec4Q32
#[inline(always)]
pub fn cubic_vec4(v: Vec4Q32) -> Vec4Q32 {
    Vec4Q32::new(
        cubic_q32(v.x),
        cubic_q32(v.y),
        cubic_q32(v.z),
        cubic_q32(v.w),
    )
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    extern crate std;
    use super::*;

    #[test]
    fn test_cubic_q32() {
        let v0 = Q32::from_f32(0.0);
        let v1 = Q32::from_f32(1.0);
        let v05 = Q32::from_f32(0.5);

        assert!((cubic_q32(v0).to_f32() - 0.0).abs() < 0.01);
        assert!((cubic_q32(v1).to_f32() - 1.0).abs() < 0.01);
        // cubic(0.5) = 0.5 * 0.5 * (3 - 2*0.5) = 0.25 * 2 = 0.5
        assert!((cubic_q32(v05).to_f32() - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_cubic_vec2() {
        let v = Vec2Q32::from_f32(0.5, 0.25);
        let result = cubic_vec2(v);
        assert!((result.x.to_f32() - 0.5).abs() < 0.01);
        // cubic(0.25) = 0.25 * 0.25 * (3 - 2*0.25) = 0.0625 * 2.5 = 0.15625
        assert!((result.y.to_f32() - 0.15625).abs() < 0.01);
    }
}
