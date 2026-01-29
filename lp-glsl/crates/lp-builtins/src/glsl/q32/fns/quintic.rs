use crate::glsl::q32::types::q32::Q32;
use crate::glsl::q32::types::vec2_q32::Vec2Q32;
use crate::glsl::q32::types::vec3_q32::Vec3Q32;
use crate::glsl::q32::types::vec4_q32::Vec4Q32;

/// Quintic polynomial smoothing for Q32
/// Returns v * v * v * (v * (v * 6.0 - 15.0) + 10.0)
#[inline(always)]
pub fn quintic_q32(v: Q32) -> Q32 {
    let v2 = v * v;
    let v3 = v2 * v;
    let six = Q32::from_f32(6.0);
    let fifteen = Q32::from_f32(15.0);
    let ten = Q32::from_f32(10.0);
    v3 * (v * (v * six - fifteen) + ten)
}

/// Component-wise quintic polynomial smoothing for Vec2Q32
#[inline(always)]
pub fn quintic_vec2(v: Vec2Q32) -> Vec2Q32 {
    Vec2Q32::new(quintic_q32(v.x), quintic_q32(v.y))
}

/// Component-wise quintic polynomial smoothing for Vec3Q32
#[inline(always)]
pub fn quintic_vec3(v: Vec3Q32) -> Vec3Q32 {
    Vec3Q32::new(quintic_q32(v.x), quintic_q32(v.y), quintic_q32(v.z))
}

/// Component-wise quintic polynomial smoothing for Vec4Q32
#[inline(always)]
pub fn quintic_vec4(v: Vec4Q32) -> Vec4Q32 {
    Vec4Q32::new(
        quintic_q32(v.x),
        quintic_q32(v.y),
        quintic_q32(v.z),
        quintic_q32(v.w),
    )
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    extern crate std;
    use super::*;

    #[test]
    fn test_quintic_q32() {
        let v0 = Q32::from_f32(0.0);
        let v1 = Q32::from_f32(1.0);
        let v05 = Q32::from_f32(0.5);

        assert!((quintic_q32(v0).to_f32() - 0.0).abs() < 0.01);
        assert!((quintic_q32(v1).to_f32() - 1.0).abs() < 0.01);
        // quintic(0.5) should be between 0 and 1
        let result = quintic_q32(v05).to_f32();
        assert!(result >= 0.0 && result <= 1.0);
    }

    #[test]
    fn test_quintic_vec2() {
        let v = Vec2Q32::from_f32(0.5, 0.25);
        let result = quintic_vec2(v);
        assert!(result.x.to_f32() >= 0.0 && result.x.to_f32() <= 1.0);
        assert!(result.y.to_f32() >= 0.0 && result.y.to_f32() <= 1.0);
    }
}
