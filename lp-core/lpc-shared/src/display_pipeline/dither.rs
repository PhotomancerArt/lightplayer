//! Temporal dithering for 16-bit to 8-bit conversion

/// Apply dithering: add overflow, round to u8, compute new overflow
///
/// interpolated: 16-bit value (0-65535)
/// overflow: current carry (i8)
/// Returns (output_u8, new_overflow)
#[inline]
pub fn dither_step(interpolated: i32, overflow: i8) -> (u8, i8) {
    let summed = interpolated + overflow as i32;
    let output = ((summed + 0x80) >> 8).clamp(0, 255) as u8;
    let new_overflow = (summed - (output as i32 * 257)) as i8;
    (output, new_overflow)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dither_step_zero() {
        let (out, overflow) = dither_step(0, 0);
        assert_eq!(out, 0);
        assert_eq!(overflow, 0);
    }

    #[test]
    fn dither_step_max() {
        let (out, _overflow) = dither_step(65535, 0);
        assert_eq!(out, 255);
    }

    #[test]
    fn dither_step_carry() {
        let (out1, overflow1) = dither_step(127, 0);
        assert_eq!(out1, 0);
        assert!(overflow1 != 0);
        let (out2, _) = dither_step(127, overflow1);
        assert!(out2 <= 1);
    }
}
