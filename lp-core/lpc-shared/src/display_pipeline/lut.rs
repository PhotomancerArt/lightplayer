//! Lookup table for gamma and white point correction

/// Number of LUT entries (index 0..256)
pub const LUT_LEN: usize = 257;

/// Build LUT for one channel
///
/// Formula: lut[i] = clamp(round(pow((i/256) * white_point, lum_power) * 0xFFFF), 0, 0xFFFF)
pub fn build_lut(lut: &mut [u32; LUT_LEN], white_point: f32, lum_power: f32) {
    for i in 0..LUT_LEN {
        let normal = (i as f32 / 256.0) * white_point;
        let output = libm::powf(normal, lum_power) * 65535.0;
        let rounded = (output + 0.5) as i64;
        let clamped = rounded.clamp(0, 65535);
        lut[i] = clamped as u32;
    }
}

/// Interpolate 16-bit value through LUT
///
/// value is 16-bit (0..65535). index = value >> 8, alpha = value & 0xFF
/// Result: (lut[index] * (0x100 - alpha) + lut[index + 1] * alpha) >> 8
pub fn lut_interpolate(value: u32, lut: &[u32; LUT_LEN]) -> u32 {
    let value = value.min(65535);
    let index = (value >> 8) as usize;
    let alpha = value & 0xFF;
    let inv_alpha = 0x100 - alpha;
    (lut[index] * inv_alpha + lut[index.saturating_add(1).min(LUT_LEN - 1)] * alpha) >> 8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_lut_identity() {
        let mut lut = [0u32; LUT_LEN];
        build_lut(&mut lut, 1.0, 1.0);
        // Linear: output should roughly follow input
        assert_eq!(lut[0], 0);
        assert_eq!(lut[256], 65535);
    }

    #[test]
    fn lut_interpolate_zero() {
        let mut lut = [0u32; LUT_LEN];
        build_lut(&mut lut, 1.0, 1.0);
        assert_eq!(lut_interpolate(0, &lut), 0);
    }

    #[test]
    fn lut_interpolate_max() {
        let mut lut = [0u32; LUT_LEN];
        build_lut(&mut lut, 1.0, 1.0);
        assert!(lut_interpolate(0xFFFF, &lut) >= 65530);
    }
}
