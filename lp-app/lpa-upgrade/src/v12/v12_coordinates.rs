//! Format 12's per-render coordinate conversions, frozen (`lpc-engine`
//! `products/visual/coordinates.rs`, `nodes/fixture/fixture_node.rs`,
//! `nodes/shader/shader_node.rs` at `f90a0388a`).

/// Q16.16 one.
pub(crate) const Q16_ONE: i32 = 1 << 16;

/// Normalized `[0, 1]` f32 → Q16.16, truncating: exactly
/// `(value.clamp(0.0, 1.0) * 65536.0) as i32` for every bit pattern,
/// computed on the bit pattern (`normalized_f32_to_q16`).
pub(crate) fn normalized_f32_to_q16(value: f32) -> i32 {
    let bits = value.to_bits();
    if bits & 0x8000_0000 != 0 {
        return 0;
    }
    if bits > 0x7F80_0000 {
        return 0;
    }
    if bits >= 0x3F80_0000 {
        return Q16_ONE;
    }
    let exponent = (bits >> 23) as i32;
    if exponent == 0 {
        return 0;
    }
    let significand = (bits & 0x007F_FFFF) | 0x0080_0000;
    let shift = 134 - exponent;
    if shift >= 32 {
        0
    } else {
        (significand >> shift) as i32
    }
}

/// Normalized Q16.16 → pixel-space Q16.16 for a target `extent` pixels
/// wide, saturating (`normalized_q16_to_pixel_q16`).
pub(crate) fn normalized_q16_to_pixel_q16(value: i32, extent: u32) -> i32 {
    if (0..=Q16_ONE).contains(&value) && extent <= 32_767 {
        return value * extent as i32;
    }
    let scaled = i64::from(value) * i64::from(extent);
    scaled.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

/// Strip position of lamp `index` on a `count`-lamp wire, as a pixel-space
/// Q16 texel centre (`strip_point_coord`).
pub(crate) fn strip_point_coord(index: u32, count: u32, wire_reversed: bool) -> i32 {
    let index = if wire_reversed {
        count - 1 - index
    } else {
        index
    } as i32;
    (index << 16) + (Q16_ONE / 2)
}

/// Q16.16 pixel → normalized `f32` (`shader_node.rs`
/// `pixel_q16_to_normalized_f32`).
pub(crate) fn pixel_q16_to_normalized_f32(coord: i32, extent: u32) -> f32 {
    if extent == 0 {
        return 0.0;
    }
    (coord as f32 / Q16_ONE as f32) / extent as f32
}

/// Normalized `[0, 1]` → Q16.16 pixel (`shader_node.rs`
/// `normalized_f32_to_pixel_q16`).
pub(crate) fn normalized_f32_to_pixel_q16(value: f32, extent: u32) -> i32 {
    let pixels = value.clamp(0.0, 1.0) * extent as f32;
    (pixels * Q16_ONE as f32) as i32
}

/// Q16.16 → `f32` (`pattern_space.rs` `q16_to_f32`).
pub(crate) fn q16_to_f32(value: i32) -> f32 {
    value as f32 / Q16_ONE as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bit-pattern conversion is the float formula, on every interesting
    /// value.
    #[test]
    fn the_q16_conversion_is_the_float_formula() {
        for value in [
            -1.0f32, -0.0, 0.0, 1e-30, 0.25, 0.5, 0.999_999, 1.0, 2.0, 0.123_456,
        ] {
            assert_eq!(
                normalized_f32_to_q16(value),
                (value.clamp(0.0, 1.0) * 65536.0) as i32,
                "{value}"
            );
        }
        assert_eq!(normalized_f32_to_q16(f32::NAN), 0);
    }

    /// Lamp `k` of a strip sits on texel `k`'s centre, or `N − 1 − k`'s when
    /// the wire runs the other way.
    #[test]
    fn a_strip_lamp_sits_on_its_texel_centre() {
        assert_eq!(strip_point_coord(0, 4, false), Q16_ONE / 2);
        assert_eq!(strip_point_coord(0, 4, true), 3 * Q16_ONE + Q16_ONE / 2);
    }
}
