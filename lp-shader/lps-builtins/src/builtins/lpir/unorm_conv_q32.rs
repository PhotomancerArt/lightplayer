//! Q32 unorm channel conversions (`FtoUnorm16` / `FtoUnorm8` / `Unorm16toF` / `Unorm8toF`), matching
//! `lpvm-cranelift` Q32 lowerings.

/// Saturating Q32 fixed-point word → low 16 bits as unorm16 (0…65535).
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpir_fto_unorm16_q32(v: i32) -> i32 {
    v.max(0).min(65535)
}

/// Saturating Q32 fixed-point word → unorm8 (drops fractional precision below 8 bits).
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpir_fto_unorm8_q32(v: i32) -> i32 {
    (v >> 8).max(0).min(255)
}

/// Low 16 bits of `v` as Q32-encoded F32 lane (same bit pattern as [`super::ftoi_sat_q32`] fractional space).
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpir_unorm16_to_f_q32(v: i32) -> i32 {
    v & 0xFFFF
}

/// Low 8 bits of unorm8, shifted to Q16.16 fractional position, as Q32 F32 lane.
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpir_unorm8_to_f_q32(v: i32) -> i32 {
    (v & 0xFF) << 8
}
