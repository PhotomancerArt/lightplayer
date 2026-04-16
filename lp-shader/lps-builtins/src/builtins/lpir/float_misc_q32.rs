//! Q32 `fabs` / `fmin` / `fmax` / `ffloor` / `fceil` / `ftrunc`.
//!
//! Rounding ops match `lpvm-wasm` `emit_q32_ffloor` / `emit_q32_fceil` / `emit_q32_ftrunc`.

#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpir_fabs_q32(v: i32) -> i32 {
    if v < 0 { v.wrapping_neg() } else { v }
}

#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpir_fmin_q32(a: i32, b: i32) -> i32 {
    a.min(b)
}

#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpir_fmax_q32(a: i32, b: i32) -> i32 {
    a.max(b)
}

#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpir_ffloor_q32(v: i32) -> i32 {
    (v >> 16) << 16
}

#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpir_fceil_q32(v: i32) -> i32 {
    (v.wrapping_add(0xFFFF) >> 16) << 16
}

#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpir_ftrunc_q32(v: i32) -> i32 {
    let t = (v >> 16) << 16;
    if v != t && v < 0 {
        t.wrapping_add(1 << 16)
    } else {
        t
    }
}
