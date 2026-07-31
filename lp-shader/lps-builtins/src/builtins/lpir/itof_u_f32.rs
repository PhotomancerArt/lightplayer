//! Unsigned int → f32 — the f32 sibling of [`super::itof_u_q32`].
//!
//! `x` is a GLSL `uint` carried in an `i32` bit pattern, so it is reinterpreted
//! as `u32` before conversion — a negative `i32` here means a large `uint`, not
//! a negative number. The Q32 sibling clamps to 32767; f32 does not need to.

/// GLSL `uint` (as i32 bits) → f32, correctly rounded.
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpir_itof_u_f32(x: i32) -> f32 {
    (x as u32) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_values_are_exact() {
        assert_eq!(__lp_lpir_itof_u_f32(0), 0.0);
        assert_eq!(__lp_lpir_itof_u_f32(1234), 1234.0);
    }

    #[test]
    fn the_i32_bit_pattern_is_read_as_unsigned() {
        // -1 as i32 is 0xFFFF_FFFF, i.e. uint 4294967295 — not -1.0.
        assert_eq!(__lp_lpir_itof_u_f32(-1), 4_294_967_295.0);
    }
}
