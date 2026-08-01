//! Q32 float → integer (`FtoiSatS` / `FtoiSatU`), matching `lpvm-cranelift` `q32_emit` (`emit_to_sint` / `emit_to_uint`).

use lps_q32::q32_encode::Q32_FRAC;

#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpir_ftoi_sat_s_q32(v: i32) -> i32 {
    let biased_value = if v < 0 { v.wrapping_add(Q32_FRAC) } else { v };
    biased_value >> 16
}

#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpir_ftoi_sat_u_q32(v: i32) -> i32 {
    let t = __lp_lpir_ftoi_sat_s_q32(v);
    if t < 0 { 0 } else { t }
}
