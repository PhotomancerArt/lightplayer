//! Signed integer → Q16.16 fixed-point (matches `lpvm-cranelift` `emit_from_sint`).

const Q32_SHIFT: u32 = 16;

/// Clamp to GLSL-int representable range in Q16.16, then shift.
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpir_itof_s_q32(x: i32) -> i32 {
    let clamped = x.clamp(-32768, 32767);
    clamped.wrapping_shl(Q32_SHIFT)
}
