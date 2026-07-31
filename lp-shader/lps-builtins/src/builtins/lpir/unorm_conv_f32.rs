//! Native f32 ↔ unorm8/unorm16 lane conversion — the f32 sibling of
//! [`super::unorm_conv_q32`].
//!
//! The Q32 versions are pure bit operations because Q16.16's fractional field
//! *is* a unorm16 code: `fto_unorm16` is a clamp of the raw word and
//! `unorm16_to_f` is a mask of it. Nothing about that is arbitrary — it fixes
//! the scale convention these builtins use, and the f32 versions must reproduce
//! the same convention or the same shader means two different things in the two
//! modes.
//!
//! The convention, spelled out:
//!
//! - **float → code** is `floor(value * 2^bits)` clamped to the code range, so
//!   `1.0` saturates to the top code rather than overflowing.
//! - **code → float** is `code / 2^bits`, so the top code maps to
//!   `65535/65536`, not to exactly `1.0`.
//!
//! That asymmetry is inherited, not chosen here. Changing it is a Q32 change
//! and out of scope for the f32 family.

use crate::f32_math;

/// f32 → unorm16 code: `floor(v * 65536)` clamped to `[0, 65535]`.
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpir_fto_unorm16_f32(v: f32) -> i32 {
    let scaled = f32_math::floor(v * 65536.0);
    if scaled <= 0.0 {
        0
    } else if scaled >= 65535.0 {
        65535
    } else {
        scaled as i32
    }
}

/// f32 → unorm8 code: `floor(v * 256)` clamped to `[0, 255]`.
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpir_fto_unorm8_f32(v: f32) -> i32 {
    let scaled = f32_math::floor(v * 256.0);
    if scaled <= 0.0 {
        0
    } else if scaled >= 255.0 {
        255
    } else {
        scaled as i32
    }
}

/// unorm16 code → f32: `code / 65536`.
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpir_unorm16_to_f_f32(v: i32) -> f32 {
    ((v & 0xFFFF) as f32) / 65536.0
}

/// unorm8 code → f32: `code / 256`.
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpir_unorm8_to_f_f32(v: i32) -> f32 {
    ((v & 0xFF) as f32) / 256.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn float_to_code_matches_the_q32_scale_convention() {
        assert_eq!(__lp_lpir_fto_unorm16_f32(0.0), 0);
        assert_eq!(__lp_lpir_fto_unorm16_f32(0.5), 32768);
        assert_eq!(__lp_lpir_fto_unorm16_f32(1.0), 65535);
        assert_eq!(__lp_lpir_fto_unorm8_f32(0.0), 0);
        assert_eq!(__lp_lpir_fto_unorm8_f32(0.5), 128);
        assert_eq!(__lp_lpir_fto_unorm8_f32(1.0), 255);
    }

    #[test]
    fn out_of_range_clamps_rather_than_wrapping() {
        assert_eq!(__lp_lpir_fto_unorm16_f32(-3.0), 0);
        assert_eq!(__lp_lpir_fto_unorm16_f32(9999.0), 65535);
        assert_eq!(__lp_lpir_fto_unorm8_f32(-0.001), 0);
        assert_eq!(__lp_lpir_fto_unorm8_f32(2.0), 255);
    }

    #[test]
    fn code_to_float_divides_by_the_power_of_two() {
        assert_eq!(__lp_lpir_unorm16_to_f_f32(0), 0.0);
        assert_eq!(__lp_lpir_unorm16_to_f_f32(32768), 0.5);
        assert_eq!(__lp_lpir_unorm8_to_f_f32(128), 0.5);
        // Top code is 65535/65536, not 1.0 — inherited from Q32.
        assert_eq!(__lp_lpir_unorm16_to_f_f32(65535), 65535.0 / 65536.0);
    }

    #[test]
    fn round_trips_every_unorm8_code() {
        for code in 0..=255i32 {
            let f = __lp_lpir_unorm8_to_f_f32(code);
            assert_eq!(__lp_lpir_fto_unorm8_f32(f), code, "code {code}");
        }
    }
}
