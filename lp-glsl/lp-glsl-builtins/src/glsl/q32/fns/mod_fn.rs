use crate::builtins::q32::__lp_q32_mod;
use crate::glsl::q32::types::q32::Q32;
use crate::glsl::q32::types::vec2_q32::Vec2Q32;
use crate::glsl::q32::types::vec3_q32::Vec3Q32;
use crate::glsl::q32::types::vec4_q32::Vec4Q32;

/// Component-wise modulo for Vec2Q32
/// Returns x mod y for each component
#[inline(always)]
pub fn mod_vec2(x: Vec2Q32, y: Vec2Q32) -> Vec2Q32 {
    Vec2Q32::new(
        Q32::from_fixed(__lp_q32_mod(x.x.to_fixed(), y.x.to_fixed())),
        Q32::from_fixed(__lp_q32_mod(x.y.to_fixed(), y.y.to_fixed())),
    )
}

/// Component-wise modulo for Vec3Q32
/// Returns x mod y for each component
#[inline(always)]
pub fn mod_vec3(x: Vec3Q32, y: Vec3Q32) -> Vec3Q32 {
    Vec3Q32::new(
        Q32::from_fixed(__lp_q32_mod(x.x.to_fixed(), y.x.to_fixed())),
        Q32::from_fixed(__lp_q32_mod(x.y.to_fixed(), y.y.to_fixed())),
        Q32::from_fixed(__lp_q32_mod(x.z.to_fixed(), y.z.to_fixed())),
    )
}

/// Component-wise modulo for Vec4Q32
/// Returns x mod y for each component
#[inline(always)]
pub fn mod_vec4(x: Vec4Q32, y: Vec4Q32) -> Vec4Q32 {
    Vec4Q32::new(
        Q32::from_fixed(__lp_q32_mod(x.x.to_fixed(), y.x.to_fixed())),
        Q32::from_fixed(__lp_q32_mod(x.y.to_fixed(), y.y.to_fixed())),
        Q32::from_fixed(__lp_q32_mod(x.z.to_fixed(), y.z.to_fixed())),
        Q32::from_fixed(__lp_q32_mod(x.w.to_fixed(), y.w.to_fixed())),
    )
}

/// Modulo with scalar for Vec3Q32
/// Returns x mod y for each component (y is scalar)
#[inline(always)]
pub fn mod_vec3_scalar(x: Vec3Q32, y: Q32) -> Vec3Q32 {
    let y_fixed = y.to_fixed();
    Vec3Q32::new(
        Q32::from_fixed(__lp_q32_mod(x.x.to_fixed(), y_fixed)),
        Q32::from_fixed(__lp_q32_mod(x.y.to_fixed(), y_fixed)),
        Q32::from_fixed(__lp_q32_mod(x.z.to_fixed(), y_fixed)),
    )
}

/// Modulo with scalar for Vec4Q32
/// Returns x mod y for each component (y is scalar)
#[inline(always)]
pub fn mod_vec4_scalar(x: Vec4Q32, y: Q32) -> Vec4Q32 {
    let y_fixed = y.to_fixed();
    Vec4Q32::new(
        Q32::from_fixed(__lp_q32_mod(x.x.to_fixed(), y_fixed)),
        Q32::from_fixed(__lp_q32_mod(x.y.to_fixed(), y_fixed)),
        Q32::from_fixed(__lp_q32_mod(x.z.to_fixed(), y_fixed)),
        Q32::from_fixed(__lp_q32_mod(x.w.to_fixed(), y_fixed)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mod_vec2() {
        let x = Vec2Q32::from_f32(7.0, 10.0);
        let y = Vec2Q32::from_f32(3.0, 4.0);
        let result = mod_vec2(x, y);
        assert!((result.x.to_f32() - 1.0).abs() < 0.01); // 7 mod 3 = 1
        assert!((result.y.to_f32() - 2.0).abs() < 0.01); // 10 mod 4 = 2
    }

    #[test]
    fn test_mod_vec3() {
        let x = Vec3Q32::from_f32(7.0, 10.0, 15.0);
        let y = Vec3Q32::from_f32(3.0, 4.0, 5.0);
        let result = mod_vec3(x, y);
        assert!((result.x.to_f32() - 1.0).abs() < 0.01);
        assert!((result.y.to_f32() - 2.0).abs() < 0.01);
        assert!((result.z.to_f32() - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_mod_vec4() {
        let x = Vec4Q32::from_f32(7.0, 10.0, 15.0, 22.0);
        let y = Vec4Q32::from_f32(3.0, 4.0, 5.0, 7.0);
        let result = mod_vec4(x, y);
        assert!((result.x.to_f32() - 1.0).abs() < 0.01);
        assert!((result.y.to_f32() - 2.0).abs() < 0.01);
        assert!((result.z.to_f32() - 0.0).abs() < 0.01);
        assert!((result.w.to_f32() - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_mod_vec3_scalar() {
        let x = Vec3Q32::from_f32(7.0, 10.0, 15.0);
        let y = Q32::from_f32(3.0);
        let result = mod_vec3_scalar(x, y);
        assert!((result.x.to_f32() - 1.0).abs() < 0.01);
        assert!((result.y.to_f32() - 1.0).abs() < 0.01); // 10 mod 3 = 1
        assert!((result.z.to_f32() - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_mod_vec4_scalar() {
        let x = Vec4Q32::from_f32(7.0, 10.0, 15.0, 22.0);
        let y = Q32::from_f32(3.0);
        let result = mod_vec4_scalar(x, y);
        assert!((result.x.to_f32() - 1.0).abs() < 0.01);
        assert!((result.y.to_f32() - 1.0).abs() < 0.01);
        assert!((result.z.to_f32() - 0.0).abs() < 0.01);
        assert!((result.w.to_f32() - 1.0).abs() < 0.01); // 22 mod 3 = 1
    }
}
