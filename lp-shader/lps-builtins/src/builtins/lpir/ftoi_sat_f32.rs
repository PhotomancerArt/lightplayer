//! Native f32 → int with saturation — the f32 sibling of [`super::ftoi_sat_q32`].
//!
//! `docs/design/float.md` §3: float→int **truncates toward zero**, and finite
//! out-of-range values **saturate** to `i32::MIN`/`i32::MAX`. §5: the result of
//! converting NaN is Unspecified (targets natively produce `0`, `i32::MAX`, or
//! `i32::MIN`).
//!
//! Rust's `as` cast is exactly this contract — truncate, saturate the finite
//! range, NaN to zero — so it is the whole implementation. Do not "improve" it
//! into a checked conversion: the saturating behavior is the specified one.

/// f32 → signed i32: truncate toward zero, saturating.
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpir_ftoi_sat_s_f32(v: f32) -> i32 {
    v as i32
}

/// f32 → unsigned (returned in the i32 bit pattern GLSL `uint` uses):
/// truncate toward zero, saturating at 0 and `u32::MAX`.
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpir_ftoi_sat_u_f32(v: f32) -> i32 {
    (v as u32) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncates_toward_zero() {
        assert_eq!(__lp_lpir_ftoi_sat_s_f32(1.9), 1);
        assert_eq!(__lp_lpir_ftoi_sat_s_f32(-1.9), -1);
    }

    #[test]
    fn finite_out_of_range_saturates() {
        assert_eq!(__lp_lpir_ftoi_sat_s_f32(1e30), i32::MAX);
        assert_eq!(__lp_lpir_ftoi_sat_s_f32(-1e30), i32::MIN);
        assert_eq!(__lp_lpir_ftoi_sat_u_f32(-5.0), 0);
        assert_eq!(__lp_lpir_ftoi_sat_u_f32(1e30) as u32, u32::MAX);
    }

    #[test]
    fn range_reaches_far_past_q32() {
        // Q32 tops out near 32767; this is the capability f32 is being added for.
        assert_eq!(__lp_lpir_ftoi_sat_s_f32(1_000_000.0), 1_000_000);
    }
}
