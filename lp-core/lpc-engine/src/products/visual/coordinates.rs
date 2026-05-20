//! Coordinate-space helpers for visual products.
//!
//! LightPlayer uses three visual coordinate spaces:
//!
//! - Fixture UV: authored device/layout positions normalized to `[0, 1]`.
//! - Shader pixel space: continuous pixel coordinates passed to `render(vec2 pos)`,
//!   with pixel centers at `x + 0.5`, `y + 0.5`.
//! - Texture UV: normalized coordinates used to sample a materialized texture.
//!
//! The direct-sampling buffer (`LpsSamplePointBuf`) carries shader pixel-space
//! coordinates encoded as Q16.16 integers. Texture sample batches carry texture UV
//! coordinates encoded as Q16.16 integers.

pub const Q16_ONE: i32 = 1 << 16;

#[must_use]
pub fn normalized_f32_to_q16(value: f32) -> i32 {
    let clamped = value.clamp(0.0, 1.0);
    (clamped * Q16_ONE as f32) as i32
}

#[must_use]
pub fn normalized_q16_to_pixel_q16(value: i32, extent: u32) -> i32 {
    let scaled = i64::from(value) * i64::from(extent);
    scaled.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

#[must_use]
pub fn pixel_q16_to_normalized_q16(coord: i32, extent: u32) -> i32 {
    if extent == 0 {
        return 0;
    }
    let normalized = i64::from(coord) / i64::from(extent);
    normalized.clamp(0, i64::from(Q16_ONE - 1)) as i32
}

#[must_use]
pub fn texel_center_to_uv_q16(texel: u32, extent: u32) -> i32 {
    if extent == 0 {
        return 0;
    }
    (((u64::from(texel)) * Q16_ONE as u64 + (Q16_ONE as u64 / 2)) / u64::from(extent)) as i32
}

#[must_use]
pub fn texture_uv_q16_to_texel(value: i32, extent: u32) -> u32 {
    if extent == 0 || value <= 0 {
        return 0;
    }
    let scaled = ((i64::from(value)) * i64::from(extent)) >> 16;
    u32::try_from(scaled).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalized_q16_scales_to_shader_pixel_space() {
        assert_eq!(normalized_q16_to_pixel_q16(0, 16), 0);
        assert_eq!(normalized_q16_to_pixel_q16(32768, 16), 8 * Q16_ONE);
        assert_eq!(normalized_q16_to_pixel_q16(Q16_ONE, 16), 16 * Q16_ONE);
    }

    #[test]
    fn pixel_q16_scales_to_normalized_texture_space() {
        assert_eq!(pixel_q16_to_normalized_q16(0, 16), 0);
        assert_eq!(pixel_q16_to_normalized_q16(8 * Q16_ONE, 16), 32768);
        assert_eq!(pixel_q16_to_normalized_q16(16 * Q16_ONE, 16), 65535);
    }

    #[test]
    fn texel_center_scales_each_axis_by_its_own_extent() {
        assert_eq!(texel_center_to_uv_q16(0, 2), 16384);
        assert_eq!(texel_center_to_uv_q16(1, 2), 49152);
        assert_eq!(texel_center_to_uv_q16(2, 4), 40960);
    }
}
