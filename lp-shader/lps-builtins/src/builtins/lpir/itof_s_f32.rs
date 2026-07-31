//! Signed int → f32 — the f32 sibling of [`super::itof_s_q32`].
//!
//! The Q32 sibling clamps to `[-32768, 32767]` before shifting, because that is
//! all Q16.16 can hold. **f32 does not clamp**: the whole point of the mode is
//! that the integer range survives. `docs/design/float.md` §3 makes int→float
//! a Guaranteed correctly-rounded (RNE) conversion, and `as f32` is that.

/// Signed i32 → f32, correctly rounded.
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpir_itof_s_f32(x: i32) -> f32 {
    x as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_for_small_integers() {
        assert_eq!(__lp_lpir_itof_s_f32(0), 0.0);
        assert_eq!(__lp_lpir_itof_s_f32(-42), -42.0);
    }

    #[test]
    fn does_not_clamp_where_q32_must() {
        assert_eq!(__lp_lpir_itof_s_f32(1_000_000), 1_000_000.0);
        assert_eq!(__lp_lpir_itof_s_f32(i32::MIN), -2_147_483_648.0);
    }

    #[test]
    fn rounds_to_nearest_even_past_24_bits() {
        // 2^24 + 1 is not representable; RNE picks the even neighbour.
        assert_eq!(__lp_lpir_itof_s_f32(16_777_217), 16_777_216.0);
    }
}
